use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml_edit::TomlError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Semver error: {0}")]
    Semver(#[from] semver::Error),

    #[error("Walkdir error: {0}")]
    Walkdir(#[from] walkdir::Error),

    #[error("Ignore error: {0}")]
    Ignore(#[from] ignore::Error),

    #[error("Template parse error: {0}")]
    TemplateParse(String),

    #[error("Manifest error: {0}")]
    Manifest(String),
}

pub type Result<T> = std::result::Result<T, Error>;
