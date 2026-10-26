//! lexicon-cli：转换 / 查词 / 词典管理（跨平台，无 GUI 依赖）。
//!
//! 用法：
//!   lexicon-cli convert [目录...] [-o 输出目录] [--force] [-j N] [--verify] [--clean]
//!   lexicon-cli query  <单词> [--cache 目录] [--json]
//!   lexicon-cli list   [--cache 目录] [--json]
//!   lexicon-cli dump-markup --dict <file.ld2> [--limit N] [--offset N] [--blocks]
//!   lexicon-cli dump-resources --dict <file.ld2> -o <目录>

use std::path::PathBuf;
use std::process::ExitCode;

use lexicon_core::config::load_config;
use lexicon_core::convert::{ConvertOptions, convert_directory};
use lexicon_core::dictset::DictSet;
use lexicon_core::manifest::Manifest;
use lexicon_core::paths::Paths;
use lexicon_core::query::QueryOpts;

fn usage() -> &'static str {
    "lexicon-cli 0.1.0 — LD2 词典工具\n\
     \n\
     convert [目录...] [-o 输出目录] [--force] [-j N] [--verify] [--clean]\n\
     query  <单词> [--cache 目录] [--json]\n\
     list   [--cache 目录] [--json]\n\
     dump-markup --dict <file.ld2> [--limit N] [--offset N] [--blocks]\n\
     dump-resources --dict <file.ld2> -o <目录>\n"
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print!("{}", usage());
        return ExitCode::SUCCESS;
    }
    if args[0] == "--version" || args[0] == "-V" {
        println!("{}", lexicon_core::VERSION);
        return ExitCode::SUCCESS;
    }
    let result = match args[0].as_str() {
        "convert" => cmd_convert(&args[1..]),
        "query" => cmd_query(&args[1..]),
        "list" => cmd_list(&args[1..]),
        "dump-markup" => cmd_dump_markup(&args[1..]),
        "dump-resources" => cmd_dump_resources(&args[1..]),
        "lookup" => cmd_lookup(&args[1..]),
        "bench" => cmd_bench(&args[1..]),
        "suggest" => cmd_suggest(&args[1..]),
        other => {
            eprintln!("未知命令: {other}\n\n{}", usage());
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::from(1)
        }
    }
}

fn take_value(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or_else(|| "缺少参数值".to_string())
}

fn default_cache() -> PathBuf {
    let mut cfg = load_config(&Paths::discover().config_path);
    cfg.apply_env_overrides();
    cfg.cache_dir
}

fn cmd_convert(args: &[String]) -> Result<ExitCode, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut out = Paths::discover().stardict_dir;
    let mut opts = ConvertOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out-dir" => out = PathBuf::from(take_value(args, &mut i)?),
            "--force" => opts.force = true,
            "-j" | "--jobs" => {
                let v = take_value(args, &mut i)?;
                opts.jobs = Some(v.parse().map_err(|_| format!("无效 jobs: {v}"))?);
            }
            "--verify" => opts.verify = true,
            "--clean" => opts.clean = true,
            "-h" | "--help" => {
                print!("{}", usage());
                return Ok(ExitCode::SUCCESS);
            }
            other => dirs.push(PathBuf::from(other)),
        }
        i += 1;
    }
    if dirs.is_empty() {
        let cfg = load_config(&Paths::discover().config_path);
        dirs = cfg.source_dirs;
    }
    let stats = convert_directory(&dirs, &out, opts).map_err(|e| e.to_string())?;
    println!(
        "发现 {} 个 ld2：跳过 {}，转换 {}",
        stats.total, stats.skipped, stats.converted
    );
    println!(
        "完成：ok={} unsupported={} error={}",
        stats.ok,
        stats.unsupported.len(),
        stats.errors.len()
    );
    if !stats.unsupported.is_empty() {
        println!("  跳过(?LDF): {}", stats.unsupported.join(", "));
    }
    if !stats.errors.is_empty() {
        eprintln!("  失败: {}", stats.errors.join(", "));
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_query(args: &[String]) -> Result<ExitCode, String> {
    let mut word = String::new();
    let mut cache = default_cache();
    let mut json = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cache" => cache = PathBuf::from(take_value(args, &mut i)?),
            "--json" => json = true,
            other => word = other.to_string(),
        }
        i += 1;
    }
    if word.is_empty() {
        return Err("缺少要查询的单词".into());
    }
    let cfg = load_config(&Paths::discover().config_path);
    let mut ds = DictSet::new(&cache);
    ds.reload(&cfg);
    let opts = QueryOpts {
        case_insensitive: cfg.case_insensitive,
        prefix_fallback: cfg.prefix_fallback,
        prefix_match_limit: cfg.prefix_match_limit,
    };
    let results = ds.lookup(&word, &opts);
    if json {
        let arr: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "word": r.word,
                    "dict_id": r.dict_id,
                    "dict_display_name": r.dict_display_name,
                    "markup": r.markup,
                    "match_type": r.match_type,
                    "score": r.score,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr).unwrap());
    } else if results.is_empty() {
        println!("未找到「{word}」");
    } else {
        for r in &results {
            println!(
                "── {} [{}] ({}, {:.3})",
                r.word, r.dict_display_name, r.match_type, r.score
            );
            println!("{}", lexicon_core::markup::to_pango(&r.markup));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_list(args: &[String]) -> Result<ExitCode, String> {
    let mut cache = default_cache();
    let mut json = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cache" => cache = PathBuf::from(take_value(args, &mut i)?),
            "--json" => json = true,
            other => return Err(format!("未知参数: {other}")),
        }
        i += 1;
    }
    let manifest = Manifest::load(&cache);
    let infos = manifest.infos();
    if json {
        let arr: Vec<serde_json::Value> = infos
            .iter()
            .map(|(slug, info)| {
                serde_json::json!({
                    "id": slug,
                    "display_name": info.display_name,
                    "entry_count": info.entry_count,
                    "status": info.status,
                    "source_path": info.source_path,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&arr).unwrap());
    } else {
        for (slug, info) in &infos {
            println!(
                "{:<40} {:>10} 条  {:<10} {}",
                slug, info.entry_count, info.status, info.display_name
            );
        }
        println!("共 {} 个词典，缓存目录 {}", infos.len(), cache.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_dump_markup(args: &[String]) -> Result<ExitCode, String> {
    let mut dict: Option<PathBuf> = None;
    let mut limit = 50usize;
    let mut offset = 0usize;
    let mut blocks = false;
    let mut indices: Option<Vec<usize>> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--dict" => dict = Some(PathBuf::from(take_value(args, &mut i)?)),
            "--limit" => {
                limit = take_value(args, &mut i)?
                    .parse()
                    .map_err(|_| "无效 limit")?
            }
            "--offset" => {
                offset = take_value(args, &mut i)?
                    .parse()
                    .map_err(|_| "无效 offset")?
            }
            "--blocks" => blocks = true,
            "--indices" => {
                let v = take_value(args, &mut i)?;
                let mut list = Vec::new();
                for part in v.split(',') {
                    if part.trim().is_empty() {
                        continue;
                    }
                    list.push(
                        part.trim()
                            .parse()
                            .map_err(|_| format!("无效索引: {part}"))?,
                    );
                }
                indices = Some(list);
            }
            other => return Err(format!("未知参数: {other}")),
        }
        i += 1;
    }
    let dict = dict.ok_or("缺少 --dict")?;
    let d = lexicon_core::ld2::open(&dict).map_err(|e| e.to_string())?;
    let picks: Vec<usize> = match &indices {
        Some(v) => v.iter().copied().filter(|&i| i < d.entry_count).collect(),
        None => (offset..(offset + limit).min(d.entry_count)).collect(),
    };
    for idx in picks {
        let e = d.entry(idx).map_err(|e| e.to_string())?;
        let val = if blocks {
            let bs: Vec<serde_json::Value> = lexicon_core::markup::to_blocks(&e.definition)
                .into_iter()
                .map(|b| match b {
                    lexicon_core::Block::Text { pango } => {
                        serde_json::json!({"kind": "text", "pango": pango})
                    }
                    lexicon_core::Block::Image { name } => {
                        serde_json::json!({"kind": "image", "name": name})
                    }
                })
                .collect();
            serde_json::json!({"i": idx, "word": e.word, "blocks": bs})
        } else {
            serde_json::json!({
                "i": idx,
                "word": e.word,
                "raw": e.definition,
                "pango": lexicon_core::markup::to_pango(&e.definition),
            })
        };
        println!("{}", serde_json::to_string(&val).unwrap());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_bench(args: &[String]) -> Result<ExitCode, String> {
    let mut cache = default_cache();
    let mut word = "computer".to_string();
    let mut n = 100usize;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--cache" => cache = PathBuf::from(take_value(args, &mut i)?),
            "--n" => n = take_value(args, &mut i)?.parse().map_err(|_| "无效 n")?,
            other => word = other.to_string(),
        }
        i += 1;
    }
    let mut cfg = load_config(&Paths::discover().config_path);
    cfg.cache_dir = cache;
    let mut ds = DictSet::new(&cfg.cache_dir);
    ds.reload(&cfg);
    let opts = QueryOpts {
        case_insensitive: cfg.case_insensitive,
        prefix_fallback: cfg.prefix_fallback,
        prefix_match_limit: cfg.prefix_match_limit,
    };
    // 预热
    let warm = ds.lookup(&word, &opts);
    let t0 = std::time::Instant::now();
    let mut total = 0usize;
    for _ in 0..n {
        total += ds.lookup(&word, &opts).len();
    }
    let lookup_ms = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
    let t1 = std::time::Instant::now();
    let mut bytes = 0usize;
    for _ in 0..n {
        for r in &warm {
            let p = lexicon_core::markup::to_pango(&r.markup);
            bytes += p.len();
        }
    }
    let markup_ms = t1.elapsed().as_secs_f64() * 1000.0 / n as f64;
    let t2 = std::time::Instant::now();
    let mut blocks = 0usize;
    for _ in 0..n {
        for r in &warm {
            blocks += lexicon_core::markup::to_blocks(&r.markup).len();
        }
    }
    let blocks_ms = t2.elapsed().as_secs_f64() * 1000.0 / n as f64;
    println!("to_blocks={blocks_ms:.3}ms/次 blocks={}", blocks / n.max(1));
    println!(
        "dicts={} word={word:?} results={} lookup={:.3}ms/次 markup={:.3}ms/次 ({} bytes/次)",
        ds.loaded_ids().len(),
        warm.len(),
        lookup_ms,
        markup_ms,
        bytes / n.max(1)
    );
    println!("累计结果数 {total}");
    // 单词典耗时（定位慢词典）
    let mut per: Vec<(String, f64, usize)> = Vec::new();
    for did in ds.loaded_ids() {
        let t = std::time::Instant::now();
        let mut cnt = 0;
        for _ in 0..n {
            cnt += lexicon_core::query::query_one(&ds, did, &word, &opts).len();
        }
        per.push((
            did.to_string(),
            t.elapsed().as_secs_f64() * 1000.0 / n as f64,
            cnt / n.max(1),
        ));
    }
    per.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    for (did, ms, cnt) in per.iter().take(8) {
        println!("  {ms:8.3}ms  x{cnt:<3} {did}");
    }
    // 慢词典内部拆解
    if let Some((did, _, _)) = per.first() {
        if let Some(sd) = ds.handle(did) {
            println!("  --- {did} 拆解");
            let variants = if cfg.case_insensitive {
                lexicon_core::query::case_variants(&word)
            } else {
                vec![word.clone()]
            };
            for v in &variants {
                let t = std::time::Instant::now();
                let r = sd.lookup(v);
                println!(
                    "    lookup({v:?}) = {:?} in {:.3}ms",
                    r.as_ref().map(|s| s.len()),
                    t.elapsed().as_secs_f64() * 1000.0
                );
            }
            let t = std::time::Instant::now();
            let sug = sd.suggest(&word, cfg.prefix_match_limit);
            println!(
                "    suggest({word:?}) -> {} 条 in {:.3}ms",
                sug.len(),
                t.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_lookup(args: &[String]) -> Result<ExitCode, String> {
    let mut ifo: Option<PathBuf> = None;
    let mut word = String::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--ifo" => ifo = Some(PathBuf::from(take_value(args, &mut i)?)),
            other => word = other.to_string(),
        }
        i += 1;
    }
    let ifo = ifo.ok_or("缺少 --ifo")?;
    let sd = lexicon_core::stardict::StarDict::open(&ifo).map_err(|e| e.to_string())?;
    match sd.lookup(&word) {
        Some(v) => println!("{}", serde_json::to_string(&v).unwrap()),
        None => println!("null"),
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_suggest(args: &[String]) -> Result<ExitCode, String> {
    let mut ifo: Option<PathBuf> = None;
    let mut prefix = String::new();
    let mut limit = 10usize;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--ifo" => ifo = Some(PathBuf::from(take_value(args, &mut i)?)),
            "--limit" => {
                limit = take_value(args, &mut i)?
                    .parse()
                    .map_err(|_| "无效 limit")?
            }
            other => prefix = other.to_string(),
        }
        i += 1;
    }
    let ifo = ifo.ok_or("缺少 --ifo")?;
    let sd = lexicon_core::stardict::StarDict::open(&ifo).map_err(|e| e.to_string())?;
    let v = sd.suggest(&prefix, limit);
    println!("{}", serde_json::to_string(&v).unwrap());
    Ok(ExitCode::SUCCESS)
}

fn cmd_dump_resources(args: &[String]) -> Result<ExitCode, String> {
    let mut dict: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--dict" => dict = Some(PathBuf::from(take_value(args, &mut i)?)),
            "-o" | "--out" => out = Some(PathBuf::from(take_value(args, &mut i)?)),
            other => return Err(format!("未知参数: {other}")),
        }
        i += 1;
    }
    let dict = dict.ok_or("缺少 --dict")?;
    let out = out.ok_or("缺少 -o")?;
    let d = lexicon_core::ld2::open(&dict).map_err(|e| e.to_string())?;
    let res = d.extract_resources();
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for (name, data) in &res {
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            continue;
        }
        std::fs::write(out.join(name), data).map_err(|e| e.to_string())?;
    }
    println!("提取 {} 个资源到 {}", res.len(), out.display());
    Ok(ExitCode::SUCCESS)
}
