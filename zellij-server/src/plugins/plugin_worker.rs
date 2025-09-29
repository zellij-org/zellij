use crate::plugins::plugin_map::PluginEnv;
use crate::plugins::zellij_exports::wasi_write_object;
use wasmi::{Instance, Store};

use async_channel::{unbounded, Receiver, Sender};
use async_std::task;
use prost::Message;
use zellij_utils::errors::prelude::*;
use zellij_utils::plugin_api::message::ProtobufMessage;

pub struct RunningWorker {
    pub instance: Instance,
    pub name: String,
    pub store: Store<PluginEnv>,
}

impl RunningWorker {
    pub fn new(store: Store<PluginEnv>, instance: Instance, name: &str) -> Self {
        RunningWorker {
            store,
            instance,
            name: name.into(),
        }
    }
    pub fn send_message(&mut self, message: String, payload: String) -> Result<()> {
        let err_context = || format!("Failed to send message to worker");
        let protobuf_message = ProtobufMessage {
            name: message,
            payload,
            ..Default::default()
        };
        let protobuf_bytes = protobuf_message.encode_to_vec();
        let work_function = self
            .instance
            .get_typed_func::<(), ()>(&mut self.store, &self.name)
            .with_context(err_context)?;
        wasi_write_object(self.store.data(), &protobuf_bytes).with_context(err_context)?;
        work_function
            .call(&mut self.store, ())
            .with_context(err_context)?;
        Ok(())
    }
}

pub enum MessageToWorker {
    Message(String, String), // message, payload
    Exit,
}

pub fn plugin_worker(mut worker: RunningWorker) -> Sender<MessageToWorker> {
    let (sender, receiver): (Sender<MessageToWorker>, Receiver<MessageToWorker>) = unbounded();
    task::spawn({
        async move {
            loop {
                match receiver.recv().await {
                    Ok(MessageToWorker::Message(message, payload)) => {
                        if let Err(e) = worker.send_message(message, payload) {
                            log::error!("Failed to send message to worker: {:?}", e);
                        }
                    },
                    Ok(MessageToWorker::Exit) => {
                        break;
                    },
                    Err(e) => {
                        log::error!("Failed to receive worker message on channel: {:?}", e);
                        break;
                    },
                }
            }
        }
    });
    sender
}
