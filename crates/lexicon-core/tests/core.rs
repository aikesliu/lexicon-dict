//! 集成测试：markup、StarDict 读写、查询聚合、配置。
//!
//! 真实 ld2 对照测试通过 `LEXICON_REFERENCE_DIR` 指定词典目录；未设置则跳过。

use std::path::PathBuf;

use lexicon_core::common::Config;
use lexicon_core::config::{load_config, save_config};
use lexicon_core::dictset::DictSet;
use lexicon_core::markup::{Block, to_blocks, to_pango};
use lexicon_core::query::{QueryOpts, query};
use lexicon_core::stardict::{StarDict, write_stardict};

fn tmpdir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "lexicon-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn write_manifest(cache: &PathBuf, dicts: &[(&str, Vec<(&str, &str)>)]) {
    for (slug, entries) in dicts {
        let items: Vec<(String, String)> = entries
            .iter()
            .map(|(w, d)| (w.to_string(), d.to_string()))
            .collect();
        write_stardict(cache, slug, slug, items, None).unwrap();
    }
    let mut root = serde_json::Map::new();
    root.insert("version".into(), serde_json::Value::from(1));
    let mut m = serde_json::Map::new();
    for (slug, entries) in dicts {
        let mut info = serde_json::Map::new();
        info.insert("display_name".into(), serde_json::Value::from(*slug));
        info.insert("status".into(), serde_json::Value::from("ok"));
        info.insert("entry_count".into(), serde_json::Value::from(entries.len()));
        m.insert((*slug).to_string(), serde_json::Value::Object(info));
    }
    root.insert("dicts".into(), serde_json::Value::Object(m));
    std::fs::write(
        cache.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap(),
    )
    .unwrap();
}

#[test]
fn stardict_roundtrip_32bit() {
    let dir = tmpdir("sd32");
    let entries: Vec<(String, String)> = vec![
        ("apple".into(), "苹果".into()),
        ("banana".into(), "香蕉".into()),
        ("zebra".into(), "斑马".into()),
    ];
    let stat = write_stardict(&dir, "d1", "D1", entries, None).unwrap();
    assert_eq!(stat.wordcount, 3);
    let sd = StarDict::open(dir.join("d1").join("d1.ifo")).unwrap();
    assert_eq!(sd.lookup("banana").as_deref(), Some("香蕉"));
    assert_eq!(sd.lookup("missing"), None);
    assert_eq!(sd.suggest("ba", 10), vec!["banana".to_string()]);
    assert_eq!(sd.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stardict_roundtrip_64bit() {
    let dir = tmpdir("sd64");
    let entries: Vec<(String, String)> = (0..8)
        .map(|i| (format!("w{i:02}"), format!("def{i}")))
        .collect();
    let stat = write_stardict(&dir, "big", "Big", entries, Some(64)).unwrap();
    assert_eq!(stat.idxoffsetbits, 64);
    let sd = StarDict::open(dir.join("big").join("big.ifo")).unwrap();
    assert_eq!(sd.lookup("w07").as_deref(), Some("def7"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn query_exact_casefold_prefix_and_order() {
    let dir = tmpdir("query");
    write_manifest(
        &dir,
        &[
            (
                "d1",
                vec![("Hello", "greeting"), ("apple", "fruit"), ("apply", "verb")],
            ),
            ("d2", vec![("hello", "greeting2")]),
        ],
    );
    let mut cfg = Config::default();
    cfg.cache_dir = dir.clone();
    cfg.dict_order = vec!["d2".into(), "d1".into()];
    let mut ds = DictSet::new(&dir);
    ds.reload(&cfg);
    let opts = QueryOpts::default();
    let r = query(&ds, "hello", &opts);
    assert_eq!(r.len(), 2);
    // 精确命中（d2）优先于 casefold（d1）
    assert_eq!(r[0].dict_id, "d2");
    assert_eq!(r[0].match_type, "exact");
    assert_eq!(r[1].match_type, "casefold");
    // 前缀回退
    let r = query(&ds, "appl", &opts);
    assert_eq!(r.len(), 2);
    assert_eq!(r[0].word, "apple");
    assert_eq!(r[0].match_type, "prefix");
    assert!(r[0].score >= 0.4 && r[0].score <= 0.8);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn query_respects_enabled_and_prefix_switch() {
    let dir = tmpdir("enabled");
    write_manifest(
        &dir,
        &[
            ("d1", vec![("word", "def1")]),
            ("d2", vec![("word", "def2")]),
        ],
    );
    let mut cfg = Config::default();
    cfg.cache_dir = dir.clone();
    cfg.dict_enabled = ["d1".to_string()].into_iter().collect();
    cfg.prefix_fallback = false;
    let mut ds = DictSet::new(&dir);
    ds.reload(&cfg);
    assert_eq!(ds.loaded_ids(), vec!["d1"]);
    let opts = QueryOpts {
        prefix_fallback: false,
        ..Default::default()
    };
    assert_eq!(query(&ds, "word", &opts).len(), 1);
    assert!(query(&ds, "wor", &opts).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_default_has_no_source_dirs() {
    assert!(Config::default().source_dirs.is_empty());

    // 显式写入的空数组不能被“默认目录”重新填上。
    let dir = tmpdir("cfg-empty");
    let path = dir.join("config.toml");
    std::fs::write(&path, "source_dirs = []\n").unwrap();
    assert!(load_config(&path).source_dirs.is_empty());

    // 默认配置落盘时不应写死缓存/词典路径。
    save_config(&Config::default(), &path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("cache_dir ="));
    assert!(text.contains("source_dirs = []"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn config_roundtrip() {
    let dir = tmpdir("cfg");
    let path = dir.join("config.toml");
    let mut cfg = Config::default();
    cfg.cache_dir = dir.join("cache");
    cfg.source_dirs = vec![dir.join("dicts")];
    cfg.dict_order = vec!["b".into(), "a".into()];
    cfg.dict_enabled = ["a".into()].into_iter().collect();
    cfg.case_insensitive = false;
    cfg.prefix_match_limit = 12;
    save_config(&cfg, &path).unwrap();
    let got = load_config(&path);
    assert_eq!(got.cache_dir, cfg.cache_dir);
    assert_eq!(got.source_dirs, cfg.source_dirs);
    assert_eq!(got.dict_order, cfg.dict_order);
    assert!(got.dict_enabled.contains("a"));
    assert!(!got.case_insensitive);
    assert_eq!(got.prefix_match_limit, 12);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn slugify_matches_python_rules() {
    assert_eq!(
        lexicon_core::slugify("E-C Computer Dictionary"),
        "e_c_computer_dictionary"
    );
    assert_eq!(lexicon_core::slugify("  Hello, World!! "), "hello_world");
    assert!(lexicon_core::slugify("纯中文").starts_with('d'));
}

#[test]
fn markup_basic() {
    assert_eq!(to_pango("hello world"), "hello world");
    assert_eq!(to_pango(""), "");
    assert_eq!(to_pango("   "), "");
    assert_eq!(to_pango("line1<n />line2"), "line1\nline2");
    assert_eq!(
        to_pango("<C><F><H /><I><N>n.计算机</N></I></F></C>"),
        "<b>n.</b>计算机"
    );
    let r = to_pango(r##"<x K="#FF9900">①</x>text"##);
    assert!(r.contains("①") && r.contains("text"));
    let r = to_pango("a < b & c > d");
    assert!(r.contains("&lt;") && r.contains("&amp;") && r.contains("&gt;"));
    let r = to_pango("before<Ë M=\"dict://res/x.png\" />after");
    assert!(r.contains("before") && r.contains("after"));
    assert!(!r.contains("dict://"));
}

#[test]
fn markup_containers_and_blocks() {
    let r = to_pango("<C>before<Ë>content</Ë>after</C>");
    assert!(r.contains("before") && r.contains("content") && r.contains("after"));
    let blocks = to_blocks("<C>before<Ë>content</Ë>after</C>");
    let text: String = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text { pango } => Some(pango.clone()),
            _ => None,
        })
        .collect();
    assert!(text.contains("before") && text.contains("content") && text.contains("after"));

    let blocks = to_blocks("<C><Ë M=\"dict://res/x.png\" />tail</C>");
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b, Block::Image { name } if name == "x.png"))
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b, Block::Text { pango } if pango.contains("tail")))
    );

    let blocks = to_blocks(
        "<Ô P=\"border:1px solid #EAF0F8\"><Ë M=\"dict://res/i-7438-0.png\" /></Ô><R>text</R>",
    );
    assert!(blocks.len() >= 2);
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b, Block::Image { name } if name == "i-7438-0.png"))
    );
}

#[test]
fn markup_shuowen_messy_tags() {
    let src = "<C><F><H><M>quan3</M></H><I><N><R><Ã>卷十一 く部</Ã></R><R><U>反切：姑泫切</U></R><Ô P=\"border:1px solid #EAF0F8\"><Ë M=\"dict://res/i-7438-0.png\" /></Ô><R>水小流也。</R></N></I></F></C>";
    let r = to_pango(src);
    assert!(r.contains("卷十一") && r.contains("反切：姑泫切") && r.contains("水小流也。"));
    assert!(!r.contains("dict://"));
    assert!(!r.contains("border"));
}

#[test]
fn ld2_real_dict_if_available() {
    let Some(base) = std::env::var_os("LEXICON_REFERENCE_DIR").map(PathBuf::from) else {
        eprintln!("跳过：未设置 LEXICON_REFERENCE_DIR");
        return;
    };
    let p = base.join("E-C Computer Dictionary.ld2");
    if !p.exists() {
        eprintln!("跳过：无真实词典 {p:?}");
        return;
    }
    let d = lexicon_core::ld2::open(&p).unwrap();
    assert!(d.entry_count > 100_000);
    assert_eq!(d.word_encoding.name(), "utf-8");
    assert_eq!(d.def_encoding.name(), "utf-16-le");
    let e0 = d.entry(0).unwrap();
    assert!(!e0.word.is_empty() && !e0.definition.is_empty());
    assert!(d.entry(d.entry_count).is_err());
}
