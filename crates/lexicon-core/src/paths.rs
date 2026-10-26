//! 路径约定：Linux 遵循 XDG，Windows 使用 LOCALAPPDATA/APPDATA。

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Paths {
    pub cache_dir: PathBuf,
    pub stardict_dir: PathBuf,
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Paths {
    pub fn discover() -> Paths {
        #[cfg(windows)]
        {
            let local =
                env_path("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData/Local"));
            let roaming = env_path("APPDATA").unwrap_or_else(|| home_dir().join("AppData/Roaming"));
            let cache_dir = local.join("lexicon-dict");
            let config_dir = roaming.join("lexicon-dict");
            Paths {
                stardict_dir: cache_dir.join("stardict"),
                config_path: config_dir.join("config.toml"),
                cache_dir,
                config_dir,
            }
        }
        #[cfg(not(windows))]
        {
            let home = home_dir();
            let cache_dir = env_path("XDG_CACHE_HOME")
                .unwrap_or_else(|| home.join(".cache"))
                .join("lexicon-dict");
            let config_dir = env_path("XDG_CONFIG_HOME")
                .unwrap_or_else(|| home.join(".config"))
                .join("lexicon-dict");
            Paths {
                stardict_dir: cache_dir.join("stardict"),
                config_path: config_dir.join("config.toml"),
                cache_dir,
                config_dir,
            }
        }
    }
}

pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(p) = env_path("USERPROFILE") {
            return p;
        }
    }
    env_path("HOME").unwrap_or_else(|| PathBuf::from("."))
}

/// 便携模式：exe 同目录存在 `portable.txt`/`portable` 或传 `--portable` 时，
/// 数据目录放在 exe 同级的 `data/`，不再写注册表/用户目录。
pub fn portable_paths(exe_dir: &std::path::Path) -> Paths {
    let cache_dir = exe_dir.join("data");
    Paths {
        stardict_dir: cache_dir.join("stardict"),
        config_path: cache_dir.join("config.toml"),
        config_dir: cache_dir.clone(),
        cache_dir,
    }
}
