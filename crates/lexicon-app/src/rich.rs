//! Pango markup → egui LayoutJob（支持 b/i/u/span foreground 与实体）。

use egui::text::LayoutJob;
use egui::{Color32, FontFamily, FontId, Stroke, TextFormat};

#[derive(Clone, Debug)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<Color32>,
}

fn hex_color(s: &str) -> Option<Color32> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    ))
}

fn entity(ent: &str) -> Option<char> {
    match ent {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = ent.strip_prefix('#') {
                dec.parse::<u32>().ok().and_then(char::from_u32)
            } else {
                None
            }
        }
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        // `find` on the whole tail is safe; only accept entities within 32 bytes.
        if let Some(end) = tail.find(';') {
            if end <= 32 {
                if let Some(c) = entity(&tail[1..end]) {
                    out.push(c);
                    rest = &tail[end + 1..];
                    continue;
                }
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

/// 字体无法覆盖的私有区/控制字符会变成方框，直接丢弃；
/// 单独的组合音标（前面没有基字）补一个虚线圆，避免显示成 tofu。
fn sanitize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut has_base = false;
    for c in s.chars() {
        let cp = c as u32;
        if (0xE000..=0xF8FF).contains(&cp)
            || (0xF0000..=0xFFFFD).contains(&cp)
            || (0x100000..=0x10FFFD).contains(&cp)
        {
            continue;
        }
        if (0x0300..=0x036F).contains(&cp) {
            if !has_base {
                out.push('\u{25CC}');
            }
            out.push(c);
            // 组合符号不构成新的基字，后续组合符号仍依附同一个基字。
            continue;
        }
        out.push(c);
        has_base = !c.is_whitespace()
            && !matches!(
                c,
                '|' | '/'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | ','
                    | '.'
                    | ';'
                    | ':'
                    | '，'
                    | '。'
                    | '；'
                    | '：'
            );
    }
    out
}

pub fn parse(pango: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut color: Option<Color32> = None;
    let mut stack: Vec<(bool, bool, bool, Option<Color32>)> = Vec::new();
    let mut buf = String::new();

    fn flush(
        buf: &mut String,
        spans: &mut Vec<Span>,
        bold: bool,
        italic: bool,
        underline: bool,
        color: Option<Color32>,
    ) {
        if !buf.is_empty() {
            spans.push(Span {
                text: sanitize_text(&std::mem::take(buf)),
                bold,
                italic,
                underline,
                color,
            });
        }
    }

    let bytes = pango.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(end) = pango[i..].find('>') {
                let tag = pango[i + 1..i + end].trim();
                let closing = tag.starts_with('/');
                let body = tag.trim_start_matches('/').trim();
                let name = body.split_whitespace().next().unwrap_or("").to_lowercase();
                flush(&mut buf, &mut spans, bold, italic, underline, color);
                match (closing, name.as_str()) {
                    (false, "b") | (false, "strong") => {
                        stack.push((bold, italic, underline, color));
                        bold = true;
                    }
                    (true, "b") | (true, "strong") => {
                        if let Some(v) = stack.pop() {
                            (bold, italic, underline, color) = v;
                        }
                    }
                    (false, "i") | (false, "em") => {
                        stack.push((bold, italic, underline, color));
                        italic = true;
                    }
                    (true, "i") | (true, "em") => {
                        if let Some(v) = stack.pop() {
                            (bold, italic, underline, color) = v;
                        }
                    }
                    (false, "u") => {
                        stack.push((bold, italic, underline, color));
                        underline = true;
                    }
                    (true, "u") => {
                        if let Some(v) = stack.pop() {
                            (bold, italic, underline, color) = v;
                        }
                    }
                    (false, "span") => {
                        stack.push((bold, italic, underline, color));
                        if let Some(p) = body.find("foreground") {
                            let rest = &body[p..];
                            if let Some(q1) = rest.find('"') {
                                if let Some(q2) = rest[q1 + 1..].find('"') {
                                    color = hex_color(&rest[q1 + 1..q1 + 1 + q2]);
                                }
                            }
                        }
                    }
                    (true, "span") => {
                        if let Some(v) = stack.pop() {
                            (bold, italic, underline, color) = v;
                        }
                    }
                    _ => {}
                }
                i += end + 1;
                continue;
            }
        }
        let next = pango[i..].find('<').map(|p| i + p).unwrap_or(bytes.len());
        buf.push_str(&unescape(&pango[i..next]));
        i = next;
    }
    flush(&mut buf, &mut spans, bold, italic, underline, color);
    spans
}

pub fn plain(pango: &str) -> String {
    parse(pango).iter().map(|s| s.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_does_not_split_utf8_at_entity_limit() {
        // A '&' followed by 30 ASCII bytes and a 3-byte CJK char: the old
        // 33-byte slice cut the CJK char in half and panicked.
        let s = format!("&{}中", "a".repeat(30));
        assert_eq!(unescape(&s), s);
        assert_eq!(unescape("&amp;中"), "&中");
    }

    #[test]
    fn long_entity_like_text_is_literal() {
        let s = format!("&{}", "x".repeat(64));
        assert_eq!(unescape(&s), s);
    }

    #[test]
    fn parse_never_panics_on_utf8_near_entity_limit() {
        for c in ["中", "😀", "é"] {
            for n in 0..40 {
                let s = format!("&{}{}&amp; tail", "a".repeat(n), c);
                let _ = parse(&s);
            }
        }
    }

    #[test]
    fn private_use_and_lone_combining_are_cleaned() {
        let spans = parse("|\u{F00F}\u{0300}");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "|\u{25CC}\u{0300}");
    }

    #[test]
    fn normal_combining_mark_stays_attached() {
        let spans = parse("e\u{0301}");
        assert_eq!(spans[0].text, "e\u{0301}");
    }
}

pub fn job(spans: &[Span], base_size: f32, default_color: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for s in spans {
        let family = if s.bold {
            crate::fonts::bold()
        } else {
            FontFamily::Proportional
        };
        let color = s.color.unwrap_or(default_color);
        job.append(
            &s.text,
            0.0,
            TextFormat {
                font_id: FontId::new(base_size, family),
                color,
                italics: s.italic,
                underline: if s.underline {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                },
                line_height: Some((base_size * 1.5).round()),
                ..Default::default()
            },
        );
    }
    job
}
