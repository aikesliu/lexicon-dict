//! ld2 文件头解析（0x00-0x60 明文区）。

use crate::error::{Error, Result};

pub const OFFSET_DATASUB: usize = 0x5C;
pub const HEADER_FIXED_BASE: u64 = 0x60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version_major: u16,
    pub version_minor: u16,
    /// Dictionary Header 起始绝对偏移 = offsetDataSub + 0x60。
    pub offset_data: u64,
}

#[inline]
fn u16_at(data: &[u8], off: usize) -> Result<u16> {
    let s = data
        .get(off..off + 2)
        .ok_or_else(|| Error::Ld2(format!("truncated header at 0x{off:x}")))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

#[inline]
pub fn u32_at(data: &[u8], off: usize) -> Result<u32> {
    let s = data
        .get(off..off + 4)
        .ok_or_else(|| Error::Ld2(format!("truncated u32 at 0x{off:x}")))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn parse_header(data: &[u8]) -> Result<Header> {
    if data.len() < HEADER_FIXED_BASE as usize {
        return Err(Error::Ld2("file shorter than 0x60 header".into()));
    }
    let version_major = u16_at(data, 0x18)?;
    let version_minor = u16_at(data, 0x1A)?;
    let offset_data_sub = u32_at(data, OFFSET_DATASUB)? as u64;
    Ok(Header {
        version_major,
        version_minor,
        offset_data: offset_data_sub + HEADER_FIXED_BASE,
    })
}
