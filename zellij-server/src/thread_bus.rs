//! Definitions and helpers for sending and receiving messages between threads.

use crate::{
    background_jobs::BackgroundJob, os_input_output::ServerOsApi, plugins::PluginInstruction,
    pty::PtyInstruction, pty_writer::PtyWriteInstruction, screen::ScreenInstruction,
    ServerInstruction,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::{channels, channels::SenderWithContext, errors::ErrorContext};

/// A container for senders to the different threads in zellij on the server side
#[derive(Default, Clone)]
pub struct ThreadSenders {
    pub to_screen: Option<SenderWithContext<ScreenInstruction>>,
    pub to_pty: Option<SenderWithContext<PtyInstruction>>,
    pub to_plugin: Option<SenderWithContext<PluginInstruction>>,
    pub to_server: Option<SenderWithContext<ServerInstruction>>,
    pub to_pty_writer: Option<SenderWithContext<PtyWriteInstruction>>,
    pub to_background_jobs: Option<SenderWithContext<BackgroundJob>>,
    // this is a convenience for the unit tests
    // it's not advisable to set it to true in production code
    pub should_silently_fail: bool,
}

impl ThreadSenders {
    pub fn send_to_screen(&self, instruction: ScreenInstruction) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_screen
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_screen
                .as_ref()
                .context("failed to get screen sender")?
                .send(instruction)
                .to_anyhow()
                .context("failed to send message to screen")
        }
    }

    pub fn send_to_pty(&self, instruction: PtyInstruction) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_pty
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_pty
                .as_ref()
                .context("failed to get pty sender")?
                .send(instruction)
                .to_anyhow()
                .context("failed to send message to pty")
        }
    }

    pub fn send_to_plugin(&self, instruction: PluginInstruction) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_plugin
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_plugin
                .as_ref()
                .context("failed to get plugin sender")?
                .send(instruction)
                .to_anyhow()
                .context("failed to send message to plugin")
        }
    }

    pub fn send_to_server(&self, instruction: ServerInstruction) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_server
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_server
                .as_ref()
                .context("failed to get server sender")?
                .send(instruction)
                .to_anyhow()
                .context("failed to send message to server")
        }
    }
    pub fn send_to_pty_writer(&self, instruction: PtyWriteInstruction) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_pty_writer
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_pty_writer
                .as_ref()
                .context("failed to get pty writer sender")?
                .send(instruction)
                .to_anyhow()
                .context("failed to send message to pty writer")
        }
    }
    pub fn send_to_background_jobs(&self, background_job: BackgroundJob) -> Result<()> {
        if self.should_silently_fail {
            let _ = self
                .to_background_jobs
                .as_ref()
                .map(|sender| sender.send(background_job))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_background_jobs
                .as_ref()
                .context("failed to get background jobs sender")?
                .send(background_job)
                .to_anyhow()
                .context("failed to send message to background jobs")
        }
    }

    #[allow(unused)]
    pub fn silently_fail_on_send(mut self) -> Self {
        // this is mostly used for the tests, see struct
        self.should_silently_fail = true;
        self
    }
    #[allow(unused)]
    pub fn replace_to_pty_writer(
        &mut self,
        new_pty_writer: SenderWithContext<PtyWriteInstruction>,
    ) {
        // this is mostly used for the tests, see struct
        self.to_pty_writer.replace(new_pty_writer);
    }
    #[allow(unused)]
    pub fn replace_to_pty(&mut self, new_pty: SenderWithContext<PtyInstruction>) {
        // this is mostly used for the tests, see struct
        self.to_pty.replace(new_pty);
    }

    #[allow(unused)]
    pub fn replace_to_plugin(&mut self, new_to_plugin: SenderWithContext<PluginInstruction>) {
        // this is mostly used for the tests, see struct
        self.to_plugin.replace(new_to_plugin);
    }
}

/// A container for a receiver, OS input and the senders to a given thread
#[derive(Default)]
pub(crate) struct Bus<T> {
    receivers: Vec<channels::Receiver<(T, ErrorContext)>>,
    pub senders: ThreadSenders,
    pub os_input: Option<Box<dyn ServerOsApi>>,
}

impl<T> Bus<T> {
    pub fn new(
        receivers: Vec<channels::Receiver<(T, ErrorContext)>>,
        to_screen: Option<&SenderWithContext<ScreenInstruction>>,
        to_pty: Option<&SenderWithContext<PtyInstruction>>,
        to_plugin: Option<&SenderWithContext<PluginInstruction>>,
        to_server: Option<&SenderWithContext<ServerInstruction>>,
        to_pty_writer: Option<&SenderWithContext<PtyWriteInstruction>>,
        to_background_jobs: Option<&SenderWithContext<BackgroundJob>>,
        os_input: Option<Box<dyn ServerOsApi>>,
    ) -> Self {
        Bus {
            receivers,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_server: to_server.cloned(),
                to_pty_writer: to_pty_writer.cloned(),
                to_background_jobs: to_background_jobs.cloned(),
                should_silently_fail: false,
            },
            os_input: os_input.clone(),
        }
    }
    #[allow(unused)]
    pub fn should_silently_fail(mut self) -> Self {
        // this is mostly used for the tests
        self.senders.should_silently_fail = true;
        self
    }
    #[allow(unused)]
    pub fn empty() -> Self {
        // this is mostly used for the tests
        Bus {
            receivers: vec![],
            senders: ThreadSenders {
                to_screen: None,
                to_pty: None,
                to_plugin: None,
                to_server: None,
                to_pty_writer: None,
                to_background_jobs: None,
                should_silently_fail: true,
            },
            os_input: None,
        }
    }

    pub fn recv(&self) -> Result<(T, ErrorContext), channels::RecvError> {
        let mut selector = channels::Select::new();
        self.receivers.iter().for_each(|r| {
            selector.recv(r);
        });
        let oper = selector.select();
        let idx = oper.index();
        oper.recv(&self.receivers[idx])
    }
}
