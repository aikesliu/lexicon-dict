//! StarDict 读写。

pub mod reader;
pub mod writer;

pub use reader::StarDict;
pub use writer::{WriteStats, write_stardict};
