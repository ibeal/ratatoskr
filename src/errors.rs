use std::fmt::{self, Display};
use std::io;
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, RatatoskrError>;

#[derive(Debug)]
pub enum RatatoskrError {
    Io(io::Error),
    ReadConfig(PathBuf, io::Error),
    ParseConfig(PathBuf, toml::de::Error),
    SerializeJson(serde_json::Error),
    InvalidRoot(String),
    AlreadyExists(PathBuf),
}

impl Display for RatatoskrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(source) => write!(f, "{source}"),
            Self::ReadConfig(path, source) => {
                write!(f, "failed to read config {}: {source}", path.display())
            }
            Self::ParseConfig(path, source) => {
                write!(f, "failed to parse config {}: {source}", path.display())
            }
            Self::SerializeJson(source) => write!(f, "failed to serialize JSON: {source}"),
            Self::InvalidRoot(message) => write!(f, "{message}"),
            Self::AlreadyExists(path) => {
                write!(f, "refusing to overwrite existing file {}", path.display())
            }
        }
    }
}

impl std::error::Error for RatatoskrError {}

impl From<io::Error> for RatatoskrError {
    fn from(source: io::Error) -> Self {
        Self::Io(source)
    }
}

impl From<serde_json::Error> for RatatoskrError {
    fn from(source: serde_json::Error) -> Self {
        Self::SerializeJson(source)
    }
}

pub fn ensure_absent(path: &Path) -> Result<()> {
    if path.exists() {
        return Err(RatatoskrError::AlreadyExists(path.to_path_buf()));
    }

    Ok(())
}
