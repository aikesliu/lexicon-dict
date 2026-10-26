//! 查询策略：精确匹配 → 大小写变体 → 前缀 fallback。
//!
//! 按 dict order 遍历已加载词典，每个词典独立判定 match_type 与 score；
//! 聚合结果按 score 降序排序（准确度优先），同分保持 dict order（稳定排序）。

use std::collections::HashMap;

use crate::common::LookupResult;
use crate::dictset::DictSet;

/// 生成去重的大小写变体（保序），覆盖 hello/Hello/HELLO/HellO 互查。
pub fn case_variants(word: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(6);
    let mut seen = std::collections::HashSet::new();
    let mut push = |v: String| {
        if !v.is_empty() && seen.insert(v.clone()) {
            out.push(v);
        }
    };
    push(word.to_string());
    push(word.to_lowercase());
    push(word.to_uppercase());
    push(capitalize(word));
    push(title(word));
    push(casefold(word));
    out
}

pub fn capitalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut first = true;
    for c in s.chars() {
        if first {
            for u in c.to_uppercase() {
                out.push(u);
            }
            first = false;
        } else {
            for l in c.to_lowercase() {
                out.push(l);
            }
        }
    }
    out
}

/// 近似 Python `str.title()`：每个「非大小写字符」之后的首个字母大写。
pub fn title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_cased = false;
    for c in s.chars() {
        if c.is_alphabetic() {
            if prev_cased {
                for l in c.to_lowercase() {
                    out.push(l);
                }
            } else {
                for u in c.to_uppercase() {
                    out.push(u);
                }
            }
            prev_cased = true;
        } else {
            out.push(c);
            prev_cased = false;
        }
    }
    out
}

/// 近似 Python `str.casefold()`（覆盖常见西文；ß→ss，ſ→s）。
pub fn casefold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' | 'ẞ' => out.push_str("ss"),
            'ſ' => out.push('s'),
            _ => {
                for l in c.to_lowercase() {
                    out.push(l);
                }
            }
        }
    }
    out
}

/// 匹配度评分：exact(1.0) > casefold(0.9) > prefix(0.4~0.8)。
///
/// prefix 档按查询词与命中词的相似度浮动，避免所有前缀命中同等权重。
fn score(match_type: &str, hit_word: &str, word: &str) -> f64 {
    match match_type {
        "exact" => 1.0,
        "casefold" => 0.9,
        _ => {
            let sim = sequence_ratio(&casefold(word), &casefold(hit_word));
            round3(0.4 + 0.4 * sim)
        }
    }
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round_ties_even() / 1000.0
}

pub struct QueryOpts {
    pub case_insensitive: bool,
    pub prefix_fallback: bool,
    pub prefix_match_limit: usize,
}

impl Default for QueryOpts {
    fn default() -> Self {
        QueryOpts {
            case_insensitive: true,
            prefix_fallback: true,
            prefix_match_limit: 8,
        }
    }
}

/// 单词典查询：精确 → 大小写变体 → 前缀 fallback。
pub fn query_one(dictset: &DictSet, did: &str, word: &str, opts: &QueryOpts) -> Vec<LookupResult> {
    let sd = match dictset.handle(did) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let info = match dictset.info(did) {
        Some(i) => i,
        None => return Vec::new(),
    };
    let candidates = if opts.case_insensitive {
        case_variants(word)
    } else {
        vec![word.to_string()]
    };
    for v in &candidates {
        if let Some(defn) = sd.lookup(v) {
            let mt = if v == word { "exact" } else { "casefold" };
            return vec![LookupResult {
                word: v.clone(),
                dict_id: did.to_string(),
                dict_display_name: info.display_name.clone(),
                markup: defn,
                match_type: mt.to_string(),
                score: score(mt, v, word),
            }];
        }
    }
    if opts.prefix_fallback && opts.prefix_match_limit > 0 {
        let mut results = Vec::new();
        for s in sd.suggest(word, opts.prefix_match_limit) {
            if let Some(defn) = sd.lookup(&s) {
                results.push(LookupResult {
                    word: s.clone(),
                    dict_id: did.to_string(),
                    dict_display_name: info.display_name.clone(),
                    markup: defn,
                    match_type: "prefix".to_string(),
                    score: score("prefix", &s, word),
                });
            }
        }
        return results;
    }
    Vec::new()
}

/// 按 dict order 聚合所有词典结果。
pub fn query(dictset: &DictSet, word: &str, opts: &QueryOpts) -> Vec<LookupResult> {
    let mut results = Vec::new();
    for did in dictset.loaded_ids() {
        results.extend(query_one(dictset, did, word, opts));
    }
    // 准确度优先：按 score 降序；同分保持 dict order（Rust sort_by 稳定）。
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

// ---------------------------------------------------------------------------
// difflib.SequenceMatcher.ratio 的移植（前缀评分需要与 Python 完全一致）
// ---------------------------------------------------------------------------

pub fn sequence_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let matched = matching_chars(&a, &b);
    2.0 * matched as f64 / total as f64
}

fn matching_chars(a: &[char], b: &[char]) -> usize {
    let sm = SequenceMatcher::new(a, b);
    let mut queue = vec![(0usize, a.len(), 0usize, b.len())];
    let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = sm.find_longest_match(alo, ahi, blo, bhi);
        if k > 0 {
            blocks.push((i, j, k));
            queue.push((alo, i, blo, j));
            queue.push((i + k, ahi, j + k, bhi));
        }
    }
    blocks.iter().map(|&(_, _, k)| k).sum()
}

struct SequenceMatcher<'a> {
    a: &'a [char],
    b: &'a [char],
    b2j: HashMap<char, Vec<usize>>,
    bjunk: std::collections::HashSet<char>,
}

impl<'a> SequenceMatcher<'a> {
    fn new(a: &'a [char], b: &'a [char]) -> Self {
        let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
        for (i, &c) in b.iter().enumerate() {
            b2j.entry(c).or_default().push(i);
        }
        let mut bjunk = std::collections::HashSet::new();
        if b.len() >= 200 {
            let ntest = b.len() / 100 + 1;
            for (&c, idxs) in b2j.iter() {
                if idxs.len() > ntest {
                    bjunk.insert(c);
                }
            }
            for c in &bjunk {
                b2j.remove(c);
            }
        }
        SequenceMatcher { a, b, b2j, bjunk }
    }

    fn find_longest_match(
        &self,
        alo: usize,
        ahi: usize,
        blo: usize,
        bhi: usize,
    ) -> (usize, usize, usize) {
        let mut besti = alo;
        let mut bestj = blo;
        let mut bestsize = 0usize;
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut newj2len: HashMap<usize, usize> = HashMap::new();
            if let Some(js) = self.b2j.get(&self.a[i]) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = j2len.get(&j.saturating_sub(1)).copied().unwrap_or(0) + 1;
                    newj2len.insert(j, k);
                    if k > bestsize {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        bestsize = k;
                    }
                }
            }
            j2len = newj2len;
        }
        while besti > alo
            && bestj > blo
            && !self.bjunk.contains(&self.b[bestj - 1])
            && self.a[besti - 1] == self.b[bestj - 1]
        {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && !self.bjunk.contains(&self.b[bestj + bestsize])
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
            bestsize += 1;
        }
        while besti > alo
            && bestj > blo
            && self.bjunk.contains(&self.b[bestj - 1])
            && self.a[besti - 1] == self.b[bestj - 1]
        {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && self.bjunk.contains(&self.b[bestj + bestsize])
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_matches_python_examples() {
        // 以下值来自 Python difflib.SequenceMatcher(None, a, b).ratio()
        assert!((sequence_ratio("abcd", "bcde") - 0.75).abs() < 1e-12);
        assert!((sequence_ratio("hello", "hello") - 1.0).abs() < 1e-12);
        assert_eq!(sequence_ratio("", ""), 1.0);
        let r = sequence_ratio("re", "regain");
        assert!((r - 0.5).abs() < 1e-12, "got {r}");
    }

    #[test]
    fn variants_cover_case_forms() {
        let v = case_variants("hello");
        assert!(v.contains(&"Hello".to_string()));
        assert!(v.contains(&"HELLO".to_string()));
        assert_eq!(title("they're"), "They'Re");
    }
}
