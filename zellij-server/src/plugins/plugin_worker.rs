use std::sync::{Arc, Mutex};

use crate::plugins::plugin_loader::VersionMismatchError;
use crate::plugins::plugin_map::PluginEnv;
use crate::plugins::zellij_exports::wasi_write_object;
use wasmer::{AsStoreMut, Instance, Store};

use zellij_utils::async_channel::{unbounded, Receiver, Sender};
use zellij_utils::async_std::task;
use zellij_utils::errors::prelude::*;
use zellij_utils::{consts::VERSION, input::plugins::PluginConfig};

pub struct RunningWorker {
    pub instance: Instance,
    pub name: String,
    pub plugin_config: PluginConfig,
    pub plugin_env: PluginEnv,
    store: Store,
}

impl RunningWorker {
    pub fn new(
        store: Store,
        instance: Instance,
        name: &str,
        plugin_config: PluginConfig,
        plugin_env: PluginEnv,
    ) -> Self {
        RunningWorker {
            store,
            instance,
            name: name.into(),
            plugin_config,
            plugin_env,
        }
    }
    pub fn send_message(&mut self, message: String, payload: String) -> Result<()> {
        let err_context = || format!("Failed to send message to worker");

        let work_function = self
            .instance
            .exports
            .get_function(&self.name)
            .with_context(err_context)?;
        wasi_write_object(&self.plugin_env.wasi_env, &(message, payload))
            .with_context(err_context)?;
        work_function
            .call(&mut self.store, &[])
            .or_else::<anyError, _>(|e| match e.downcast::<serde_json::Error>() {
                Ok(_) => panic!(
                    "{}",
                    anyError::new(VersionMismatchError::new(
                        VERSION,
                        "Unavailable",
                        &self.plugin_config.path,
                        self.plugin_config.is_builtin(),
                    ))
                ),
                Err(e) => Err(e).with_context(err_context),
            })?;

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
