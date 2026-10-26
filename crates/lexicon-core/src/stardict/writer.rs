//! StarDict 写入器：把 (word, definition) 序列写成 .ifo/.idx/.dict。
//!
//! 目标 StarDict 3.0.6，sametypesequence=h（释义为 HTML 文本，LD2 XML 视作 HTML 子集，
//! 渲染端清洗）。.idx 每条 record = word\0 + offset(BE) + size(BE)；wordcount > 50 万启用
//! idxoffsetbits=64（offset/size 各 u64 BE）。

use std::io::Write;
use std::path::Path;

use crate::error::{Error, Result};

pub const IFO_VERSION: &str = "3.0.6";
pub const LARGE_THRESHOLD: usize = 500_000;

#[derive(Debug, Clone, Copy)]
pub struct WriteStats {
    pub wordcount: usize,
    pub idxfilesize: usize,
    pub dict_size: u64,
    pub idxoffsetbits: u32,
}

/// 写入 StarDict 词典文件，返回统计信息。
///
/// 空 word 或含 `\0` 的条目会被丢弃；按 UTF-8 字节序排序（StarDict 要求 idx 字典序）。
pub fn write_stardict<I>(
    out_dir: &Path,
    slug: &str,
    bookname: &str,
    entries: I,
    idx_offset_bits: Option<u32>,
) -> Result<WriteStats>
where
    I: IntoIterator<Item = (String, String)>,
{
    use std::io::BufWriter;

    let out_root = out_dir.join(slug);
    std::fs::create_dir_all(&out_root)?;
    let base = out_root.join(slug);

    let mut items: Vec<(String, String)> = entries
        .into_iter()
        .filter(|(w, _)| !w.is_empty() && !w.contains('\0'))
        .collect();
    items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let bits = idx_offset_bits.unwrap_or(if items.len() > LARGE_THRESHOLD {
        64
    } else {
        32
    });

    let dict_path = base.with_extension("dict");
    let mut idx_buf: Vec<u8> = Vec::new();
    let mut dict_size: u64 = 0;
    {
        let df = std::fs::File::create(&dict_path)?;
        let mut dw = BufWriter::new(df);
        for (word, defn) in &items {
            let db = defn.as_bytes();
            dw.write_all(db)?;
            idx_buf.extend_from_slice(word.as_bytes());
            idx_buf.push(0);
            if bits == 64 {
                idx_buf.extend_from_slice(&dict_size.to_be_bytes());
                idx_buf.extend_from_slice(&(db.len() as u64).to_be_bytes());
            } else {
                idx_buf.extend_from_slice(&(dict_size as u32).to_be_bytes());
                idx_buf.extend_from_slice(&(db.len() as u32).to_be_bytes());
            }
            dict_size += db.len() as u64;
        }
        dw.flush()?;
    }
    std::fs::write(base.with_extension("idx"), &idx_buf)?;

    let mut ifo = String::new();
    ifo.push_str("StarDict's dict ifo file\n");
    ifo.push_str(&format!("version={IFO_VERSION}\n"));
    ifo.push_str(&format!("bookname={bookname}\n"));
    ifo.push_str(&format!("wordcount={}\n", items.len()));
    ifo.push_str(&format!("idxfilesize={}\n", idx_buf.len()));
    ifo.push_str("sametypesequence=h\n");
    if bits == 64 {
        ifo.push_str("idxoffsetbits=64\n");
    }
    std::fs::write(base.with_extension("ifo"), ifo)?;

    Ok(WriteStats {
        wordcount: items.len(),
        idxfilesize: idx_buf.len(),
        dict_size,
        idxoffsetbits: bits,
    })
}

/// 读取 `.ifo` 的原始文本（测试/校验用）。
pub fn read_ifo_text(ifo_path: &Path) -> Result<String> {
    std::fs::read_to_string(ifo_path).map_err(Error::Io)
}

#[allow(dead_code)]
fn _assert_rec_hdr(bits: u32) -> usize {
    if bits == 64 { 16 } else { 8 }
}
