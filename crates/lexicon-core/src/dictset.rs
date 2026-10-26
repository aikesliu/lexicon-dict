//! DictSet：按用户 order 聚合多个 StarDict handle，提供统一查询入口。
//!
//! 加载策略：reload(config) 读 manifest，按 config.dict_order 排序，
//! config.dict_enabled 为空集时视为全部启用；启用的词典 mmap 其 .idx/.dict（惰性 page-in）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::common::{Config, DictId, DictInfo, LookupResult};
use crate::error::Result;
use crate::manifest::Manifest;
use crate::query::{QueryOpts, query};
use crate::stardict::StarDict;

pub struct DictSet {
    cache_dir: PathBuf,
    handles: HashMap<DictId, StarDict>,
    info: HashMap<DictId, DictInfo>,
    order: Vec<DictId>,
}

impl DictSet {
    pub fn new(cache_dir: impl Into<PathBuf>) -> DictSet {
        DictSet {
            cache_dir: cache_dir.into(),
            handles: HashMap::new(),
            info: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// 读 manifest，返回所有已转换词典的 info（含 unsupported/error 状态）。
    pub fn list_available(&self) -> Vec<DictInfo> {
        Manifest::load(&self.cache_dir)
            .infos()
            .into_iter()
            .map(|(_slug, info)| info)
            .collect()
    }

    /// manifest 中的 (slug, info)，保持插入顺序。
    pub fn available_with_ids(&self) -> Vec<(String, DictInfo)> {
        Manifest::load(&self.cache_dir).infos()
    }

    /// 按 config.order/enabled 重新装载。enabled 为空集视为全部启用。
    pub fn reload(&mut self, config: &Config) {
        self.handles.clear();
        self.info.clear();
        self.order.clear();

        let available: Vec<(DictId, DictInfo)> = Manifest::load(&self.cache_dir)
            .infos()
            .into_iter()
            .filter(|(_, i)| i.status == "ok")
            .collect();
        let avail_ids: Vec<DictId> = available.iter().map(|(k, _)| k.clone()).collect();
        let avail_set: std::collections::HashSet<&DictId> = avail_ids.iter().collect();

        let mut ordered: Vec<DictId> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for did in &config.dict_order {
            if avail_set.contains(did) && seen.insert(did.clone()) {
                ordered.push(did.clone());
            }
        }
        for did in &avail_ids {
            if seen.insert(did.clone()) {
                ordered.push(did.clone());
            }
        }
        // available 的 info 取出，保持 manifest 顺序。
        let mut info_by_id: HashMap<DictId, DictInfo> = HashMap::new();
        for (id, info) in available {
            info_by_id.insert(id, info);
        }

        for did in ordered {
            let mut info = match info_by_id.remove(&did) {
                Some(i) => i,
                None => continue,
            };
            let enabled_set = &config.dict_enabled;
            info.enabled = enabled_set.is_empty() || enabled_set.contains(&did);
            self.info.insert(did.clone(), info);
            self.order.push(did.clone());
            if enabled_set.is_empty() || enabled_set.contains(&did) {
                self.load(&did);
            }
        }
    }

    fn load(&mut self, did: &str) {
        let ifo = self.cache_dir.join(did).join(format!("{did}.ifo"));
        match StarDict::open(&ifo) {
            Ok(sd) => {
                self.handles.insert(did.to_string(), sd);
            }
            Err(e) => {
                if let Some(info) = self.info.get_mut(did) {
                    info.status = "error".into();
                    info.error = e.to_string();
                }
            }
        }
    }

    pub fn loaded_ids(&self) -> Vec<&str> {
        self.order
            .iter()
            .filter(|d| self.handles.contains_key(*d))
            .map(|d| d.as_str())
            .collect()
    }

    pub fn handle(&self, did: &str) -> Option<&StarDict> {
        self.handles.get(did)
    }

    pub fn info(&self, did: &str) -> Option<&DictInfo> {
        self.info.get(did)
    }

    pub fn all_info(&self) -> Vec<&DictInfo> {
        self.order.iter().filter_map(|d| self.info.get(d)).collect()
    }

    pub fn lookup(&self, word: &str, opts: &QueryOpts) -> Vec<LookupResult> {
        query(self, word, opts)
    }

    /// 跨所有已加载词典聚合前缀建议，按 dict order 优先去重。
    pub fn suggest(&self, prefix: &str, limit: usize) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for did in self.loaded_ids() {
            if let Some(sd) = self.handles.get(did) {
                for w in sd.suggest(prefix, limit) {
                    if seen.insert(w.clone()) {
                        out.push(w);
                        if out.len() >= limit {
                            return out;
                        }
                    }
                }
            }
        }
        out
    }

    pub fn ensure_loaded(&mut self, config: &Config) -> Result<()> {
        if self.order.is_empty() {
            self.reload(config);
        }
        Ok(())
    }
}
