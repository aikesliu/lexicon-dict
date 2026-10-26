//! StarDict 读取器：mmap `.idx` + 字节间隔采样跳板 + 二分 + mmap `.dict` 按需读取。
//!
//! 查询热路径仅做内存二分与一小段切片，内存占用与 `.dict` 总大小解耦（按需 page-in）。
//!
//! 注意：`.idx` 的 8/16 字节头里含二进制 offset/size，NUL 字节并不只出现在 word 末尾，
//! 因此不能从任意偏移可靠地反推 record 起点；这里沿用原实现的采样跳板（每 4096 字节
//! 采样一个已知 record 起点），既正确又能在加载时顺序构建。

use std::fs::File;
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::error::{Error, Result};

/// 每 4096 字节采样一个 record 起点构建跳板。
pub const SAMPLE_INTERVAL: usize = 4096;

pub struct StarDict {
    pub ifo_path: PathBuf,
    pub bookname: String,
    pub wordcount: usize,
    pub idxoffsetbits: u32,
    idx: Mmap,
    dict: Mmap,
    rec_hdr_size: usize,
    sample_words: Vec<Box<[u8]>>,
    sample_pos: Vec<usize>,
}

struct Record<'a> {
    word: &'a [u8],
    offset: u64,
    size: u64,
    next: usize,
}

impl StarDict {
    pub fn open(ifo_path: impl AsRef<Path>) -> Result<StarDict> {
        let ifo_path = ifo_path.as_ref().to_path_buf();
        if !ifo_path.exists() {
            return Err(Error::StarDict(format!(
                "ifo not found: {}",
                ifo_path.display()
            )));
        }
        let text = std::fs::read_to_string(&ifo_path)
            .map_err(|e| Error::StarDict(format!("read ifo {}: {e}", ifo_path.display())))?;
        let mut bookname = String::new();
        let mut wordcount = 0usize;
        let mut idxoffsetbits = 32u32;
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                match k.trim() {
                    "bookname" => bookname = v.trim().to_string(),
                    "wordcount" => {
                        wordcount = v.trim().parse().map_err(|_| {
                            Error::StarDict(format!("bad wordcount in {}", ifo_path.display()))
                        })?
                    }
                    "idxoffsetbits" => {
                        idxoffsetbits = if v.trim() == "64" { 64 } else { 32 };
                    }
                    _ => {}
                }
            }
        }
        let base = ifo_path.with_extension("");
        let idx = map_read(&base.with_extension("idx"))?;
        let dict = map_read(&base.with_extension("dict"))?;
        let rec_hdr_size = if idxoffsetbits == 64 { 16 } else { 8 };
        let mut sd = StarDict {
            ifo_path,
            bookname,
            wordcount,
            idxoffsetbits,
            idx,
            dict,
            rec_hdr_size,
            sample_words: Vec::new(),
            sample_pos: Vec::new(),
        };
        sd.build_samples()?;
        Ok(sd)
    }

    fn read_record(&self, pos: usize) -> Option<Record<'_>> {
        if pos >= self.idx.len() {
            return None;
        }
        let rest = &self.idx[pos..];
        let nul = memchr::memchr(0, rest)?;
        let word = &rest[..nul];
        let hdr = &rest[nul + 1..];
        if hdr.len() < self.rec_hdr_size {
            return None;
        }
        let (offset, size) = if self.idxoffsetbits == 64 {
            (
                u64::from_be_bytes(hdr[0..8].try_into().ok()?),
                u64::from_be_bytes(hdr[8..16].try_into().ok()?),
            )
        } else {
            (
                u32::from_be_bytes(hdr[0..4].try_into().ok()?) as u64,
                u32::from_be_bytes(hdr[4..8].try_into().ok()?) as u64,
            )
        };
        Some(Record {
            word,
            offset,
            size,
            next: pos + nul + 1 + self.rec_hdr_size,
        })
    }

    fn build_samples(&mut self) -> Result<()> {
        let mut pos = 0usize;
        let mut next_sample = 0usize;
        while pos < self.idx.len() {
            let rec = match self.read_record(pos) {
                Some(r) => r,
                None => break,
            };
            let next = rec.next;
            if next <= pos {
                break;
            }
            if pos >= next_sample {
                let word: Box<[u8]> = rec.word.into();
                self.sample_words.push(word);
                self.sample_pos.push(pos);
                next_sample = pos + SAMPLE_INTERVAL;
            }
            pos = next;
        }
        Ok(())
    }

    fn dict_slice(&self, offset: u64, size: u64) -> Option<&[u8]> {
        let a = offset as usize;
        let b = a.checked_add(size as usize)?;
        self.dict.get(a..b)
    }

    /// 精确匹配，返回释义 HTML/XML 或 None。
    pub fn lookup(&self, word: &str) -> Option<String> {
        let wb = word.as_bytes();
        let i = self.sample_words.partition_point(|w| w.as_ref() <= wb);
        if i == 0 {
            return None;
        }
        let i = i - 1;
        let mut pos = *self.sample_pos.get(i)?;
        let end_pos = self
            .sample_pos
            .get(i + 1)
            .copied()
            .unwrap_or(self.idx.len());
        while pos < end_pos {
            let rec = self.read_record(pos)?;
            if rec.word == wb {
                return self
                    .dict_slice(rec.offset, rec.size)
                    .map(|b| String::from_utf8_lossy(b).into_owned());
            }
            if rec.word > wb {
                break;
            }
            pos = rec.next;
        }
        None
    }

    /// 返回以 prefix 开头的词（前缀匹配，用于输入提示）。
    pub fn suggest(&self, prefix: &str, limit: usize) -> Vec<String> {
        let pb = prefix.as_bytes();
        if pb.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut i = self.sample_words.partition_point(|w| w.as_ref() < pb);
        if i > 0 {
            i -= 1;
        }
        let mut pos = self.sample_pos.get(i).copied().unwrap_or(0);
        let mut results = Vec::new();
        while pos < self.idx.len() && results.len() < limit {
            let rec = match self.read_record(pos) {
                Some(r) => r,
                None => break,
            };
            if rec.word.starts_with(pb) {
                results.push(String::from_utf8_lossy(rec.word).into_owned());
            } else if rec.word > pb {
                break;
            }
            pos = rec.next;
        }
        results
    }

    pub fn len(&self) -> usize {
        self.wordcount
    }

    pub fn is_empty(&self) -> bool {
        self.wordcount == 0
    }
}

fn map_read(path: &Path) -> Result<Mmap> {
    let f =
        File::open(path).map_err(|e| Error::StarDict(format!("open {}: {e}", path.display())))?;
    // SAFETY: 只读映射，文件句柄在映射建立后即可关闭；内容由 Mmap 持有。
    let m = unsafe { Mmap::map(&f) }
        .map_err(|e| Error::StarDict(format!("mmap {}: {e}", path.display())))?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_synthetic() {
        let dir = std::env::temp_dir().join(format!("lexicon-sd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let items: Vec<(String, String)> = vec![
            ("apple".into(), "a".into()),
            ("banana".into(), "b".into()),
            ("cherry".into(), "c".into()),
            ("zebra".into(), "z".into()),
        ];
        crate::stardict::write_stardict(&dir, "d", "D", items, None).unwrap();
        let sd = StarDict::open(dir.join("d/d.ifo")).unwrap();
        for (w, expected) in [
            ("apple", "a"),
            ("banana", "b"),
            ("cherry", "c"),
            ("zebra", "z"),
        ] {
            assert_eq!(sd.lookup(w).as_deref(), Some(expected), "{w}");
        }
        assert_eq!(sd.lookup("aardvark"), None);
        assert_eq!(sd.lookup("zzz"), None);
        assert_eq!(sd.suggest("ba", 10), vec!["banana".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
