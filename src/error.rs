use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unable to open input file {path}: {inner}")]
    ReadInput { path: PathBuf, inner: io::Error },

    #[error("unable to create output file {path}: {inner}")]
    WriteOutput { path: PathBuf, inner: io::Error },

    #[error(transparent)]
    Clap(#[from] clap::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("unable to parse input JSON")]
    Json(#[from] json::Error),

    #[error("template engine error")]
    TemplateEngine(#[from] tinytemplate::error::Error),

    #[error("ethereum ABI error")]
    EthereumABI(#[from] ethabi::Error),

    #[error("metadata error: {0}")]
    Metadata(String),
}
