//! 共享类型（对应 Python `common.py`）。

use std::path::PathBuf;

pub type DictId = String;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DictInfo {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub source_mtime: f64,
    #[serde(default)]
    pub source_size: u64,
    #[serde(default)]
    pub source_sha256: String,
    #[serde(default)]
    pub entry_count: u64,
    #[serde(default)]
    pub word_encoding: String,
    #[serde(default)]
    pub def_encoding: String,
    #[serde(default = "default_idxoffsetbits")]
    pub idxoffsetbits: u32,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub res_count: u64,
    #[serde(default)]
    pub res_extracted: bool,
    /// 运行时由配置填充，不持久化到 manifest。
    #[serde(skip)]
    pub enabled: bool,
}

fn default_idxoffsetbits() -> u32 {
    32
}
fn default_status() -> String {
    "ok".to_string()
}

#[derive(Debug, Clone)]
pub struct LookupResult {
    pub word: String,
    pub dict_id: DictId,
    pub dict_display_name: String,
    /// 释义（ld2 原始 XML/HTML，渲染端清洗为 Pango）。
    pub markup: String,
    pub match_type: String,
    pub score: f64,
}

/// 运行时配置（对应 config.toml）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub cache_dir: PathBuf,
    pub source_dirs: Vec<PathBuf>,
    pub auto_load: bool,
    pub preload_all: bool,
    pub dict_order: Vec<DictId>,
    pub dict_enabled: std::collections::BTreeSet<DictId>,
    pub case_insensitive: bool,
    pub prefix_fallback: bool,
    pub prefix_match_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        let paths = crate::paths::Paths::discover();
        Config {
            cache_dir: paths.stardict_dir,
            // 默认不预设任何词典目录；由用户在设置里手动添加。
            source_dirs: Vec::new(),
            auto_load: true,
            preload_all: false,
            dict_order: Vec::new(),
            dict_enabled: std::collections::BTreeSet::new(),
            case_insensitive: true,
            prefix_fallback: true,
            prefix_match_limit: 8,
        }
    }
}
