//! ld2 Dictionary：组合 header/streams/encoding，提供 entry 迭代。
//!
//! Word Index 每条 10 字节：u32 lastWordPos, u32 lastXmlPos, u8 flags, u8 crossRefs。
//! entry_count = (idx_len / 10) - 1（末尾 1 个 slot 为哨兵，存最后一条的结束 offset）。
//! crossRefs > 0 时，Phrases 区域前 refs*4 字节是 u32 引用数组，其后才是 word 文本；
//! 该 entry 自身 XML 为空，释义由被引用 entry 的 XML 拼接而成。

use std::path::{Path, PathBuf};

use memmap2::Mmap;

use super::encoding::{Encoding, decode_with, detect_encoding};
use super::header::{Header, parse_header, u32_at};
use super::resources::extract_resources;
use super::streams::{collect_streams, inflate_all};
use crate::error::{Error, Result};

pub const ENTRY_SIZE: usize = 10;
pub const ENCODING_SAMPLE: usize = 65536;
pub const MAGIC_LD2: [u8; 4] = *b"?LD2";
pub const MAGIC_LDF: [u8; 4] = *b"?LDF";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub word_pos: u32,
    pub xml_pos: u32,
    pub flags: u8,
    pub refs: u8,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub word: String,
    /// 解码后的 XML/HTML 释义。
    pub definition: String,
    pub cross_refs: Vec<u32>,
    pub flags: u8,
    pub index: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Ld2Header {
    pub dict_type: u32,
    pub limit: u64,
    pub offset_cdh: u64,
    pub idx_len: u64,
    pub phr_len: u64,
    pub xml_len: u64,
}

/// 已解析的 ld2 词典。mmap 源文件，inflated buffer 常驻内存（仅转换期）。
pub struct Dictionary {
    pub source_path: PathBuf,
    data: Mmap,
    inflated: Vec<u8>,
    dh: Ld2Header,
    header: Header,
    pub word_encoding: Encoding,
    pub def_encoding: Encoding,
    pub entry_count: usize,
    off_phr: usize,
    off_xml: usize,
}

fn file_len(data: &[u8]) -> u64 {
    data.len() as u64
}

fn parse_dict_header(data: &[u8], offset_data: u64) -> Result<(Ld2Header, u64)> {
    let mut off =
        usize::try_from(offset_data).map_err(|_| Error::Ld2("offsetData out of range".into()))?;
    let mut dt = u32_at(data, off)?;
    if dt != 3 {
        let rel = u32_at(data, off + 4)? as u64;
        off = usize::try_from(rel + offset_data + 0x0C)
            .map_err(|_| Error::Ld2("offsetInfo out of range".into()))?;
        dt = u32_at(data, off)?;
    }
    let limit_sub = u32_at(data, off + 4)? as u64;
    let cdh_sub = u32_at(data, off + 8)? as u64;
    let idx_len = u32_at(data, off + 12)? as u64;
    let phr_len = u32_at(data, off + 16)? as u64;
    let xml_len = u32_at(data, off + 20)? as u64;
    let off_u64 = off as u64;
    Ok((
        Ld2Header {
            dict_type: dt,
            limit: limit_sub + off_u64 + 8,
            offset_cdh: cdh_sub + off_u64 + 0x1C,
            idx_len,
            phr_len,
            xml_len,
        },
        off_u64,
    ))
}

/// 收集 refs=0 entry 的纯文本作为编码探测样本。
///
/// Phrases 段除 word 文本外还含 refs>0 条目的 u32 引用数组（二进制），
/// 直接对整段探测会被引用数据污染。这里只取 refs=0 的纯文本部分。
fn collect_text_samples(
    inflated: &[u8],
    dh: &Ld2Header,
    max_bytes: usize,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let off_phr = usize::try_from(dh.idx_len).map_err(|_| Error::Ld2("bad idx_len".into()))?;
    let off_xml = off_phr
        .checked_add(usize::try_from(dh.phr_len).map_err(|_| Error::Ld2("bad phr_len".into()))?)
        .ok_or_else(|| Error::Ld2("offsets overflow".into()))?;
    let entry_count = slot_count(dh.idx_len);
    let mut word_buf: Vec<u8> = Vec::new();
    let mut def_buf: Vec<u8> = Vec::new();
    for i in 0..entry_count {
        if word_buf.len() >= max_bytes && def_buf.len() >= max_bytes {
            break;
        }
        let cur = read_slot(inflated, i)?;
        let lw = cur.word_pos;
        let lx = cur.xml_pos;
        if cur.refs != 0 {
            continue;
        }
        let nxt = read_slot(inflated, i + 1)?;
        let nw = nxt.word_pos;
        let nx = nxt.xml_pos;
        if word_buf.len() < max_bytes {
            let a = off_phr + lw as usize;
            let b = off_phr + nw as usize;
            if b <= inflated.len() && a <= b {
                word_buf.extend_from_slice(&inflated[a..b]);
            }
        }
        if def_buf.len() < max_bytes {
            let a = off_xml + lx as usize;
            let b = off_xml + nx as usize;
            if b <= inflated.len() && a <= b {
                def_buf.extend_from_slice(&inflated[a..b]);
            }
        }
    }
    word_buf.truncate(max_bytes);
    def_buf.truncate(max_bytes);
    Ok((word_buf, def_buf))
}

fn slot_count(idx_len: u64) -> usize {
    (idx_len as usize / ENTRY_SIZE).saturating_sub(1)
}

#[inline]
fn read_slot(data: &[u8], i: usize) -> Result<Slot> {
    let off = i
        .checked_mul(ENTRY_SIZE)
        .ok_or_else(|| Error::Ld2("slot index overflow".into()))?;
    let s = data
        .get(off..off + ENTRY_SIZE)
        .ok_or_else(|| Error::Ld2(format!("slot {i} out of bounds")))?;
    Ok(Slot {
        word_pos: u32::from_le_bytes([s[0], s[1], s[2], s[3]]),
        xml_pos: u32::from_le_bytes([s[4], s[5], s[6], s[7]]),
        flags: s[8],
        refs: s[9],
    })
}

impl Dictionary {
    /// 打开并解析 ld2 文件。不支持 ?LDF 变体。
    pub fn open(path: impl AsRef<Path>) -> Result<Dictionary> {
        let p = path.as_ref().to_path_buf();
        let file = std::fs::File::open(&p)?;
        // SAFETY: 只读映射，文件在映射期间由 Dictionary 持有；调用方不得修改该文件。
        let mm = unsafe { Mmap::map(&file)? };
        let data: &[u8] = &mm;
        let magic: [u8; 4] = data
            .get(0..4)
            .map(|s| [s[0], s[1], s[2], s[3]])
            .ok_or_else(|| Error::Ld2("file shorter than 4 bytes".into()))?;
        if magic != MAGIC_LD2 {
            if magic == MAGIC_LDF {
                return Err(Error::UnsupportedVariant {
                    magic,
                    path: p,
                    hint: format!("magic={magic:?}"),
                });
            }
            return Err(Error::UnsupportedVariant {
                magic,
                path: p,
                hint: format!("unknown magic={magic:?}"),
            });
        }
        let header = parse_header(data)?;
        let (dh, _offset_data) = parse_dict_header(data, header.offset_data)?;
        let (streams, compressed_start) = collect_streams(data, dh.offset_cdh, dh.limit)?;
        let inflated = inflate_all(data, &streams, compressed_start)?;
        let expected = (dh.idx_len + dh.phr_len + dh.xml_len) as usize;
        if inflated.len() != expected {
            return Err(Error::Ld2(format!(
                "inflated length mismatch: {} != {} at {}",
                inflated.len(),
                expected,
                p.display()
            )));
        }
        let (word_sample, def_sample) = collect_text_samples(&inflated, &dh, ENCODING_SAMPLE)?;
        let word_encoding = detect_encoding(&word_sample);
        let def_encoding = detect_encoding(&def_sample);
        let entry_count = slot_count(dh.idx_len);
        let off_phr = dh.idx_len as usize;
        let off_xml = off_phr + dh.phr_len as usize;
        if off_xml > inflated.len() {
            return Err(Error::Ld2("inflated buffer shorter than offsets".into()));
        }
        Ok(Dictionary {
            source_path: p,
            data: mm,
            inflated,
            dh,
            header,
            word_encoding,
            def_encoding,
            entry_count,
            off_phr,
            off_xml,
        })
    }

    pub fn version(&self) -> (u16, u16) {
        (self.header.version_major, self.header.version_minor)
    }

    pub fn raw_data(&self) -> &[u8] {
        &self.data
    }

    /// 字典压缩区结束的绝对偏移（资源段起点）。
    pub fn limit(&self) -> u64 {
        self.dh.limit
    }

    pub fn filesize(&self) -> u64 {
        file_len(&self.data)
    }

    pub fn header(&self) -> Ld2Header {
        self.dh
    }

    pub fn extract_resources(&self) -> std::collections::BTreeMap<String, Vec<u8>> {
        extract_resources(&self.data, self.dh.limit, self.filesize())
    }

    fn slot(&self, i: usize) -> Result<Slot> {
        read_slot(&self.inflated, i)
    }

    /// 返回 entry i 的 (word 文本字节, 引用索引列表)。
    fn word_span(&self, i: usize) -> Result<(Vec<u8>, Vec<u32>)> {
        let (lw, _lx, _fl, refs) = {
            let s = self.slot(i)?;
            (s.word_pos, s.xml_pos, s.flags, s.refs)
        };
        let nw = self.slot(i + 1)?.word_pos;
        let base = self.off_phr + lw as usize;
        let end = self.off_phr + nw as usize;
        if end > self.inflated.len() || base > end {
            return Err(Error::Ld2(format!("word span out of bounds at entry {i}")));
        }
        let mut refs_list = Vec::new();
        let mut word_start = base;
        if refs > 0 {
            let need = refs as usize * 4;
            if base + need <= self.inflated.len() && base + need <= end {
                for chunk in self.inflated[base..base + need].chunks_exact(4) {
                    refs_list.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            word_start = base + need;
        }
        let word_start = word_start.min(end);
        if end > self.inflated.len() {
            return Err(Error::Ld2(format!("word span out of bounds at entry {i}")));
        }
        Ok((self.inflated[word_start..end].to_vec(), refs_list))
    }

    fn xml_span(&self, i: usize) -> Result<&[u8]> {
        let lx = self.slot(i)?.xml_pos;
        let nx = self.slot(i + 1)?.xml_pos;
        let a = self.off_xml + lx as usize;
        let b = self.off_xml + nx as usize;
        if b > self.inflated.len() || a > b {
            return Err(Error::Ld2(format!("xml span out of bounds at entry {i}")));
        }
        Ok(&self.inflated[a..b])
    }

    pub fn entry(&self, i: usize) -> Result<Entry> {
        if i >= self.entry_count {
            return Err(Error::Ld2(format!("entry index {i} out of range")));
        }
        let flags = self.slot(i)?.flags;
        let (word_b, refs_list) = self.word_span(i)?;
        let mut word = if word_b.is_empty() {
            String::new()
        } else {
            decode_with(&word_b, self.word_encoding)
        };
        let def_b: Vec<u8> = if !refs_list.is_empty() {
            let mut parts: Vec<u8> = Vec::new();
            for &r in &refs_list {
                if (r as usize) < self.entry_count {
                    parts.extend_from_slice(self.xml_span(r as usize)?);
                }
            }
            if word.is_empty() {
                if let Some(&r0) = refs_list.first() {
                    let (rb, _) = self.word_span(r0 as usize)?;
                    if !rb.is_empty() {
                        word = decode_with(&rb, self.word_encoding);
                    }
                }
            }
            parts
        } else {
            self.xml_span(i)?.to_vec()
        };
        let definition = if def_b.is_empty() {
            String::new()
        } else {
            decode_with(&def_b, self.def_encoding)
        };
        Ok(Entry {
            word,
            definition,
            cross_refs: refs_list,
            flags,
            index: i,
        })
    }

    /// 只取 word，不构造完整 Entry（编码探测/建索引用）。
    pub fn word(&self, i: usize) -> Result<String> {
        let (b, _) = self.word_span(i)?;
        Ok(if b.is_empty() {
            String::new()
        } else {
            decode_with(&b, self.word_encoding)
        })
    }

    pub fn iter(&self) -> EntryIter<'_> {
        EntryIter {
            dict: self,
            next: 0,
        }
    }
}

pub struct EntryIter<'a> {
    dict: &'a Dictionary,
    next: usize,
}

impl Iterator for EntryIter<'_> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.dict.entry_count {
            return None;
        }
        let i = self.next;
        self.next += 1;
        Some(self.dict.entry(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_ldf() {
        let dir = std::env::temp_dir().join(format!("lexicon-ld2-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("fake.ld2");
        let mut bytes = b"?LDF".to_vec();
        bytes.extend(std::iter::repeat(0u8).take(200));
        std::fs::write(&p, bytes).unwrap();
        match Dictionary::open(&p) {
            Err(Error::UnsupportedVariant { .. }) => {}
            Ok(_) => panic!("expected UnsupportedVariant, got Ok"),
            Err(e) => panic!("expected UnsupportedVariant, got {e}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
