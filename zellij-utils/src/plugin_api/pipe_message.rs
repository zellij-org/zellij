pub use super::generated_api::api::pipe_message::{PipeMessage as ProtobufPipeMessage, PipeSource as ProtobufPipeSource, Arg as ProtobufArg};
use crate::data::{PipeMessage, PipeSource};

use std::convert::TryFrom;

impl TryFrom<ProtobufPipeMessage> for PipeMessage {
    type Error = &'static str;
    fn try_from(protobuf_pipe_message: ProtobufPipeMessage) -> Result<Self, &'static str> {
        let source = match (ProtobufPipeSource::from_i32(protobuf_pipe_message.source), protobuf_pipe_message.cli_source_id, protobuf_pipe_message.plugin_source_id) {
            (Some(ProtobufPipeSource::Cli), Some(cli_source_id), _) => PipeSource::Cli(cli_source_id),
            (Some(ProtobufPipeSource::Plugin), _,  Some(plugin_source_id)) => PipeSource::Plugin(plugin_source_id),
            _ => return Err("Invalid PipeSource or payload")
        };
        let name = protobuf_pipe_message.name;
        let payload = protobuf_pipe_message.payload;
        let args = protobuf_pipe_message.args.into_iter().map(|arg| (arg.key, arg.value)).collect();
        Ok(PipeMessage {
            source,
            name,
            payload,
            args,
        })
    }
}

impl TryFrom<PipeMessage> for ProtobufPipeMessage {
    type Error = &'static str;
    fn try_from(pipe_message: PipeMessage) -> Result<Self, &'static str> {
        let (source, cli_source_id, plugin_source_id) = match pipe_message.source {
            PipeSource::Cli(input_pipe_id) => (ProtobufPipeSource::Cli as i32, Some(input_pipe_id), None),
            PipeSource::Plugin(plugin_id) => (ProtobufPipeSource::Plugin as i32, None, Some(plugin_id)),
        };
        let name = pipe_message.name;
        let payload = pipe_message.payload;
        let args: Vec<_> = pipe_message.args
            .into_iter()
            .map(|(key, value)| ProtobufArg { key, value })
            .collect();
        Ok(ProtobufPipeMessage {
            source,
            cli_source_id,
            plugin_source_id,
            name,
            payload,
            args,
        })
    }
}
