use thiserror::Error;

/// Error type shared by all FlowDE crates.
#[derive(Debug, Error)]
pub enum FlowError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("toml parse: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("toml write: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("config: {0}")]
    Config(String),
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("timeout")]
    Timeout,
    #[error("remote error {code}: {message}")]
    Remote { code: String, message: String },
}

pub type Result<T> = std::result::Result<T, FlowError>;
