mod config;
mod error;
mod publisher;
mod runtime;
mod schema;
mod source_tism;

pub use config::{
    BridgeConfig, BridgeOptions, ChannelConfig, ServerConfig, load_config, parse_config,
};
pub use error::BridgeError;

#[derive(Debug, Clone)]
pub struct Bridge {
    config: BridgeConfig,
}

impl Bridge {
    pub fn from_config(config: BridgeConfig) -> Result<Self, BridgeError> {
        Ok(Self {
            config: config.validate()?,
        })
    }

    pub async fn run_until_shutdown(self) -> Result<(), BridgeError> {
        runtime::run_bridge(self.config).await
    }

    pub fn run_blocking(self) -> Result<(), BridgeError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.run_until_shutdown())
    }
}
