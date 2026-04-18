use std::{io, path::PathBuf};

use foxglove::FoxgloveError;
use thiserror::Error;

/// Errors returned while loading configuration or running the bridge.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("argument error: {0}")]
    Argument(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("failed to parse TOML config: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Foxglove error: {0}")]
    Foxglove(#[from] FoxgloveError),
    #[error("task join error: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("schema path is missing for channel `{channel}`")]
    MissingSchemaPath { channel: String },
    #[error("schema name is missing for channel `{channel}`")]
    MissingSchemaName { channel: String },
    #[error("failed to read schema for channel `{channel}` from `{path}`: {source}")]
    SchemaRead {
        channel: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
