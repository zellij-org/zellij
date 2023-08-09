pub use super::generated_api::api::message::Message as ProtobufMessage;
use crate::data::PluginMessage;

use std::convert::TryFrom;

impl TryFrom<ProtobufMessage> for PluginMessage {
    type Error = &'static str;
    fn try_from(protobuf_message: ProtobufMessage) -> Result<Self, &'static str> {
        let name = protobuf_message.name;
        let payload = protobuf_message.payload;
        let worker_name = protobuf_message.worker_name;
        Ok(PluginMessage {
            name,
            payload,
            worker_name,
        })
    }
}

impl TryFrom<PluginMessage> for ProtobufMessage {
    type Error = &'static str;
    fn try_from(plugin_message: PluginMessage) -> Result<Self, &'static str> {
        Ok(ProtobufMessage {
            name: plugin_message.name,
            payload: plugin_message.payload,
            worker_name: plugin_message.worker_name,
        })
    }
}
