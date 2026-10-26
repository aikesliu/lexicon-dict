//! 进程内词典引擎：加载 DictSet、查询、整理 UI 需要的数据。

use std::path::PathBuf;

use lexicon_core::common::{Config, DictInfo};
use lexicon_core::dictset::DictSet;
use lexicon_core::manifest::Manifest;
use lexicon_core::markup::{to_blocks, to_pango, Block};
use lexicon_core::query::{query_one, QueryOpts};

#[derive(Clone, Debug)]
pub enum BlockData {
    Text(String),
    Image { path: PathBuf },
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub word: String,
    pub dict_id: String,
    pub dict_name: String,
    pub pango: String,
    pub blocks: Vec<BlockData>,
    pub score: f64,
    pub match_type: String,
}

pub struct Engine {
    pub config: Config,
    ds: DictSet,
    pub total_entries: u64,
}

impl Engine {
    pub fn load(config: Config) -> Engine {
        let mut ds = DictSet::new(&config.cache_dir);
        ds.reload(&config);
        let total_entries = dicts_total(&config.cache_dir);
        Engine {
            config,
            ds,
            total_entries,
        }
    }

    pub fn reload(&mut self) {
        self.ds.reload(&self.config);
        self.total_entries = dicts_total(&self.config.cache_dir);
    }

    pub fn dict_count(&self) -> usize {
        self.ds.loaded_ids().len()
    }

    pub fn dict_rows(&self) -> Vec<(String, DictInfo, bool)> {
        let enabled = &self.config.dict_enabled;
        self.ds
            .available_with_ids()
            .into_iter()
            .map(|(id, info)| {
                let on = enabled.is_empty() || enabled.contains(&id);
                (id, info, on)
            })
            .collect()
    }

    pub fn query(&self, word: &str) -> Vec<Hit> {
        let opts = QueryOpts {
            case_insensitive: self.config.case_insensitive,
            prefix_fallback: self.config.prefix_fallback,
            prefix_match_limit: self.config.prefix_match_limit,
        };
        let mut out = Vec::new();
        for did in self.ds.loaded_ids() {
            for r in query_one(&self.ds, did, word, &opts) {
                let res_dir = self.config.cache_dir.join(&r.dict_id).join("res");
                let raw_blocks = to_blocks(&r.markup);
                let mut blocks: Vec<BlockData> = Vec::new();
                let mut has_image = false;
                for b in raw_blocks {
                    match b {
                        Block::Text { pango } => blocks.push(BlockData::Text(pango)),
                        Block::Image { name } => {
                            let path = res_dir.join(&name);
                            if path.is_file() {
                                has_image = true;
                                blocks.push(BlockData::Image { path });
                            }
                        }
                    }
                }
                if !has_image {
                    blocks.clear();
                }
                out.push(Hit {
                    word: r.word,
                    dict_id: r.dict_id,
                    dict_name: r.dict_display_name,
                    pango: to_pango(&r.markup),
                    blocks,
                    score: r.score,
                    match_type: r.match_type,
                });
            }
        }
        out.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }
}

fn dicts_total(cache: &std::path::Path) -> u64 {
    Manifest::load(cache)
        .infos()
        .iter()
        .filter(|(_, i)| i.status == "ok")
        .map(|(_, i)| i.entry_count)
        .sum()
}
