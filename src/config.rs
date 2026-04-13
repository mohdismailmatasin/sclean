use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::error::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_protected_dirs")]
    pub protected_dirs: Vec<String>,

    #[serde(default = "default_targets")]
    pub targets: Vec<CleanTarget>,

    #[serde(default = "default_max_log_age_days")]
    pub max_log_age_days: u64,

    #[serde(default = "default_max_temp_age_days")]
    pub max_temp_age_days: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CleanTarget {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_target_type")]
    pub target_type: TargetType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Directory,
    EmptyDirs,
    Orphans,
    LockFile,
    DockerPrune,
    PacmanOldPkgs,
    PacmanDownloadCache,
    File,
}

fn default_target_type() -> TargetType {
    TargetType::Directory
}

fn default_protected_dirs() -> Vec<String> {
    vec![
        "Desktop".into(),
        "Downloads".into(),
        "Pictures".into(),
        "Videos".into(),
        "Documents".into(),
        "Music".into(),
        "Templates".into(),
    ]
}

fn default_max_log_age_days() -> u64 {
    7
}

fn default_max_temp_age_days() -> u64 {
    1
}

fn default_targets() -> Vec<CleanTarget> {
    vec![]
}

impl Default for Config {
    fn default() -> Self {
        Config {
            protected_dirs: default_protected_dirs(),
            targets: default_targets(),
            max_log_age_days: default_max_log_age_days(),
            max_temp_age_days: default_max_temp_age_days(),
        }
    }
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("sclean")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            debug!(path = %path.display(), "Loading config file");
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => {
                        debug!("Config loaded successfully");
                        return config;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse config file, using defaults");
                    }
                },
                Err(e) => {
                    warn!(error = %e, "Failed to read config file, using defaults");
                }
            }
        }
        debug!("Using default configuration");
        Self::default()
    }

    pub fn save(&self) -> Result<(), Error> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir).map_err(Error::ConfigDirCreate)?;
        let content = toml::to_string_pretty(self)?;
        fs::write(Self::config_path(), content).map_err(Error::ConfigWrite)?;
        debug!(path = %Self::config_path().display(), "Config saved");
        Ok(())
    }

    pub fn generate_default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/home"));
        let cache = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/home/.cache"));

        macro_rules! dir {
            ($name:expr, $path:expr) => {
                CleanTarget {
                    name: $name.into(),
                    path: $path.into(),
                    enabled: true,
                    target_type: TargetType::Directory,
                }
            };
        }

        macro_rules! dir_home {
            ($name:expr, $path:expr) => {
                dir!($name, home.join($path).to_string_lossy())
            };
        }

        Config {
            protected_dirs: default_protected_dirs(),
            targets: vec![
                dir!("User temp files", "/tmp"),
                dir!("System temp files", "/var/tmp"),
                dir!("User cache", cache.to_string_lossy()),
                dir_home!("Thumbnail cache", ".cache/thumbnails"),
                dir_home!("Firefox cache", ".cache/mozilla/firefox"),
                dir_home!("Chrome cache", ".config/google-chrome/Default/Cache"),
                dir_home!("Chromium cache", ".cache/chromium"),
                dir_home!("Python pip cache", ".cache/pip"),
                dir_home!("Node.js npm cache", ".npm"),
                dir_home!("Yarn cache", ".cache/yarn"),
                dir_home!("Bun cache", ".cache/bun"),
                dir_home!("pnpm cache", ".local/share/pnpm"),
                dir_home!("Poetry cache", ".cache/pypoetry"),
                dir_home!("uv cache", ".cache/uv"),
                dir_home!("Rust cargo cache", ".cargo/registry"),
                dir_home!("Flatpak app cache", ".cache/flatpak"),
                dir_home!("PHP composer cache", ".composer/cache"),
                dir_home!("Go build cache", ".cache/go-build"),
                dir!("APT package cache", "/var/cache/apt"),
                dir!("Pacman package cache", "/var/cache/pacman"),
                CleanTarget {
                    name: "Pacman old packages".into(),
                    path: "/var/cache/pacman/pkg".into(),
                    enabled: true,
                    target_type: TargetType::PacmanOldPkgs,
                },
                CleanTarget {
                    name: "Pacman download cache".into(),
                    path: "/var/cache/pacman/pkg".into(),
                    enabled: true,
                    target_type: TargetType::PacmanDownloadCache,
                },
                dir!("Snap cache", "/var/lib/snapd/cache"),
                dir!("Flatpak system cache", "/var/cache/flatpak"),
                dir_home!("Discord cache", ".config/Discord/Cache"),
                dir_home!("Slack cache", ".config/Slack/Cache"),
                dir_home!("VS Code cache", ".config/Code/Cache"),
                dir_home!("VS Code cached data", ".config/Code/CachedData"),
                dir_home!("JetBrains IDE caches", ".cache/JetBrains"),
                dir_home!("Spotify cache", ".cache/spotify"),
                dir_home!("Steam shader cache", ".local/share/Steam/shadercache"),
                dir_home!("Steam temp files", ".local/share/Steam/temp"),
                dir_home!("Mesa shader cache", ".cache/mesa_shader_cache"),
                dir_home!("NVIDIA GL cache", ".cache/nvidia/GLCache"),
                dir_home!("Font cache", ".cache/fontconfig"),
                dir_home!("Tracker indexer cache (v3)", ".cache/tracker3"),
                dir_home!("Tracker indexer cache (v2)", ".cache/tracker"),
                dir_home!("Baloo file indexer cache", ".local/share/baloo"),
                CleanTarget {
                    name: "Recent files".into(),
                    path: home
                        .join(".local/share/recently-used.xbel")
                        .to_string_lossy()
                        .to_string(),
                    enabled: true,
                    target_type: TargetType::File,
                },
                dir!("System logs", "/var/log"),
                dir!("System journal logs", "/var/log/journal"),
                dir!("Core dumps", "/var/lib/systemd/coredump"),
                dir_home!("User trash", ".local/share/Trash"),
                CleanTarget {
                    name: "Docker prune".into(),
                    path: "/var/lib/docker".into(),
                    enabled: false,
                    target_type: TargetType::DockerPrune,
                },
                CleanTarget {
                    name: "Empty directories in home".into(),
                    path: home.to_string_lossy().to_string(),
                    enabled: false,
                    target_type: TargetType::EmptyDirs,
                },
                CleanTarget {
                    name: "Orphan packages (Arch)".into(),
                    path: "/usr".into(),
                    enabled: true,
                    target_type: TargetType::Orphans,
                },
                CleanTarget {
                    name: "Pacman lock file".into(),
                    path: "/var/lib/pacman/db.lck".into(),
                    enabled: true,
                    target_type: TargetType::LockFile,
                },
            ],
            max_log_age_days: 7,
            max_temp_age_days: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.protected_dirs.len(), 7);
        assert!(config.targets.is_empty());
    }

    #[test]
    fn test_generate_default_has_targets() {
        let config = Config::generate_default();
        assert!(!config.targets.is_empty());
        assert!(config.targets.iter().any(|t| t.name == "User temp files"));
    }

    #[test]
    fn test_protected_dirs_default() {
        let config = Config::default();
        assert!(config.protected_dirs.contains(&"Desktop".to_string()));
        assert!(config.protected_dirs.contains(&"Downloads".to_string()));
    }

    #[test]
    fn test_config_path() {
        let path = Config::config_path();
        assert!(path.ends_with("sclean/config.toml"));
    }
}
