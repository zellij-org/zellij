//! Definitions and helpers for sending and receiving messages between threads.

use crate::{
    os_input_output::ServerOsApi, pty::PtyInstruction, screen::ScreenInstruction,
    wasm_vm::PluginInstruction, ServerInstruction,
};
use zellij_utils::{channels, channels::SenderWithContext, errors::ErrorContext};

/// A container for senders to the different threads in zellij on the server side
#[derive(Default, Clone)]
pub(crate) struct ThreadSenders {
    pub to_screen: Option<SenderWithContext<ScreenInstruction>>,
    pub to_pty: Option<SenderWithContext<PtyInstruction>>,
    pub to_plugin: Option<SenderWithContext<PluginInstruction>>,
    pub to_server: Option<SenderWithContext<ServerInstruction>>,
}

impl ThreadSenders {
    pub fn send_to_screen(
        &self,
        instruction: ScreenInstruction,
    ) -> Result<(), channels::SendError<(ScreenInstruction, ErrorContext)>> {
        // self.to_screen.as_ref().unwrap().send(instruction)
        self.to_screen.as_ref().map(|sender| sender.send(instruction)).unwrap_or_else(|| Ok(())) // TODO: check with Kunal
    }

    pub fn send_to_pty(
        &self,
        instruction: PtyInstruction,
    ) -> Result<(), channels::SendError<(PtyInstruction, ErrorContext)>> {
        // self.to_pty.as_ref().unwrap().send(instruction)
        self.to_pty.as_ref().map(|sender| sender.send(instruction)).unwrap_or_else(|| Ok(())) // TODO: check with Kunal
    }

    pub fn send_to_plugin(
        &self,
        instruction: PluginInstruction,
    ) -> Result<(), channels::SendError<(PluginInstruction, ErrorContext)>> {
        // self.to_plugin.as_ref().unwrap().send(instruction)
        self.to_plugin.as_ref().map(|sender| sender.send(instruction)).unwrap_or_else(|| Ok(())) // TODO: check with Kunal
    }

    pub fn send_to_server(
        &self,
        instruction: ServerInstruction,
    ) -> Result<(), channels::SendError<(ServerInstruction, ErrorContext)>> {
        // self.to_server.as_ref().unwrap().send(instruction)
        self.to_server.as_ref().map(|sender| sender.send(instruction)).unwrap_or_else(|| Ok(())) // TODO: check with Kunal
    }
}

/// A container for a receiver, OS input and the senders to a given thread
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
        os_input: Option<Box<dyn ServerOsApi>>,
    ) -> Self {
        Bus {
            receivers,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_server: to_server.cloned(),
            },
            os_input: os_input.clone(),
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
