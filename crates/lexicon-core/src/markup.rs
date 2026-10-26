//! 把 LD2 释义 XML/HTML 清洗成 Pango markup / 有序块。
//!
//! 与 Python 版 `markup.py` 对齐：
//! - 结构容器 `<C><F><H><I><N>` 剥离，保留子内容；
//! - `<n/>`/`<br/>` 换行；`<x K="blue">` 着色；`<Ô P="...">` 样式容器剥离；
//! - 块级 `P/Q/T/W/X` 前置换行；`U/h` 加粗；`M/v/V` 斜体；`Ã` 灰色斜体；
//! - `<Ë M="dict://res/x.png">` 图片引用（to_blocks 生成图片块）；
//! - 文本节点里二次嵌入的 HTML 再清洗；
//! - 词性缩写加粗、斜杠音标斜体；
//! - 解析失败回退为去标签纯文本。

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text { pango: String },
    Image { name: String },
}

fn color_for(k: Option<&str>) -> Option<String> {
    let s = k?.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with('#') {
        return Some(s.to_string());
    }
    let map = color_map();
    let key = s.to_lowercase();
    map.get(key.as_str()).cloned()
}

fn color_map() -> &'static HashMap<&'static str, String> {
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let pairs: &[(&str, &str)] = &[
            ("black", "#cccccc"),
            ("blue", "#6ba0ff"),
            ("navy", "#6ba0ff"),
            ("teal", "#4ec9b0"),
            ("aqua", "#4ec9b0"),
            ("cyan", "#4ec9b0"),
            ("red", "#ff6b6b"),
            ("maroon", "#ff6b6b"),
            ("crimson", "#ff6b6b"),
            ("green", "#7ee787"),
            ("lime", "#7ee787"),
            ("purple", "#c77dff"),
            ("magenta", "#c77dff"),
            ("fuchsia", "#c77dff"),
            ("orange", "#ffa657"),
            ("brown", "#d2a679"),
            ("gray", "#9a9a9a"),
            ("grey", "#9a9a9a"),
            ("silver", "#b0b0b0"),
            ("white", "#ffffff"),
            ("yellow", "#f0c674"),
            ("gold", "#f0c674"),
            ("pink", "#ff9ecf"),
        ];
        pairs.iter().map(|(k, v)| (*k, (*v).to_string())).collect()
    })
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Text(String),
    Open {
        name: String,
        attrs: Vec<(String, String)>,
        self_closing: bool,
    },
    Close {
        name: String,
    },
}

/// 宽容的标签分词器：不要求 XML 良构，未知标签原样保留名字。
fn tokenize(src: &str) -> Vec<Tok> {
    tokenize_with(src, true)
}

/// HTML 模式：标签名必须以 ASCII 字母开头（`<物>` 这类非 ASCII 标签按文本处理，
/// 与 libxml2 的 HTML parser 行为一致）。
fn tokenize_html(src: &str) -> Vec<Tok> {
    tokenize_with(src, false)
}

fn tokenize_with(src: &str, xml_names: bool) -> Vec<Tok> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut text_start = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with("<!--") {
            if let Some(end) = src[i + 4..].find("-->") {
                push_text(&mut out, &src[text_start..i]);
                i += 4 + end + 3;
                text_start = i;
                continue;
            }
        }
        if src[i..].starts_with("<![CDATA[") {
            if let Some(end) = src[i + 9..].find("]]>") {
                push_text(&mut out, &src[text_start..i]);
                out.push(Tok::Text(src[i + 9..i + 9 + end].to_string()));
                i += 9 + end + 3;
                text_start = i;
                continue;
            }
        }
        // 找到标签结束的 '>'（引号内的 '>' 不算）
        let mut j = i + 1;
        let mut quote: Option<u8> = None;
        while j < bytes.len() {
            let b = bytes[j];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                quote = Some(b);
            } else if b == b'>' {
                break;
            }
            j += 1;
        }
        if j >= bytes.len() {
            i += 1;
            continue;
        }
        if !tag_name_ok(&src[i + 1..j], xml_names) {
            i += 1;
            continue;
        }
        let raw = &src[i + 1..j];
        push_text(&mut out, &src[text_start..i]);
        i = j + 1;
        text_start = i;
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        if let Some(rest) = raw.strip_prefix('/') {
            out.push(Tok::Close {
                name: rest.trim().to_string(),
            });
        } else if raw.ends_with('/') {
            let body = raw[..raw.len() - 1].trim();
            let (name, attrs) = parse_tag(body);
            out.push(Tok::Open {
                name,
                attrs,
                self_closing: true,
            });
        } else {
            let (name, attrs) = parse_tag(raw);
            out.push(Tok::Open {
                name,
                attrs,
                self_closing: false,
            });
        }
    }
    push_text(&mut out, &src[text_start..]);
    out
}

fn tag_name_ok(raw: &str, xml_names: bool) -> bool {
    // 注意：`< b>` 不是标签（XML/HTML 都要求标签名紧跟 `<`）。
    let mut chars = raw.chars().peekable();
    if chars.peek() == Some(&'/') {
        chars.next();
    }
    match chars.next() {
        None => false,
        Some(c) => {
            if xml_names {
                c.is_alphabetic() || c == '_' || c == ':'
            } else {
                c.is_ascii_alphabetic()
            }
        }
    }
}

fn push_text(out: &mut Vec<Tok>, s: &str) {
    if !s.is_empty() {
        out.push(Tok::Text(s.to_string()));
    }
}

fn parse_tag(raw: &str) -> (String, Vec<(String, String)>) {
    let raw = raw.trim();
    let mut name = String::new();
    let mut i = 0usize;
    let bytes = raw.as_bytes();
    while i < bytes.len() {
        let c = raw[i..].chars().next().unwrap();
        if c.is_whitespace() || c == '/' {
            break;
        }
        name.push(c);
        i += c.len_utf8();
    }
    let mut attrs = Vec::new();
    let rest = &raw[i..];
    let mut k = 0usize;
    let rb = rest.as_bytes();
    while k < rb.len() {
        while k < rb.len() && (rb[k] as char).is_whitespace() {
            k += 1;
        }
        if k >= rb.len() {
            break;
        }
        let key_start = k;
        while k < rb.len() && rb[k] != b'=' && !(rb[k] as char).is_whitespace() {
            k += 1;
        }
        let key = rest[key_start..k].trim().to_string();
        while k < rb.len() && (rb[k] as char).is_whitespace() {
            k += 1;
        }
        if k < rb.len() && rb[k] == b'=' {
            k += 1;
            while k < rb.len() && (rb[k] as char).is_whitespace() {
                k += 1;
            }
            if k < rb.len() && (rb[k] == b'"' || rb[k] == b'\'') {
                let q = rb[k];
                k += 1;
                let val_start = k;
                while k < rb.len() && rb[k] != q {
                    k += 1;
                }
                let val = rest[val_start..k].to_string();
                if k < rb.len() {
                    k += 1;
                }
                if !key.is_empty() {
                    attrs.push((key, val));
                }
            } else {
                let val_start = k;
                while k < rb.len() && !(rb[k] as char).is_whitespace() {
                    k += 1;
                }
                if !key.is_empty() {
                    attrs.push((key, rest[val_start..k].to_string()));
                }
            }
        } else if !key.is_empty() {
            attrs.push((key, String::new()));
        }
    }
    (name, attrs)
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn entity_map() -> &'static HashMap<&'static str, &'static str> {
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        entities::ENTITIES
            .iter()
            .map(|e| (e.entity, e.characters))
            .collect()
    })
}

fn decode_entity(ent: &str) -> Option<String> {
    match ent {
        "amp" => return Some("&".to_string()),
        "lt" => return Some("<".to_string()),
        "gt" => return Some(">".to_string()),
        "quot" => return Some("\"".to_string()),
        "apos" => return Some("'".to_string()),
        _ => {}
    }
    if let Some(hex) = ent.strip_prefix("#x").or_else(|| ent.strip_prefix("#X")) {
        let cp = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(cp).map(|c| c.to_string());
    }
    if let Some(dec) = ent.strip_prefix('#') {
        let cp: u32 = dec.parse().ok()?;
        return char::from_u32(cp).map(|c| c.to_string());
    }
    // HTML5 命名实体表（含 &agrave; &aelig; &pound; 等）。
    let key = format!("&{ent};");
    entity_map().get(key.as_str()).map(|s| (*s).to_string())
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        // 找分号；实体名最长 32 字符（HTML5 最长实体约 32）。
        let semi = tail[1..].find(';').filter(|&e| e <= 32).map(|e| e + 1);
        if let Some(end) = semi {
            let ent = &tail[1..end];
            if let Some(decoded) = decode_entity(ent) {
                out.push_str(&decoded);
                rest = &tail[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

fn res_name(url: &str) -> String {
    for p in ["dict://res/", "dict://", "res://", "res/"] {
        if let Some(rest) = url.strip_prefix(p) {
            return rest.to_string();
        }
    }
    url.to_string()
}

fn is_block_tag(t: &str) -> bool {
    matches!(t, "P" | "Q" | "T" | "W" | "X")
}
fn is_bold_tag(t: &str) -> bool {
    matches!(t, "U" | "h")
}
fn is_italic_tag(t: &str) -> bool {
    matches!(t, "M" | "v" | "V")
}

struct Walker {
    out: String,
    /// true 时生成图片占位锚点（to_blocks 路径）。
    image_blocks: bool,
}

impl Walker {
    fn walk(&mut self, toks: &[Tok], i: &mut usize, close_at: Option<&str>, lower: bool) {
        while *i < toks.len() {
            match &toks[*i] {
                Tok::Text(t) => {
                    self.out.push_str(&escape(&unescape(t)));
                    *i += 1;
                }
                Tok::Open {
                    name,
                    attrs,
                    self_closing,
                } => {
                    let n = if lower {
                        name.to_lowercase()
                    } else {
                        name.clone()
                    };
                    *i += 1;
                    if *self_closing {
                        if n == "br" || n == "n" || n == "hr" {
                            self.out.push('\n');
                        } else if n == "img" || n == "Ë" || n == "e" {
                            if self.image_blocks {
                                if let Some(m) = attr(attrs, "M").or_else(|| attr(attrs, "m")) {
                                    let name = res_name(m);
                                    if !name.is_empty() {
                                        self.out.push('\0');
                                        self.out.push_str("IMG:");
                                        self.out.push_str(&name);
                                        self.out.push('\0');
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    if n == "br" || n == "n" || n == "hr" {
                        self.out.push('\n');
                        continue;
                    }
                    if n == "x" {
                        let color = color_for(attr(attrs, "K").or_else(|| attr(attrs, "k")));
                        if let Some(c) = color {
                            self.out.push_str(&format!("<span foreground=\"{c}\">"));
                            self.walk(toks, i, Some("x"), lower);
                            self.out.push_str("</span>");
                        } else {
                            self.walk(toks, i, Some("x"), lower);
                        }
                        continue;
                    }
                    if n == "img" || n == "Ë" || n == "e" {
                        if self.image_blocks {
                            if let Some(m) = attr(attrs, "M").or_else(|| attr(attrs, "m")) {
                                let name = res_name(m);
                                if !name.is_empty() {
                                    self.out.push('\0');
                                    self.out.push_str("IMG:");
                                    self.out.push_str(&name);
                                    self.out.push('\0');
                                }
                            }
                        }
                        self.walk(toks, i, Some(&n), lower);
                        continue;
                    }
                    if is_block_tag(&n) {
                        self.out.push('\n');
                        self.walk(toks, i, Some(&n), lower);
                        continue;
                    }
                    if is_bold_tag(&n) {
                        self.out.push_str("<b>");
                        self.walk(toks, i, Some(&n), lower);
                        self.out.push_str("</b>");
                        continue;
                    }
                    if is_italic_tag(&n) {
                        self.out.push_str("<i>");
                        self.walk(toks, i, Some(&n), lower);
                        self.out.push_str("</i>");
                        continue;
                    }
                    if n == "Ã" {
                        self.out.push_str("<span foreground=\"#9a9a9a\"><i>");
                        self.walk(toks, i, Some("Ã"), lower);
                        self.out.push_str("</i></span>");
                        continue;
                    }
                    self.walk(toks, i, Some(&n), lower);
                }
                Tok::Close { name } => {
                    let n = if lower {
                        name.to_lowercase()
                    } else {
                        name.clone()
                    };
                    *i += 1;
                    if let Some(c) = close_at {
                        if n == c {
                            return;
                        }
                        // 不匹配的闭标签（容忍畸形 XML）：忽略，继续当前元素。
                    }
                }
            }
        }
    }
}

/// 把 LD2 def 清洗为 Pango markup 字符串。
pub fn to_pango(def_xml: &str) -> String {
    if def_xml.trim().is_empty() {
        return String::new();
    }
    let cleaned = strip_control(def_xml);
    let lower = !looks_well_formed(&cleaned);
    let toks = tokenize(&cleaned);
    let mut w = Walker {
        out: String::new(),
        image_blocks: false,
    };
    let mut i = 0usize;
    w.walk(&toks, &mut i, None, lower);
    let mut result = clean_embedded_html(&w.out);
    if result.trim().is_empty() {
        return fallback(def_xml);
    }
    result = enhance_pango(result.trim());
    collapse_blank_lines(&result)
}

/// 切成有序块（文本 + 内联图片）。
pub fn to_blocks(def_xml: &str) -> Vec<Block> {
    if def_xml.trim().is_empty() {
        return Vec::new();
    }
    let src = strip_style_tags(def_xml);
    if !has_image_ref(&src) {
        let p = to_pango(def_xml);
        return if p.trim().is_empty() {
            Vec::new()
        } else {
            vec![Block::Text { pango: p }]
        };
    }
    let cleaned = strip_control(&src);
    let lower = !looks_well_formed(&cleaned);
    let toks = tokenize(&cleaned);
    let mut w = Walker {
        out: String::new(),
        image_blocks: true,
    };
    let mut i = 0usize;
    w.walk(&toks, &mut i, None, lower);
    let mut result = clean_embedded_html(&w.out);
    if result.trim().is_empty() {
        let p = to_pango(def_xml);
        return if p.trim().is_empty() {
            Vec::new()
        } else {
            vec![Block::Text { pango: p }]
        };
    }
    result = enhance_pango(result.trim());
    result = collapse_blank_lines(&result);

    let mut blocks = Vec::new();
    let mut pos = 0usize;
    let bytes = result.as_bytes();
    let mut j = 0usize;
    while j < bytes.len() {
        if bytes[j] == 0 && result[j..].starts_with("\0IMG:") {
            if let Some(off) = result[j + 5..].find('\0') {
                let name = &result[j + 5..j + 5 + off];
                let seg = result[pos..j].trim();
                if !seg.is_empty() {
                    blocks.push(Block::Text {
                        pango: seg.to_string(),
                    });
                }
                blocks.push(Block::Image {
                    name: name.to_string(),
                });
                j = j + 5 + off + 1;
                pos = j;
                continue;
            }
        }
        j += 1;
    }
    if pos < result.len() {
        let seg = result[pos..].trim();
        if !seg.is_empty() {
            blocks.push(Block::Text {
                pango: seg.to_string(),
            });
        }
    }
    blocks
}

fn strip_control(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            let cp = c as u32;
            !((0x00..=0x08).contains(&cp)
                || cp == 0x0B
                || cp == 0x0C
                || (0x0E..=0x1F).contains(&cp))
        })
        .collect()
}

fn looks_well_formed(src: &str) -> bool {
    // 粗略判断：标签配平、文本里没有裸 & / 非 XML 实体（lxml XML parser 会失败）。
    let mut depth: i32 = 0;
    let toks = tokenize(src);
    for t in &toks {
        match t {
            Tok::Open {
                name, self_closing, ..
            } if !*self_closing && !is_void(name) => depth += 1,
            Tok::Close { .. } => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Tok::Text(text) => {
                if !xml_entities_ok(text) {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn xml_entities_ok(text: &str) -> bool {
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        let tail = &rest[pos + 1..];
        let semi = match tail.find(';') {
            Some(s) => s,
            None => return false,
        };
        let ent = &tail[..semi];
        let ok = matches!(ent, "amp" | "lt" | "gt" | "quot" | "apos")
            || ent
                .strip_prefix("#x")
                .or_else(|| ent.strip_prefix("#X"))
                .map(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
                .unwrap_or(false)
            || ent
                .strip_prefix('#')
                .map(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false);
        if !ok {
            return false;
        }
        rest = &tail[semi + 1..];
    }
    true
}

fn is_void(name: &str) -> bool {
    matches!(name, "br" | "n" | "hr" | "img" | "Ë" | "e")
}

fn strip_style_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('<') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        // 只比较开头两个字节，避免对整段 tail 做 to_lowercase（O(n²)）。
        if tail.starts_with("<Ô")
            || tail.starts_with("<ô")
            || tail.starts_with("</Ô")
            || tail.starts_with("</ô")
        {
            if let Some(end) = tail.find('>') {
                rest = &tail[end + 1..];
                continue;
            }
        }
        out.push('<');
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

fn has_image_ref(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("<ë") || lower.contains("<img") || lower.contains("dict://res/")
}

fn fallback(def_xml: &str) -> String {
    // 去标签 + 压缩空白（与 Python 的 _fallback 一致）。
    let no_tags = strip_tags(def_xml);
    let collapsed = no_tags.split_whitespace().collect::<Vec<_>>().join(" ");
    escape(&collapsed)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// 嵌入 HTML 二次清洗（Cambridge/Collins 等把 HTML 作为文本节点嵌入 def）
// ---------------------------------------------------------------------------

fn clean_embedded_html(s: &str) -> String {
    if !s.contains("&lt;") && !has_embedded_html_tag(s) {
        return s.to_string();
    }
    let mut stash: Vec<String> = Vec::new();
    let protected = protect_pango_tags(s, &mut stash);
    let unescaped = unescape(&protected);
    let cleaned = html_to_pango(&unescaped);
    restore_placeholders(&cleaned, &stash)
}

fn has_embedded_html_tag(s: &str) -> bool {
    let lower = s.to_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            let rest = &lower[j..];
            for tag in [
                "div", "font", "a", "p", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6",
            ] {
                if rest.starts_with(tag) {
                    let after = rest.as_bytes().get(tag.len()).copied();
                    let word_end = match after {
                        None => true,
                        Some(b) => !(b.is_ascii_alphanumeric() || b == b'_'),
                    };
                    if word_end {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn protect_pango_tags(s: &str, stash: &mut Vec<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0usize;
    'outer: while i < bytes.len() {
        if bytes[i] == b'<' {
            let tail = &s[i..];
            for tag in ["<b", "</b", "<i", "</i", "<u", "</u", "<span", "</span"] {
                if tail.starts_with(tag) {
                    if let Some(end) = tail.find('>') {
                        let piece = &tail[..=end];
                        out.push_str(&format!("\u{E000}{}\u{E001}", stash.len()));
                        stash.push(piece.to_string());
                        i += end + 1;
                        continue 'outer;
                    }
                }
            }
        }
        let c = match s[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        out.push(c);
        i += c.len_utf8();
    }
    out
}

fn restore_placeholders(s: &str, stash: &[String]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{E000}' {
            let mut num = String::new();
            while let Some(&n) = chars.peek() {
                if n == '\u{E001}' {
                    chars.next();
                    break;
                }
                num.push(n);
                chars.next();
            }
            if let Ok(idx) = num.parse::<usize>() {
                if let Some(v) = stash.get(idx) {
                    out.push_str(v);
                    continue;
                }
            }
            out.push(c);
        } else {
            out.push(c);
        }
    }
    out
}

fn html_to_pango(raw: &str) -> String {
    let toks = tokenize_html(raw);
    let mut out = String::new();
    walk_html(&toks, &mut out);
    out
}

fn is_html_block(t: &str) -> bool {
    matches!(
        t,
        "div" | "p" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

fn walk_html(toks: &[Tok], out: &mut String) {
    let mut i = 0usize;
    let mut stack: Vec<(String, bool, bool, bool)> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::Text(t) => {
                out.push_str(&escape(&unescape(t)));
                i += 1;
            }
            Tok::Open {
                name,
                attrs,
                self_closing,
            } => {
                let n = name.to_lowercase();
                i += 1;
                if *self_closing {
                    if is_html_block(&n) {
                        out.push('\n');
                        // lxml 把 <p/> 当作空元素：后随文本是它的 tail，而 _walk_html
                        // 对块级元素的 tail 只补换行、丢弃文本。逐字复刻该行为。
                        if let Some(Tok::Text(_)) = toks.get(i) {
                            i += 1;
                            out.push('\n');
                        }
                    } else if n == "br" {
                        out.push('\n');
                    }
                    continue;
                }
                if n == "br" {
                    out.push('\n');
                    continue;
                }
                let block = is_html_block(&n);
                let mut bold = matches!(n.as_str(), "b" | "strong");
                let mut italic = matches!(n.as_str(), "i" | "em");
                let underline = matches!(n.as_str(), "u" | "a");
                if matches!(n.as_str(), "span" | "font" | "a") {
                    if let Some(style) = attr(attrs, "style") {
                        let (b, it) = style_flags(style);
                        bold |= b;
                        italic |= it;
                    }
                }
                if block {
                    out.push('\n');
                }
                if bold {
                    out.push_str("<b>");
                }
                if italic {
                    out.push_str("<i>");
                }
                if underline {
                    out.push_str("<u>");
                }
                stack.push((n, bold, italic, underline));
            }
            Tok::Close { name } => {
                let n = name.to_lowercase();
                i += 1;
                // 与 Python _walk_html 一致：块级元素若有 tail 文本，只补一个换行并丢弃 tail。
                if is_html_block(&n) {
                    if let Some(Tok::Text(_)) = toks.get(i) {
                        i += 1;
                        out.push('\n');
                    }
                }
                if let Some(pos) = stack.iter().rposition(|(t, ..)| *t == n) {
                    while stack.len() > pos {
                        let (_t, bold, italic, underline) = stack.pop().unwrap();
                        if underline {
                            out.push_str("</u>");
                        }
                        if italic {
                            out.push_str("</i>");
                        }
                        if bold {
                            out.push_str("</b>");
                        }
                    }
                }
            }
        }
    }
    while let Some((_t, bold, italic, underline)) = stack.pop() {
        if underline {
            out.push_str("</u>");
        }
        if italic {
            out.push_str("</i>");
        }
        if bold {
            out.push_str("</b>");
        }
    }
}

fn style_flags(style: &str) -> (bool, bool) {
    let s = style.to_lowercase();
    let bold = s.contains("font-weight")
        && (s.contains("bold") || s.contains("700") || s.contains("800") || s.contains("900"));
    let italic = s.contains("font-style") && s.contains("italic");
    (bold, italic)
}

// ---------------------------------------------------------------------------
// 排版增强：词性加粗、斜杠音标斜体（只作用于标签外文本）
// ---------------------------------------------------------------------------

fn enhance_pango(s: &str) -> String {
    if !s.contains('<') {
        return enhance_segment(s);
    }
    let mut out = String::new();
    let mut pos = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if let Some(end) = s[i..].find('>') {
                if i > pos {
                    out.push_str(&enhance_segment(&s[pos..i]));
                }
                out.push_str(&s[i..=i + end]);
                i += end + 1;
                pos = i;
                continue;
            }
        }
        i += 1;
    }
    if pos < s.len() {
        out.push_str(&enhance_segment(&s[pos..]));
    }
    out
}

fn enhance_segment(seg: &str) -> String {
    let seg = phonetic_italic(seg);
    pos_bold(&seg)
}

fn phonetic_italic(seg: &str) -> String {
    // (?<!\w)/[^/\s\n]{1,30}/(?!\w)
    let chars: Vec<char> = seg.chars().collect();
    let mut out = String::with_capacity(seg.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' {
            let prev_ok = i == 0 || !is_word_char(chars[i - 1]);
            if prev_ok {
                let mut j = i + 1;
                let mut ok = true;
                while j < chars.len() {
                    let d = chars[j];
                    if d == '/' {
                        break;
                    }
                    if d.is_whitespace() || d == '\n' {
                        ok = false;
                        break;
                    }
                    if j - i > 31 {
                        ok = false;
                        break;
                    }
                    j += 1;
                }
                if ok && j < chars.len() && chars[j] == '/' && (j - i - 1) >= 1 {
                    let next_ok = j + 1 >= chars.len() || !is_word_char(chars[j + 1]);
                    if next_ok {
                        out.push_str("<i>");
                        for d in &chars[i..=j] {
                            out.push(*d);
                        }
                        out.push_str("</i>");
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn pos_bold(seg: &str) -> String {
    // 与 Python 正则 (?<![\w])(n|v|vi|vt|adj|adv|prep|conj|pron|num|art|aux|abbr)\.
    // 一致：前一个字符不能是 word 字符；按最长优先匹配。
    const POS: [&str; 13] = [
        "abbr", "adj", "adv", "prep", "conj", "pron", "num", "art", "aux", "vi", "vt", "n", "v",
    ];
    let chars: Vec<char> = seg.chars().collect();
    let mut out = String::with_capacity(seg.len());
    let mut i = 0usize;
    while i < chars.len() {
        let prev_ok = i == 0 || !is_word_char(chars[i - 1]);
        if prev_ok {
            let mut matched = None;
            for pos in POS {
                let pc: Vec<char> = pos.chars().collect();
                if chars.len() > i + pc.len()
                    && chars[i..i + pc.len()]
                        .iter()
                        .zip(&pc)
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                    && chars[i + pc.len()] == '.'
                {
                    matched = Some(pc.len() + 1);
                    break;
                }
            }
            if let Some(len) = matched {
                out.push_str("<b>");
                for d in &chars[i..i + len] {
                    out.push(*d);
                }
                out.push_str("</b>");
                i += len;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut nl = 0usize;
    for c in s.chars() {
        if c == '\n' {
            nl += 1;
            if nl <= 1 {
                out.push(c);
            }
        } else {
            nl = 0;
            out.push(c);
        }
    }
    out
}
