//! config.toml 读写；字段与 Python 版逐一对应，并兼容旧的 `source_dir` 单值。

use std::path::{Path, PathBuf};

use crate::common::Config;
use crate::error::{Error, Result};

fn path_from(v: &toml::Value) -> Option<PathBuf> {
    v.as_str().map(|s| PathBuf::from(expand_home(s)))
}

pub fn expand_home(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        return crate::paths::home_dir()
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    if s == "~" {
        return crate::paths::home_dir().to_string_lossy().into_owned();
    }
    s.to_string()
}

pub fn load_config(path: &Path) -> Config {
    let mut cfg = Config::default();
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return cfg,
    };
    let value: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(_) => return cfg,
    };
    let table = match value.as_table() {
        Some(t) => t,
        None => return cfg,
    };
    if let Some(v) = table.get("cache_dir").and_then(path_from) {
        cfg.cache_dir = v;
    }
    // source_dirs 优先；旧 source_dir 兼容。
    let dirs: Option<Vec<PathBuf>> = table
        .get("source_dirs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(path_from).collect());
    if let Some(dirs) = dirs {
        // 显式写入的空数组也应被尊重：默认没有词典目录。
        cfg.source_dirs = dirs;
    } else if let Some(single) = table.get("source_dir").and_then(path_from) {
        cfg.source_dirs = vec![single];
    }
    if let Some(v) = table.get("auto_load").and_then(|v| v.as_bool()) {
        cfg.auto_load = v;
    }
    if let Some(v) = table.get("preload_all").and_then(|v| v.as_bool()) {
        cfg.preload_all = v;
    }
    if let Some(arr) = table.get("dict_order").and_then(|v| v.as_array()) {
        cfg.dict_order = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(arr) = table.get("dict_enabled").and_then(|v| v.as_array()) {
        cfg.dict_enabled = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    if let Some(v) = table.get("case_insensitive").and_then(|v| v.as_bool()) {
        cfg.case_insensitive = v;
    }
    if let Some(v) = table.get("prefix_fallback").and_then(|v| v.as_bool()) {
        cfg.prefix_fallback = v;
    }
    if let Some(v) = table.get("prefix_match_limit").and_then(|v| v.as_integer()) {
        if v > 0 {
            cfg.prefix_match_limit = v as usize;
        }
    }
    cfg
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 写出与 Python 版相同结构的 config.toml（原子替换）。
pub fn save_config(cfg: &Config, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let dirs: Vec<String> = cfg
        .source_dirs
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let order: Vec<String> = cfg.dict_order.clone();
    let enabled: Vec<String> = cfg.dict_enabled.iter().cloned().collect();
    let mut text = String::new();
    // 默认缓存目录由平台路径推导，不写入配置；只有用户自定义缓存时才落盘，
    // 避免默认配置里出现绝对路径。
    if cfg.cache_dir != Config::default().cache_dir {
        text.push_str(&format!(
            "cache_dir = {}\n",
            quote(&cfg.cache_dir.to_string_lossy())
        ));
    }
    text.push_str(&format!("source_dirs = [{}]\n", list(&dirs)));
    text.push_str(&format!("auto_load = {}\n", cfg.auto_load));
    text.push_str(&format!("preload_all = {}\n", cfg.preload_all));
    text.push_str(&format!("dict_order = [{}]\n", list(&order)));
    text.push_str(&format!("dict_enabled = [{}]\n", list(&enabled)));
    text.push_str(&format!("case_insensitive = {}\n", cfg.case_insensitive));
    text.push_str(&format!("prefix_fallback = {}\n", cfg.prefix_fallback));
    text.push_str(&format!(
        "prefix_match_limit = {}\n",
        cfg.prefix_match_limit
    ));
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl Config {
    /// 环境变量 `LEXICON_DICT_CACHE_DIR` 覆盖（测试/多实例用）。
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("LEXICON_DICT_CACHE_DIR") {
            if !v.is_empty() {
                self.cache_dir = PathBuf::from(expand_home(&v));
            }
        }
    }
}

#[allow(dead_code)]
fn unused(_: Error) {}
