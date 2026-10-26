//! 单进程 app 的配置：核心词典配置沿用 lexicon-core 的 config.toml，GUI 偏好另存 gui.toml。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lexicon_core::common::Config;
use lexicon_core::config::{load_config, save_config};
use lexicon_core::paths::Paths as CorePaths;
use serde::{Deserialize, Serialize};

pub const ACTIONS: [(&str, &str); 14] = [
    ("next", "下一条"),
    ("prev", "上一条"),
    ("page_down", "向下翻页"),
    ("page_up", "向上翻页"),
    ("next_tab", "下一个查询词"),
    ("prev_tab", "上一个查询词"),
    ("copy", "复制释义"),
    ("us", "美音"),
    ("uk", "英音"),
    ("first", "第一条"),
    ("last", "最后一条"),
    ("focus", "回到输入框"),
    ("help", "快捷键帮助"),
    ("close", "返回/取消"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Gui {
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub debounce_ms: u64,
    pub page_size: usize,
    /// dark | light
    pub theme: String,
    pub shortcuts: BTreeMap<String, String>,
}

impl Default for Gui {
    fn default() -> Self {
        let mut shortcuts = BTreeMap::new();
        for (k, v) in [
            ("next", "j"),
            ("prev", "k"),
            ("page_down", "f"),
            ("page_up", "b"),
            ("next_tab", "Tab"),
            ("prev_tab", "Shift+Tab"),
            ("copy", "y"),
            ("us", "v"),
            ("uk", "Shift+V"),
            ("first", "g"),
            ("last", "Shift+G"),
            ("focus", "/"),
            ("help", "?"),
            ("close", "Escape"),
        ] {
            shortcuts.insert(k.to_string(), v.to_string());
        }
        Gui {
            width: 880.0,
            height: 760.0,
            font_size: 16.0,
            debounce_ms: 300,
            page_size: 10,
            theme: "dark".into(),
            shortcuts,
        }
    }
}

impl Gui {
    pub fn shortcut(&self, key: &str) -> &str {
        self.shortcuts.get(key).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn set_shortcut(&mut self, key: &str, value: String) {
        self.shortcuts.insert(key.to_string(), value);
    }
}

#[derive(Clone)]
pub struct Paths {
    pub core_config: PathBuf,
    pub gui_config: PathBuf,
    pub cache_dir: PathBuf,
    pub portable: bool,
}

impl Paths {
    pub fn discover() -> Paths {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let portable = exe_dir.join("portable").exists() || exe_dir.join("portable.txt").exists();
        if portable {
            let base = exe_dir.join("data");
            return Paths {
                core_config: base.join("config.toml"),
                gui_config: base.join("gui.toml"),
                cache_dir: base.join("stardict"),
                portable: true,
            };
        }
        let p = CorePaths::discover();
        Paths {
            core_config: p.config_path.clone(),
            gui_config: p.config_dir.join("gui.toml"),
            cache_dir: p.stardict_dir.clone(),
            portable: false,
        }
    }

    pub fn load_core(&self) -> Config {
        let existed = self.core_config.exists();
        let mut cfg = load_config(&self.core_config);
        if !existed {
            cfg.cache_dir = self.cache_dir.clone();
        }
        cfg
    }

    pub fn save_core(&self, cfg: &Config) {
        let _ = save_config(cfg, &self.core_config);
    }

    pub fn load_gui(&self) -> Gui {
        std::fs::read_to_string(&self.gui_config)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save_gui(&self, gui: &Gui) {
        if let Some(parent) = self.gui_config.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(text) = toml::to_string_pretty(gui) {
            let _ = std::fs::write(&self.gui_config, text);
        }
    }
}

/// 词典显示名：优先取中文部分（与原扩展 dictName.js 一致）。
pub fn display_dict_name(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        let cp = *c as u32;
        if (0x4E00..=0x9FA5).contains(&cp) {
            let mut start = i;
            while start > 0 && chars[start - 1].is_whitespace() {
                start -= 1;
            }
            if start > 0 {
                return chars[start..].iter().collect::<String>().trim().to_string();
            }
            break;
        }
    }
    stem.to_string()
}
