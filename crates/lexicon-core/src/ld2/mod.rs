//! `.ld2` 词典解析。
//!
//! 文件格式（小端，已用本地文件实测验证）：
//!   Header 0x00-0x60 明文 -> offsetData = offsetDataSub@0x5C + 0x60
//!   Dictionary Header@offsetData -> limit/offsetCDH/三段长度
//!   Compressed Data Header@offsetCDH -> streams table + 连续 zlib 块
//!   解压后：Word Index(每条 10 字节) + Phrases(词条) + XML Defs(释义)

pub mod dictionary;
pub mod encoding;
pub mod header;
pub mod resources;
pub mod streams;

pub use dictionary::{Dictionary, Entry, Ld2Header, MAGIC_LD2, MAGIC_LDF, Slot};
pub use encoding::{Encoding, decode_with, detect_encoding};

/// 打开并解析 ld2 文件。
pub fn open(path: impl AsRef<std::path::Path>) -> crate::error::Result<Dictionary> {
    Dictionary::open(path)
}
