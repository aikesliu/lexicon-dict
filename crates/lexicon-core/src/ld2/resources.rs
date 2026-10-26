//! ld2 资源段（trailing resources）解析：提取内嵌图片等资源。
//!
//! 资源段紧跟字典数据之后，位于文件 `[limit, filesize)` 区间（limit = Dictionary Header
//! 算出的压缩区结束绝对偏移）。多数词典无此段（limit == filesize）。
//!
//! 格式（小端，逆向 + 端到端提取验证通过）：
//!   资源头 9×u32 @ limit：
//!     [0] type(=4)            [1] data_size          [2] pairs_table_size
//!     [3] names_pool_len      [4] data_pool_len      [5] _(compressed_size)
//!     [6] blob_total(=校验)   [7] chunk_size(16384)  [8] 0(streams 表首 prev)
//!   streams 表 @ limit+32：累积压缩 end-offset（相对 compressed_start），collect_streams
//!     以 offset_cdh=limit+24 使表从 [8]=0 起读，filesize 终止。
//!   表后连续 zlib 块 → inflate_all 拼成 blob：
//!     blob = [pairs 表 8×(N+1)] [names 池] [data 池]
//!     record i: name = names_pool[A[i]:A[i+1]]，data = data_pool[B[i]:B[i+1]]
//!       其中 A[i]=u32@blob[8i]、B[i]=u32@blob[8i+4]（累积尾偏移，

use std::collections::BTreeMap;

use super::header::u32_at;
use super::streams::{collect_streams, inflate_all};

/// 资源头里 streams 表逻辑起点与 `limit` 的差（collect_streams 的 +8 跳过 [6][7]）。
const RES_OFFSET_CDH_SUB: u64 = 24;

pub fn extract_resources(data: &[u8], limit: u64, filesize: u64) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    if limit >= filesize {
        return out;
    }
    let region = filesize - limit;
    if region < 36 {
        return out;
    }
    let limit_usize = match usize::try_from(limit) {
        Ok(v) => v,
        Err(_) => return out,
    };
    let mut fld = [0u32; 9];
    for (i, slot) in fld.iter_mut().enumerate() {
        match u32_at(data, limit_usize + 4 * i) {
            Ok(v) => *slot = v,
            Err(_) => return out,
        }
    }
    let rtype = fld[0];
    let data_size = fld[1] as u64;
    let pairs_sz = fld[2] as usize;
    let names_len = fld[3] as usize;
    let data_len = fld[4] as usize;
    let blob_total = fld[6] as usize;

    // 硬门控：资源段 type 必须 LE==4。American Heritage 尾部是 BE 的异构结构。
    if rtype != 4 {
        return out;
    }
    if limit + 8 + data_size != filesize {
        return out;
    }
    if blob_total != pairs_sz + names_len + data_len {
        return out;
    }

    let (streams, compressed_start) =
        match collect_streams(data, limit + RES_OFFSET_CDH_SUB, filesize) {
            Ok(v) => v,
            Err(_) => return out,
        };
    let blob = match inflate_all(data, &streams, compressed_start) {
        Ok(b) => b,
        Err(_) => return out,
    };
    if blob.len() != blob_total {
        return out;
    }
    if pairs_sz % 8 != 0 {
        return out;
    }
    let n_rec = pairs_sz / 8; // N+1 条（含哨兵）
    if n_rec < 1 {
        return out;
    }
    let names_base = pairs_sz;
    let data_base = pairs_sz + names_len;
    let mut a = vec![0u32; n_rec];
    let mut b = vec![0u32; n_rec];
    for i in 0..n_rec {
        match (u32_at(&blob, 8 * i), u32_at(&blob, 8 * i + 4)) {
            (Ok(x), Ok(y)) => {
                a[i] = x;
                b[i] = y;
            }
            _ => return out,
        }
    }
    for i in 0..n_rec.saturating_sub(1) {
        let (na, nb) = (a[i] as usize, a[i + 1] as usize);
        let (da, db) = (b[i] as usize, b[i + 1] as usize);
        if nb <= na || db <= da {
            continue;
        }
        let name_end = names_base + nb;
        let data_end = data_base + db;
        if name_end > blob.len() || data_end > blob.len() {
            continue;
        }
        let name_bytes = &blob[names_base + na..name_end];
        let name: String = name_bytes
            .iter()
            .map(|&c| if c.is_ascii() { c as char } else { '\u{FFFD}' })
            .collect();
        if name.is_empty() {
            continue;
        }
        out.insert(name, blob[data_base + da..data_end].to_vec());
    }
    out
}
