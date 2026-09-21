use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid item format: {0}")]
    Format(String),
    #[error("encryption error: {0}")]
    Crypto(String),
    #[error("master key not loaded: {0}")]
    MasterKeyNotLoaded(String),
    #[error("unsupported encryption method: {0}")]
    UnsupportedMethod(u32),
    #[error("server error {status}: {message}")]
    Server { status: u16, code: Option<String>, message: String },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sync error: {0}")]
    Sync(String),
}

pub type Result<T> = std::result::Result<T, Error>;
