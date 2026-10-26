//! ld2 → StarDict 转换。
//!
//! 增量：按 source sha256 命中 manifest 且已提取过资源则跳过；?LDF 变体识别后跳过并标记。
//! 输出与兼容实现逐字节一致（.ifo/.idx/.dict 可被其他 StarDict 读取器读取）。

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::common::DictInfo;
use crate::error::{Error, Result};
use crate::manifest::{Manifest, info_to_value};
use crate::stardict::write_stardict;

const CHUNK: usize = 1 << 20;

#[derive(Debug, Default, Clone)]
pub struct ConvertStats {
    pub total: usize,
    pub skipped: usize,
    pub converted: usize,
    pub ok: usize,
    pub unsupported: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ConvertOptions {
    pub force: bool,
    pub jobs: Option<usize>,
    pub verify: bool,
    pub clean: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            force: false,
            jobs: None,
            verify: false,
            clean: false,
        }
    }
}

/// 文件名转 ascii-safe slug：小写、非 [a-z0-9] → '_'，合并连续下划线，去首尾。
/// 纯非 ascii 名（结果为空）回退 'd' + sha256(name)[:10]。
pub fn slugify(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_us = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            last_us = false;
        } else if !last_us {
            out.push('_');
            last_us = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        let mut h = Sha256::new();
        h.update(name.as_bytes());
        let hex = hex_digest(h.finalize().as_slice());
        format!("d{}", &hex[..10])
    } else {
        trimmed
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// 返回 (sha256, mtime, size)。
pub fn fingerprint(p: &Path) -> Result<(String, f64, u64)> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let st = std::fs::metadata(p)?;
    let mtime = st
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    Ok((hex_digest(h.finalize().as_slice()), mtime, st.len()))
}

fn walk_ld2(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let p = ent.path();
        let ft = match ent.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk_ld2(&p, out);
        } else if ft.is_file() && p.extension().and_then(|e| e.to_str()) == Some("ld2") {
            out.push(p);
        }
    }
}

fn collect_inputs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for d in dirs {
        walk_ld2(d, &mut files);
    }
    files.sort();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for p in files {
        let key = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
        if seen.insert(key) {
            out.push(p);
        }
    }
    out
}

fn verify_against_ld2(
    d: &crate::ld2::Dictionary,
    out_dir: &Path,
    slug: &str,
    n: usize,
) -> (usize, usize) {
    let ifo = out_dir.join(slug).join(format!("{slug}.ifo"));
    let sd = match crate::stardict::StarDict::open(&ifo) {
        Ok(s) => s,
        Err(_) => return (0, n),
    };
    let total = d.entry_count;
    if total == 0 || n == 0 {
        return (0, 0);
    }
    let step = (total / n).max(1);
    let mut mismatch = 0;
    let mut missing = 0;
    let mut checked = 0;
    let mut i = 0;
    while i < total && checked < n {
        if let Ok(e) = d.entry(i) {
            match sd.lookup(&e.word) {
                None => missing += 1,
                Some(got) if got != e.definition => mismatch += 1,
                _ => {}
            }
            checked += 1;
        }
        i += step;
    }
    (mismatch, missing)
}

fn convert_one(
    ld2_path: &Path,
    out_dir: &Path,
    slug: &str,
    bookname: &str,
    verify_n: usize,
) -> (String, DictInfo) {
    let mut info = DictInfo {
        display_name: bookname.to_string(),
        source_path: ld2_path.to_string_lossy().into_owned(),
        status: "ok".into(),
        idxoffsetbits: 32,
        ..Default::default()
    };
    let (sha, mtime, size) = match fingerprint(ld2_path) {
        Ok(v) => v,
        Err(e) => {
            info.status = "error".into();
            info.error = format!("fingerprint: {e}");
            return (slug.to_string(), info);
        }
    };
    info.source_sha256 = sha;
    info.source_mtime = mtime;
    info.source_size = size;

    let d = match crate::ld2::open(ld2_path) {
        Ok(d) => d,
        Err(Error::UnsupportedVariant { magic, path, hint }) => {
            info.status = "unsupported_ldf".into();
            info.error = Error::UnsupportedVariant { magic, path, hint }.to_string();
            return (slug.to_string(), info);
        }
        Err(e) => {
            info.status = "error".into();
            info.error = format!("open: {e}");
            return (slug.to_string(), info);
        }
    };

    let entries: Vec<(String, String)> = match d.iter().collect::<Result<Vec<_>>>() {
        Ok(v) => v.into_iter().map(|e| (e.word, e.definition)).collect(),
        Err(e) => {
            info.status = "error".into();
            info.error = format!("read: {e}");
            return (slug.to_string(), info);
        }
    };
    let stat = match write_stardict(out_dir, slug, bookname, entries, None) {
        Ok(s) => s,
        Err(e) => {
            info.status = "error".into();
            info.error = format!("write: {e}");
            return (slug.to_string(), info);
        }
    };
    info.entry_count = stat.wordcount as u64;
    info.word_encoding = d.word_encoding.name().to_string();
    info.def_encoding = d.def_encoding.name().to_string();
    info.idxoffsetbits = stat.idxoffsetbits;

    // 提取内嵌资源（说文解字等带字形图的词典）到 <slug>/res/。
    let resources = d.extract_resources();
    if !resources.is_empty() {
        let res_dir = out_dir.join(slug).join("res");
        if let Err(e) = std::fs::create_dir_all(&res_dir) {
            info.error = format!("resources: {e}");
        } else {
            for (name, data) in &resources {
                // 只允许扁平文件名，避免畸形资源名逃逸目录。
                if name.contains('/') || name.contains('\\') || name.contains("..") {
                    continue;
                }
                if let Err(e) = std::fs::write(res_dir.join(name), data) {
                    info.error = format!("resources: {e}");
                    break;
                }
            }
        }
    }
    info.res_count = resources.len() as u64;
    info.res_extracted = true;

    if verify_n > 0 {
        let (mismatch, missing) = verify_against_ld2(&d, out_dir, slug, verify_n);
        if mismatch > 0 || missing > 0 {
            info.error = format!("verify warn: mismatch={mismatch} missing={missing}");
        }
    }
    (slug.to_string(), info)
}

fn bookname_for(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 转换一个或多个目录下所有 *.ld2（递归子目录，按绝对路径去重）。
pub fn convert_directory(
    input_dirs: &[PathBuf],
    out_dir: &Path,
    opts: ConvertOptions,
) -> Result<ConvertStats> {
    if opts.clean && out_dir.exists() {
        std::fs::remove_dir_all(out_dir)?;
    }
    std::fs::create_dir_all(out_dir)?;

    let files = collect_inputs(input_dirs);
    let mut manifest = Manifest::load(out_dir);
    let existing = manifest.dicts.clone();
    let mut skipped = 0usize;
    let mut tasks: Vec<(PathBuf, String, String)> = Vec::new();

    for p in &files {
        let slug = slugify(&bookname_for(p));
        let (sha, mt, sz) = match fingerprint(p) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let cached = existing.get(&slug);
        let hit = cached
            .and_then(|v| {
                let ok_sha = v.get("source_sha256").and_then(|s| s.as_str()) == Some(sha.as_str());
                let res = v.get("res_extracted").and_then(|b| b.as_bool()) == Some(true);
                Some(ok_sha && res)
            })
            .unwrap_or(false);
        if hit && !opts.force {
            skipped += 1;
            if let Some(v) = manifest.dicts.get_mut(&slug) {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("source_mtime".into(), Value::from(mt));
                    obj.insert("source_size".into(), Value::from(sz));
                }
            }
            continue;
        }
        tasks.push((p.clone(), slug, bookname_for(p)));
    }

    let verify_n = if opts.verify { 30 } else { 0 };
    let jobs = opts
        .jobs
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        })
        .max(1)
        .min(tasks.len().max(1));

    let results: Vec<(String, DictInfo)> = if tasks.is_empty() {
        Vec::new()
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build()
            .map_err(|e| Error::Other(format!("thread pool: {e}")))?;
        pool.install(|| {
            tasks
                .par_iter()
                .map(|(p, slug, book)| convert_one(p, out_dir, slug, book, verify_n))
                .collect()
        })
    };

    for (slug, info) in &results {
        manifest.dicts.insert(slug.clone(), info_to_value(info));
    }
    manifest.save(out_dir)?;

    let mut stats = ConvertStats {
        total: files.len(),
        skipped,
        converted: results.len(),
        ..Default::default()
    };
    let mut ok = 0;
    for (_slug, _info) in manifest.infos() {
        match _info.status.as_str() {
            "ok" => ok += 1,
            "unsupported_ldf" => stats.unsupported.push(_slug),
            "error" => stats.errors.push(_slug),
            _ => {}
        }
    }
    stats.ok = ok;
    Ok(stats)
}

/// 读取 manifest 并返回 slug → DictInfo（按 manifest 顺序）。
pub fn list_dicts(out_dir: &Path) -> BTreeMap<String, DictInfo> {
    // 注意：调用方若需要顺序，用 Manifest::infos；此函数仅用于无序查询。
    Manifest::load(out_dir).infos().into_iter().collect()
}
