use crate::{BridgeError, ChannelConfig};
use foxglove::Schema;
use std::fs;

pub fn load_schema(channel: &ChannelConfig) -> Result<Option<Schema>, BridgeError> {
    let Some(path) = &channel.schema_path else {
        return Ok(None);
    };
    let Some(name) = &channel.schema_name else {
        return Err(BridgeError::MissingSchemaName {
            channel: channel.topic.clone(),
        });
    };

    let encoding = channel
        .schema_encoding
        .clone()
        .unwrap_or_else(|| channel.message_encoding.clone());
    let bytes = fs::read(path).map_err(|source| BridgeError::SchemaRead {
        channel: channel.topic.clone(),
        path: path.clone(),
        source,
    })?;

    Ok(Some(Schema::new(name.clone(), encoding, bytes)))
}
