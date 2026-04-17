use crate::BridgeError;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub bridge: BridgeOptions,
    #[serde(default)]
    pub channels: Vec<ChannelConfig>,
}

impl BridgeConfig {
    pub fn validate(mut self) -> Result<Self, BridgeError> {
        if self.channels.is_empty() {
            return Err(BridgeError::Configuration(
                "at least one channel must be configured".to_string(),
            ));
        }

        let mut seen_topics = std::collections::BTreeSet::new();
        let mut seen_addresses = std::collections::BTreeSet::new();

        for channel in &mut self.channels {
            channel.normalize(&self.bridge)?;

            if !seen_topics.insert(channel.topic.clone()) {
                return Err(BridgeError::Configuration(format!(
                    "duplicate channel topic `{}`",
                    channel.topic
                )));
            }

            if !seen_addresses.insert(channel.tism_address.clone()) {
                return Err(BridgeError::Configuration(format!(
                    "duplicate TISM address `{}`",
                    channel.tism_address
                )));
            }
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_server_name")]
    pub name: String,
    #[serde(default = "default_message_backlog_size")]
    pub message_backlog_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            name: default_server_name(),
            message_backlog_size: default_message_backlog_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct BridgeOptions {
    #[serde(default = "default_publish_hz")]
    pub default_publish_hz: f64,
    #[serde(default = "default_open_retry_ms")]
    pub open_retry_ms: u64,
}

impl Default for BridgeOptions {
    fn default() -> Self {
        Self {
            default_publish_hz: default_publish_hz(),
            open_retry_ms: default_open_retry_ms(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChannelConfig {
    pub tism_address: String,
    pub topic: String,
    #[serde(default)]
    pub publish_hz: Option<f64>,
    #[serde(default)]
    pub on_change_only: bool,
    #[serde(default = "default_message_encoding")]
    pub message_encoding: String,
    #[serde(default)]
    pub schema_name: Option<String>,
    #[serde(default)]
    pub schema_encoding: Option<String>,
    #[serde(default)]
    pub schema_path: Option<PathBuf>,
    #[serde(default)]
    pub max_message_bytes: Option<usize>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl ChannelConfig {
    pub fn effective_publish_hz(&self, defaults: &BridgeOptions) -> f64 {
        self.publish_hz.unwrap_or(defaults.default_publish_hz)
    }

    fn normalize(&mut self, defaults: &BridgeOptions) -> Result<(), BridgeError> {
        if self.tism_address.trim().is_empty() {
            return Err(BridgeError::Configuration(
                "channel `tism_address` cannot be empty".to_string(),
            ));
        }

        if self.topic.trim().is_empty() {
            return Err(BridgeError::Configuration(
                "channel `topic` cannot be empty".to_string(),
            ));
        }

        let publish_hz = self.effective_publish_hz(defaults);
        if !publish_hz.is_finite() || publish_hz <= 0.0 {
            return Err(BridgeError::Configuration(format!(
                "channel `{}` has invalid publish_hz `{publish_hz}`",
                self.topic
            )));
        }

        if self.message_encoding == "flatbuffer" {
            if self.schema_name.is_none() {
                return Err(BridgeError::MissingSchemaName {
                    channel: self.topic.clone(),
                });
            }

            if self.schema_path.is_none() {
                return Err(BridgeError::MissingSchemaPath {
                    channel: self.topic.clone(),
                });
            }

            if self.schema_encoding.is_none() {
                self.schema_encoding = Some("flatbuffer".to_string());
            }
        }

        if self.schema_path.is_some() && self.schema_name.is_none() {
            return Err(BridgeError::MissingSchemaName {
                channel: self.topic.clone(),
            });
        }

        if self.schema_name.is_some() && self.schema_path.is_none() {
            return Err(BridgeError::MissingSchemaPath {
                channel: self.topic.clone(),
            });
        }

        if let Some(schema_path) = &self.schema_path {
            if !schema_path.is_file() {
                return Err(BridgeError::Configuration(format!(
                    "schema file for channel `{}` does not exist: {}",
                    self.topic,
                    schema_path.display()
                )));
            }
        }

        Ok(())
    }
}

pub fn load_config(path: impl AsRef<Path>) -> Result<BridgeConfig, BridgeError> {
    let path = path.as_ref();
    let config_text = fs::read_to_string(path)?;
    parse_config(&config_text, path.parent())
}

pub fn parse_config(
    contents: &str,
    base_dir: Option<&Path>,
) -> Result<BridgeConfig, BridgeError> {
    let mut config: BridgeConfig = toml::from_str(contents)?;

    if let Some(base_dir) = base_dir {
        for channel in &mut config.channels {
            if let Some(schema_path) = &channel.schema_path {
                if schema_path.is_relative() {
                    channel.schema_path = Some(base_dir.join(schema_path));
                }
            }
        }
    }

    config.validate()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8765
}

fn default_server_name() -> String {
    "tism-bridge".to_string()
}

fn default_message_backlog_size() -> usize {
    1024
}

fn default_publish_hz() -> f64 {
    30.0
}

fn default_open_retry_ms() -> u64 {
    250
}

fn default_message_encoding() -> String {
    "flatbuffer".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rejects_flatbuffer_channel_without_schema_path() {
        let config = r#"
            [[channels]]
            tism_address = "imu"
            topic = "/imu"
            schema_name = "foxglove.Imu"
        "#;

        let err = parse_config(config, None).expect_err("config should fail");
        assert!(matches!(err, BridgeError::MissingSchemaPath { .. }));
    }

    #[test]
    fn resolves_relative_schema_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift")
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("gs_visualizer_{unique}"));
        fs::create_dir_all(base_dir.join("schemas")).expect("create temp dir");
        fs::write(base_dir.join("schemas/Imu.bfbs"), b"bfbs").expect("create schema");

        let config = r#"
            [[channels]]
            tism_address = "imu"
            topic = "/imu"
            schema_name = "foxglove.Imu"
            schema_path = "schemas/Imu.bfbs"
        "#;

        let parsed = parse_config(config, Some(&base_dir)).expect("config should parse");
        let schema_path = parsed.channels[0]
            .schema_path
            .as_ref()
            .expect("resolved path");
        assert_eq!(schema_path, &base_dir.join("schemas/Imu.bfbs"));
    }
}
