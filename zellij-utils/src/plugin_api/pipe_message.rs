pub use super::generated_api::api::pipe_message::{
    Arg as ProtobufArg, PipeMessage as ProtobufPipeMessage, PipeSource as ProtobufPipeSource,
};
use crate::data::{PipeMessage, PipeSource};

use std::convert::TryFrom;

pub fn pipe_message_from_protobuf_bytes(bytes: &[u8]) -> Option<PipeMessage> {
    use prost::Message;
    let protobuf_pipe_message = ProtobufPipeMessage::decode(bytes).ok()?;
    match PipeMessage::try_from(protobuf_pipe_message) {
        Ok(pipe_message) => Some(pipe_message),
        Err(e) => {
            eprintln!("Skipping a pipe message this plugin cannot read: {}", e);
            None
        },
    }
}

impl TryFrom<ProtobufPipeMessage> for PipeMessage {
    type Error = &'static str;
    fn try_from(protobuf_pipe_message: ProtobufPipeMessage) -> Result<Self, &'static str> {
        let source = match (
            ProtobufPipeSource::try_from(protobuf_pipe_message.source).ok(),
            protobuf_pipe_message.cli_source_id,
            protobuf_pipe_message.plugin_source_id,
        ) {
            (Some(ProtobufPipeSource::Cli), Some(cli_source_id), _) => {
                PipeSource::Cli(cli_source_id)
            },
            (Some(ProtobufPipeSource::Plugin), _, Some(plugin_source_id)) => {
                PipeSource::Plugin(plugin_source_id)
            },
            (Some(ProtobufPipeSource::Keybind), _, _) => PipeSource::Keybind,
            (Some(ProtobufPipeSource::PromptRequest), _, _) => match (
                protobuf_pipe_message.prompt_caller_plugin_id,
                protobuf_pipe_message.prompt_request_id,
            ) {
                (Some(caller_plugin_id), Some(request_id)) => PipeSource::PromptRequest {
                    caller_plugin_id,
                    request_id,
                },
                _ => return Err("Invalid PipeSource or payload"),
            },
            _ => return Err("Invalid PipeSource or payload"),
        };
        let name = protobuf_pipe_message.name;
        let payload = protobuf_pipe_message.payload;
        let args = protobuf_pipe_message
            .args
            .into_iter()
            .map(|arg| (arg.key, arg.value))
            .collect();
        let is_private = protobuf_pipe_message.is_private;
        Ok(PipeMessage {
            source,
            name,
            payload,
            args,
            is_private,
        })
    }
}

impl TryFrom<PipeMessage> for ProtobufPipeMessage {
    type Error = &'static str;
    fn try_from(pipe_message: PipeMessage) -> Result<Self, &'static str> {
        let mut prompt_caller_plugin_id = None;
        let mut prompt_request_id = None;
        let (source, cli_source_id, plugin_source_id) = match pipe_message.source {
            PipeSource::Cli(input_pipe_id) => {
                (ProtobufPipeSource::Cli as i32, Some(input_pipe_id), None)
            },
            PipeSource::Plugin(plugin_id) => {
                (ProtobufPipeSource::Plugin as i32, None, Some(plugin_id))
            },
            PipeSource::Keybind => (ProtobufPipeSource::Keybind as i32, None, None),
            PipeSource::PromptRequest {
                caller_plugin_id,
                request_id,
            } => {
                prompt_caller_plugin_id = Some(caller_plugin_id);
                prompt_request_id = Some(request_id);
                (ProtobufPipeSource::PromptRequest as i32, None, None)
            },
        };
        let name = pipe_message.name;
        let payload = pipe_message.payload;
        let args: Vec<_> = pipe_message
            .args
            .into_iter()
            .map(|(key, value)| ProtobufArg { key, value })
            .collect();
        let is_private = pipe_message.is_private;
        Ok(ProtobufPipeMessage {
            source,
            cli_source_id,
            plugin_source_id,
            name,
            payload,
            args,
            is_private,
            prompt_caller_plugin_id,
            prompt_request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_plugin_skips_a_pipe_message_it_cannot_read_instead_of_failing() {
        use prost::Message;
        let unknown_source = ProtobufPipeMessage {
            source: 999,
            name: "x".to_owned(),
            ..Default::default()
        };
        assert!(pipe_message_from_protobuf_bytes(&unknown_source.encode_to_vec()).is_none());
        let keybind = ProtobufPipeMessage {
            source: ProtobufPipeSource::Keybind as i32,
            name: "x".to_owned(),
            ..Default::default()
        };
        let read = pipe_message_from_protobuf_bytes(&keybind.encode_to_vec()).unwrap();
        assert_eq!(read.source, PipeSource::Keybind);
        assert_eq!(read.name, "x");
    }

    #[test]
    fn every_pipe_source_survives_protobuf() {
        for source in [
            PipeSource::Cli("id".to_owned()),
            PipeSource::Plugin(3),
            PipeSource::Keybind,
            PipeSource::PromptRequest {
                caller_plugin_id: 7,
                request_id: 42,
            },
        ] {
            let mut args = BTreeMap::new();
            args.insert("k".to_owned(), "v".to_owned());
            let message =
                PipeMessage::new(source, "confirm", &Some("p".to_owned()), &Some(args), true);
            let protobuf = ProtobufPipeMessage::try_from(message.clone()).unwrap();
            assert_eq!(PipeMessage::try_from(protobuf), Ok(message));
        }
    }
}
