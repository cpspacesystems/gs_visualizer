use crate::{BridgeError, ChannelConfig, schema};
use foxglove::{Context, RawChannel};
use std::sync::Arc;

pub fn build_channel(
    context: &Arc<Context>,
    channel: &ChannelConfig,
) -> Result<Arc<RawChannel>, BridgeError> {
    let mut builder = context
        .channel_builder(channel.topic.clone())
        .message_encoding(channel.message_encoding.clone())
        .metadata(channel.metadata.clone());

    if let Some(schema) = schema::load_schema(channel)? {
        builder = builder.schema(schema);
    }

    builder.build_raw().map_err(BridgeError::from)
}
