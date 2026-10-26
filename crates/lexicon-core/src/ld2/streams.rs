//! streams 表收集与 zlib inflate。
//!
//! Compressed Data Header@offsetCDH：+0x08 起 streams table（累积 end-offset u32），
//! 终止条件 `prev + pos < limit`；表结束后是连续 zlib-wrapped deflate 块。
//!
//! 该终止条件是原始 Python 实现实测得出的启发式，必须逐字保留，否则会在
//! 某些词典上多读/少读一个 stream。

use crate::error::{Error, Result};

/// 返回 (streams 累积 end-offset 列表, 压缩数据起始位置)。
pub fn collect_streams(data: &[u8], offset_cdh: u64, limit: u64) -> Result<(Vec<u32>, usize)> {
    let mut pos =
        usize::try_from(offset_cdh).map_err(|_| Error::Ld2("offsetCDH out of range".into()))? + 8;
    let mut prev = read_u32(data, pos)?;
    pos += 4;
    let mut streams = Vec::new();
    while (prev as u64) + (pos as u64) < limit {
        let cur = read_u32(data, pos)?;
        streams.push(cur);
        pos += 4;
        prev = cur;
    }
    Ok((streams, pos))
}

#[inline]
fn read_u32(data: &[u8], pos: usize) -> Result<u32> {
    let s = data
        .get(pos..pos + 4)
        .ok_or_else(|| Error::Ld2(format!("streams table out of bounds at 0x{pos:x}")))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// 逐 stream zlib 解压并顺序拼接成单一 buffer。
pub fn inflate_all(data: &[u8], streams: &[u32], compressed_start: usize) -> Result<Vec<u8>> {
    use std::io::Read;

    let mut out = Vec::new();
    let mut last: usize = 0;
    for &end_rel in streams {
        let end = compressed_start
            .checked_add(end_rel as usize)
            .ok_or_else(|| Error::Ld2("stream offset overflow".into()))?;
        if end > data.len() || end < compressed_start + last {
            return Err(Error::Ld2(format!(
                "stream range 0x{compressed_start:x}+{last}..0x{end:x} out of bounds"
            )));
        }
        let chunk = &data[compressed_start + last..end];
        let mut dec = flate2::read::ZlibDecoder::new(chunk);
        dec.read_to_end(&mut out)
            .map_err(|e| Error::Ld2(format!("zlib: {e}")))?;
        last = end_rel as usize;
    }
    Ok(out)
}
