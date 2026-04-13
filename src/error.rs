use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum Error {
    #[error("failed to read config file: {0}")]
    ConfigRead(#[from] std::io::Error),

    #[error("failed to parse config file: {0}")]
    ConfigParse(#[from] toml::de::Error),

    #[error("failed to create config directory: {0}")]
    ConfigDirCreate(std::io::Error),

    #[error("failed to serialize config: {0}")]
    ConfigSerialize(#[from] toml::ser::Error),

    #[error("failed to write config file: {0}")]
    ConfigWrite(std::io::Error),

    #[error("path does not exist: {0}")]
    PathNotFound(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("failed to clean directory {path}: {source}")]
    CleanDirectory {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to remove empty directory {path}: {source}")]
    RemoveEmptyDir {
        path: String,
        source: std::io::Error,
    },

    #[error("failed to execute pacman: {0}")]
    PacmanExec(std::io::Error),

    #[error("pacman returned error: {0}")]
    Pacman(String),

    #[error("failed to remove lock file {path}: {source}")]
    RemoveLockFile {
        path: String,
        source: std::io::Error,
    },
}
