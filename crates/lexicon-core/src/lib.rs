//! lexicon-core：`.ld2` → StarDict → 多词典查询的纯 Rust 实现。
//!
//! 无 GUI/DBus 依赖，Linux 与 Windows 共用；桌面 app 与 CLI 都建立在此之上。

pub mod common;
pub mod config;
pub mod convert;
pub mod dictset;
pub mod error;
pub mod ld2;
pub mod manifest;
pub mod markup;
pub mod paths;
pub mod query;
pub mod stardict;

pub use common::{Config, DictId, DictInfo, LookupResult};
pub use convert::{ConvertOptions, ConvertStats, convert_directory, slugify};
pub use dictset::DictSet;
pub use error::{Error, Result};
pub use markup::{Block, to_blocks, to_pango};
pub use query::QueryOpts;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
