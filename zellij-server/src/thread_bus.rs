//! Definitions and helpers for sending and receiving messages between threads.

use crate::{
    os_input_output::ServerOsApi, pty::PtyInstruction, screen::ScreenInstruction,
    tty_writer::TtyWriteInstruction, wasm_vm::PluginInstruction, ServerInstruction,
};
use zellij_utils::{channels, channels::SenderWithContext, errors::ErrorContext};

/// A container for senders to the different threads in zellij on the server side
#[derive(Default, Clone)]
pub(crate) struct ThreadSenders {
    pub to_screen: Option<SenderWithContext<ScreenInstruction>>,
    pub to_pty: Option<SenderWithContext<PtyInstruction>>,
    pub to_plugin: Option<SenderWithContext<PluginInstruction>>,
    pub to_server: Option<SenderWithContext<ServerInstruction>>,
    pub to_tty_writer: Option<SenderWithContext<TtyWriteInstruction>>,
    // this is a convenience for the unit tests
    // it's not advisable to set it to true in production code
    pub should_silently_fail: bool,
}

impl ThreadSenders {
    pub fn send_to_screen(
        &self,
        instruction: ScreenInstruction,
    ) -> Result<(), channels::SendError<(ScreenInstruction, ErrorContext)>> {
        if self.should_silently_fail {
            let _ = self
                .to_screen
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_screen.as_ref().unwrap().send(instruction)
        }
    }

    pub fn send_to_pty(
        &self,
        instruction: PtyInstruction,
    ) -> Result<(), channels::SendError<(PtyInstruction, ErrorContext)>> {
        if self.should_silently_fail {
            let _ = self
                .to_pty
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_pty.as_ref().unwrap().send(instruction)
        }
    }

    pub fn send_to_plugin(
        &self,
        instruction: PluginInstruction,
    ) -> Result<(), channels::SendError<(PluginInstruction, ErrorContext)>> {
        if self.should_silently_fail {
            let _ = self
                .to_plugin
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_plugin.as_ref().unwrap().send(instruction)
        }
    }

    pub fn send_to_server(
        &self,
        instruction: ServerInstruction,
    ) -> Result<(), channels::SendError<(ServerInstruction, ErrorContext)>> {
        if self.should_silently_fail {
            let _ = self
                .to_server
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_server.as_ref().unwrap().send(instruction)
        }
    }
    pub fn send_to_tty_writer(
        &self,
        instruction: TtyWriteInstruction,
    ) -> Result<(), channels::SendError<(TtyWriteInstruction, ErrorContext)>> {
        if self.should_silently_fail {
            let _ = self
                .to_tty_writer
                .as_ref()
                .map(|sender| sender.send(instruction))
                .unwrap_or_else(|| Ok(()));
            Ok(())
        } else {
            self.to_tty_writer.as_ref().unwrap().send(instruction)
        }
    }

    #[allow(unused)]
    pub fn silently_fail_on_send(mut self) -> Self {
        // this is mostly used for the tests, see struct
        self.should_silently_fail = true;
        self
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
        to_tty_writer: Option<&SenderWithContext<TtyWriteInstruction>>,
        os_input: Option<Box<dyn ServerOsApi>>,
    ) -> Self {
        Bus {
            receivers,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_server: to_server.cloned(),
                to_tty_writer: to_tty_writer.cloned(),
                should_silently_fail: false,
            },
            os_input: os_input.clone(),
        }
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
                to_tty_writer: None,
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
