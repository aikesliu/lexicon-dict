//! 编码探测：UTF-8 / UTF-16-LE / GB18030 三选。
//!
//! 同一 ld2 词典内 word 与 def 的编码相互独立（实测：E-C Computer word=UTF-8/def=UTF-16-LE，
//! Modern Chinese 两者皆 UTF-16-LE，NCCE word=UTF-16-LE/def=UTF-8），需分别探测。
//!
//! 关键陷阱：UTF-16-LE 的中文（如 '一'=0x00 0x4E）作为 UTF-8 严格解码往往"成功"但解出含大量
//! \x00 的乱码。因此 UTF-8 解码成功后，必须检查结果的控制符比例，高则判定为 UTF-16-LE 误判。

use encoding_rs::{GB18030, UTF_8, UTF_16LE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Gb18030,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf-8",
            Encoding::Utf16Le => "utf-16-le",
            Encoding::Gb18030 => "gb18030",
        }
    }

    pub fn from_name(name: &str) -> Encoding {
        match name {
            "utf-16-le" | "utf-16le" | "utf16-le" => Encoding::Utf16Le,
            "gb18030" | "gbk" | "gb2312" => Encoding::Gb18030,
            _ => Encoding::Utf8,
        }
    }
}

/// 对样本字节返回最可能的编码。
pub fn detect_encoding(sample: &[u8]) -> Encoding {
    if sample.is_empty() {
        return Encoding::Utf8;
    }
    // 1. UTF-8 严格解码 + 控制符比例检查（识别 UTF-16-LE 误判）。
    if let Ok(decoded) = std::str::from_utf8(sample) {
        let mut ctrl = 0usize;
        let mut len = 0usize;
        for c in decoded.chars() {
            len += 1;
            if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' {
                ctrl += 1;
            }
        }
        if ctrl as f64 / (len.max(1) as f64) <= 0.05 {
            return Encoding::Utf8;
        }
        // 控制符多 → 疑似 UTF-16-LE，继续启发式检测。
    }
    // 2. UTF-16-LE
    if try_utf16le(sample) {
        return Encoding::Utf16Le;
    }
    // 3. GB18030（GBK 超集，兜底）
    if GB18030
        .decode_without_bom_handling_and_without_replacement(sample)
        .is_some()
    {
        return Encoding::Gb18030;
    }
    Encoding::Utf8
}

fn try_utf16le(sample: &[u8]) -> bool {
    let n = sample.len() - (sample.len() % 2);
    if n < 2 {
        return false;
    }
    // Python 的 utf-16-le 严格解码拒绝孤立代理项。
    let mut units = Vec::with_capacity(n / 2);
    for pair in sample[..n].chunks_exact(2) {
        units.push(u16::from_le_bytes([pair[0], pair[1]]));
    }
    let decoded: String = match char::decode_utf16(units).collect::<Result<String, _>>() {
        Ok(s) => s,
        Err(_) => return false,
    };
    if decoded.is_empty() {
        return false;
    }
    let mut suspicious = 0usize;
    let mut len = 0usize;
    for c in decoded.chars() {
        len += 1;
        let cp = c as u32;
        if (0xE000..=0xF8FF).contains(&cp) || (0xDC80..=0xDCFF).contains(&cp) || cp == 0xFFFD {
            suspicious += 1;
        }
    }
    suspicious as f64 / (len.max(1) as f64) < 0.05
}

/// 按指定编码解码；非法序列降级为 U+FFFD（避免单条坏字符中断整体）。
pub fn decode_with(raw: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 => UTF_8.decode_without_bom_handling(raw).0.into_owned(),
        Encoding::Utf16Le => UTF_16LE.decode_without_bom_handling(raw).0.into_owned(),
        Encoding::Gb18030 => GB18030.decode_without_bom_handling(raw).0.into_owned(),
    }
}
