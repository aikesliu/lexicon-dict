//! manifest.json：转换指纹与词典元信息。
//!
//! 结构：`{"version": 1, "dicts": {"<slug>": {...}}}`。`dicts` 保持插入顺序
//!（serde_json 的 preserve_order），与 Python 的 dict 行为一致。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::common::DictInfo;
use crate::error::Result;

pub const MANIFEST_NAME: &str = "manifest.json";

#[derive(Debug, Clone)]
pub struct Manifest {
    pub version: u32,
    pub dicts: Map<String, Value>,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            version: 1,
            dicts: Map::new(),
        }
    }
}

impl Manifest {
    pub fn path(out_dir: &Path) -> PathBuf {
        out_dir.join(MANIFEST_NAME)
    }

    pub fn load(out_dir: &Path) -> Manifest {
        let p = Self::path(out_dir);
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(_) => return Manifest::default(),
        };
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Manifest::default(),
        };
        let obj = match value.as_object() {
            Some(o) => o,
            None => return Manifest::default(),
        };
        let version = obj.get("version").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
        let dicts = obj
            .get("dicts")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        Manifest { version, dicts }
    }

    /// 原子写 manifest（写临时文件后 rename）。
    pub fn save(&self, out_dir: &Path) -> Result<()> {
        let p = Self::path(out_dir);
        let mut root = Map::new();
        root.insert("version".into(), Value::from(self.version));
        root.insert("dicts".into(), Value::Object(self.dicts.clone()));
        let text = serde_json::to_string_pretty(&Value::Object(root))?;
        let tmp = p.with_file_name(format!("{MANIFEST_NAME}.tmp"));
        std::fs::write(&tmp, text + "\n")?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// 按插入顺序返回 (slug, DictInfo)。
    pub fn infos(&self) -> Vec<(String, DictInfo)> {
        self.dicts
            .iter()
            .map(|(k, v)| {
                let info: DictInfo = serde_json::from_value(v.clone()).unwrap_or_default();
                (k.clone(), info)
            })
            .collect()
    }
}

pub fn info_to_value(info: &DictInfo) -> Value {
    serde_json::to_value(info).unwrap_or(Value::Null)
}
