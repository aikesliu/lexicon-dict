//! 统一错误类型。
//!
//! 转换器需要逐文件容错，因此错误里保留尽可能多的上下文，调用方决定
//! 是否降级为 manifest 里的 `status=error`。

use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// ld2 文件结构不合法。
    Ld2(String),
    /// 不支持的 ld2 变体（?LDF），转换时跳过并记录。
    UnsupportedVariant {
        magic: [u8; 4],
        path: PathBuf,
        hint: String,
    },
    /// StarDict 读写错误。
    StarDict(String),
    /// 配置/清单格式错误。
    Data(String),
    /// 其它。
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::Ld2(s) => write!(f, "ld2: {s}"),
            Error::UnsupportedVariant { magic, path, hint } => write!(
                f,
                "unsupported ld2 variant {:?} at {} ({})",
                String::from_utf8_lossy(magic),
                path.display(),
                hint
            ),
            Error::StarDict(s) => write!(f, "stardict: {s}"),
            Error::Data(s) => write!(f, "data: {s}"),
            Error::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Data(format!("json: {e}"))
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Error::Data(format!("toml: {e}"))
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Error::Data(format!("toml: {e}"))
    }
}
