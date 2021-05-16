//! Definitions and helpers for sending and receiving messages between threads.

use crate::{
    os_input_output::ServerOsApi, pty::PtyInstruction, screen::ScreenInstruction,
    wasm_vm::PluginInstruction, ServerInstruction,
};
use std::sync::mpsc;
use zellij_utils::{channels::SenderWithContext, errors::ErrorContext};

/// A container for senders to the different threads in zellij on the server side
#[derive(Clone)]
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
    ) -> Result<(), mpsc::SendError<(ScreenInstruction, ErrorContext)>> {
        self.to_screen.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_pty(
        &self,
        instruction: PtyInstruction,
    ) -> Result<(), mpsc::SendError<(PtyInstruction, ErrorContext)>> {
        self.to_pty.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_plugin(
        &self,
        instruction: PluginInstruction,
    ) -> Result<(), mpsc::SendError<(PluginInstruction, ErrorContext)>> {
        self.to_plugin.as_ref().unwrap().send(instruction)
    }

    pub fn send_to_server(
        &self,
        instruction: ServerInstruction,
    ) -> Result<(), mpsc::SendError<(ServerInstruction, ErrorContext)>> {
        self.to_server.as_ref().unwrap().send(instruction)
    }
}

/// A container for a receiver, OS input and the senders to a given thread
pub(crate) struct Bus<T> {
    pub receiver: mpsc::Receiver<(T, ErrorContext)>,
    pub senders: ThreadSenders,
    pub os_input: Option<Box<dyn ServerOsApi>>,
}

impl<T> Bus<T> {
    pub fn new(
        receiver: mpsc::Receiver<(T, ErrorContext)>,
        to_screen: Option<&SenderWithContext<ScreenInstruction>>,
        to_pty: Option<&SenderWithContext<PtyInstruction>>,
        to_plugin: Option<&SenderWithContext<PluginInstruction>>,
        to_server: Option<&SenderWithContext<ServerInstruction>>,
        os_input: Option<Box<dyn ServerOsApi>>,
    ) -> Self {
        Bus {
            receiver,
            senders: ThreadSenders {
                to_screen: to_screen.cloned(),
                to_pty: to_pty.cloned(),
                to_plugin: to_plugin.cloned(),
                to_server: to_server.cloned(),
            },
            os_input: os_input.clone(),
        }
    }

    pub fn recv(&self) -> Result<(T, ErrorContext), mpsc::RecvError> {
        self.receiver.recv()
    }
}
