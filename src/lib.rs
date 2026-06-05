#![doc = include_str!("../README.md")]

mod config;
mod error;
pub mod implementation;
mod publisher;
mod runtime;
mod schema;

pub use config::{
    BridgeConfig, BridgeOptions, ChannelConfig, ServerConfig, load_config, parse_config,
};
pub use error::BridgeError;

/// Runtime configuration and execution handle for the Foxglove bridge.
#[derive(Debug, Clone)]
pub struct Bridge {
    config: BridgeConfig,
}

impl Bridge {
    /// Builds a bridge from validated configuration.
    pub fn from_config(config: BridgeConfig) -> Result<Self, BridgeError> {
        Ok(Self {
            config: config.validate()?,
        })
    }

    /// Runs the bridge until Ctrl-C or a worker exits with an error.
    pub async fn run_until_shutdown(self) -> Result<(), BridgeError> {
        runtime::run_bridge(self.config).await
    }

    /// Runs the bridge on an internally owned Tokio runtime.
    pub fn run_blocking(self) -> Result<(), BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.run_until_shutdown())
    }
}
