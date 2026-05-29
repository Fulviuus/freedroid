use serde::{Serialize, Serializer};

/// Unified error type for all backend operations. Serializes to a plain string
/// so the frontend receives a readable message from `invoke()` rejections.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("adb error: {0}")]
    Adb(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("shell error: {0}")]
    Shell(#[from] tauri_plugin_shell::Error),

    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
