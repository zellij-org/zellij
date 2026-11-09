use crate::plugins::plugin_map::PluginEnv;
use crate::plugins::zellij_exports::wasi_write_object;
use wasmi::{Instance, Store};

use prost::Message;
use tokio::sync::mpsc::{channel, error::TrySendError, Receiver, Sender};
use zellij_utils::errors::prelude::*;
use zellij_utils::plugin_api::message::ProtobufMessage;

/// Worker callbacks can queue at most this many messages before back-pressure rejects new work.
/// A plugin that floods its worker faster than the worker runs must not grow host memory without
/// bound.
pub(crate) const WORKER_CHANNEL_CAPACITY: usize = 64;

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
        crate::plugins::shared::refuel_for_callback(&mut self.store);
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

/// Queue a message for a worker, rejecting it when the bounded queue is full.
pub(crate) fn try_send_to_worker(
    sender: &Sender<MessageToWorker>,
    message: MessageToWorker,
) -> bool {
    match sender.try_send(message) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            log::warn!("Plugin worker message queue is full; rejecting message");
            false
        },
        Err(TrySendError::Closed(_)) => false,
    }
}

fn worker_callback_is_current(
    load_alive: &std::sync::atomic::AtomicBool,
    quarantined: &std::sync::atomic::AtomicBool,
) -> bool {
    load_alive.load(std::sync::atomic::Ordering::Acquire)
        && !quarantined.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn plugin_worker(mut worker: RunningWorker) -> Sender<MessageToWorker> {
    let (sender, mut receiver): (Sender<MessageToWorker>, Receiver<MessageToWorker>) =
        channel(WORKER_CHANNEL_CAPACITY);
    let load_alive = worker.store.data().load_alive.clone();
    let quarantined = worker.store.data().quarantined.clone();
    crate::global_async_runtime::get_tokio_runtime().spawn({
        async move {
            loop {
                if !worker_callback_is_current(&load_alive, &quarantined) {
                    break;
                }
                match receiver.recv().await {
                    Some(MessageToWorker::Message(message, payload)) => {
                        // A fatal error may have quarantined the plugin while this worker was
                        // waiting, and an unload may have retired this load. Do not enter the
                        // guest once either has happened; messages already queued are discarded.
                        if !worker_callback_is_current(&load_alive, &quarantined) {
                            break;
                        }
                        if let Err(e) = worker.send_message(message, payload) {
                            log::error!("Failed to send message to worker: {:?}", e);
                            // Other failures leave the plugin running, as they always have. A
                            // worker that exhausts its fuel is what a runaway plugin looks like,
                            // so it is treated like the plugin's own callbacks doing so.
                            // TODO(plugin-hardening): ordinary worker traps (panics) are still
                            // only logged; quarantine the plugin on those as well once it is
                            // decided that a crashed worker should stop its plugin.
                            if crate::plugins::shared::is_out_of_fuel(&e) {
                                let data = worker.store.data();
                                crate::plugins::wasm_bridge::handle_plugin_crash(
                                    data.plugin_id,
                                    data.generation,
                                    format!("{:?}", e).replace("\n", "\n\r"),
                                    data.senders.clone(),
                                );
                                break;
                            }
                        }
                    },
                    Some(MessageToWorker::Exit) => {
                        break;
                    },
                    None => {
                        log::error!("Failed to receive worker message on channel");
                        break;
                    },
                }
            }
        }
    });
    sender
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn worker_message_queues_are_bounded() {
        let (sender, _receiver) = channel(WORKER_CHANNEL_CAPACITY);
        for index in 0..WORKER_CHANNEL_CAPACITY {
            assert!(try_send_to_worker(
                &sender,
                MessageToWorker::Message(format!("message-{index}"), "payload".to_owned())
            ));
        }
        assert!(
            !try_send_to_worker(
                &sender,
                MessageToWorker::Message("overflow".to_owned(), "payload".to_owned())
            ),
            "a full worker queue must reject the message instead of growing"
        );
    }

    #[test]
    fn a_queued_worker_callback_does_not_run_after_retirement() {
        let load_alive = Arc::new(AtomicBool::new(true));
        let quarantined = Arc::new(AtomicBool::new(false));
        assert!(worker_callback_is_current(&load_alive, &quarantined));

        load_alive.store(false, Ordering::Release);
        assert!(
            !worker_callback_is_current(&load_alive, &quarantined),
            "a callback queued before retirement must not be dispatched"
        );
    }

    #[test]
    fn a_queued_worker_callback_does_not_run_after_quarantine() {
        let load_alive = Arc::new(AtomicBool::new(true));
        let quarantined = Arc::new(AtomicBool::new(false));
        quarantined.store(true, Ordering::SeqCst);
        assert!(!worker_callback_is_current(&load_alive, &quarantined));
    }
}
