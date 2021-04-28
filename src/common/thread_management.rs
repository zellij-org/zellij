use crate::common::SyncChannelWithContext;
use crate::common::{
    ChannelWithContext, PluginInstruction, PtyInstruction, ScreenInstruction, SenderType,
    SenderWithContext,
};
use std::sync::mpsc;

use super::{errors::ErrorContext, AppInstruction};

pub type ReceiverWithContext<T> = mpsc::Receiver<(T, ErrorContext)>;

pub struct ChannelCommunication<T: Clone> {
    sender: SenderWithContext<T>,
    receiver: mpsc::Receiver<(T, ErrorContext)>,
}
impl<T: Clone> ChannelCommunication<T> {
    pub fn new(sender: SenderWithContext<T>, receiver: ReceiverWithContext<T>) -> Self {
        Self { sender, receiver }
    }
}

pub struct ThreadCommunicationManager {
    screen_channel: ChannelCommunication<ScreenInstruction>,
    pty_channel: ChannelCommunication<PtyInstruction>,
    plugin_channel: ChannelCommunication<PluginInstruction>,
}
impl ThreadCommunicationManager {
    pub fn new() -> Self {
        let (send_screen_instructions, receive_screen_instructions): ChannelWithContext<
            ScreenInstruction,
        > = mpsc::channel();
        let send_screen_instructions =
            SenderWithContext::new(SenderType::Sender(send_screen_instructions));

        let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
            mpsc::channel();
        let send_pty_instructions =
            SenderWithContext::new(SenderType::Sender(send_pty_instructions));

        let (send_plugin_instructions, receive_plugin_instructions): ChannelWithContext<
            PluginInstruction,
        > = mpsc::channel();
        let send_plugin_instructions =
            SenderWithContext::new(SenderType::Sender(send_plugin_instructions));

        Self {
            screen_channel: ChannelCommunication::new(
                send_screen_instructions,
                receive_screen_instructions,
            ),
            pty_channel: ChannelCommunication::new(send_pty_instructions, receive_pty_instructions),
            plugin_channel: ChannelCommunication::new(
                send_plugin_instructions,
                receive_plugin_instructions,
            ),
        }
    }
    pub fn send_screen_instructions_clone(&self) -> SenderWithContext<ScreenInstruction> {
        self.screen_channel.sender.clone()
    }
    pub fn send_screen_instructions(&self) -> &SenderWithContext<ScreenInstruction> {
        &self.screen_channel.sender
    }
    pub fn receive_screen_instructions_clone(&self) -> &ReceiverWithContext<ScreenInstruction> {
        &self.screen_channel.receiver
    }

    pub fn send_pty_instructions_clone(&self) -> SenderWithContext<PtyInstruction> {
        self.pty_channel.sender.clone()
    }
    pub fn send_pty_instructions(&self) -> &SenderWithContext<PtyInstruction> {
        &self.pty_channel.sender
    }
    pub fn receive_pty_instructions_clone(&self) -> &ReceiverWithContext<PtyInstruction> {
        &self.pty_channel.receiver
    }

    pub fn send_plugin_instructions_clone(&self) -> SenderWithContext<PluginInstruction> {
        self.plugin_channel.sender.clone()
    }
    pub fn send_plugin_instructions(&self) -> &SenderWithContext<PluginInstruction> {
        &self.plugin_channel.sender
    }
    pub fn receive_plugin_instructions_clone(&self) -> &ReceiverWithContext<PluginInstruction> {
        &self.plugin_channel.receiver
    }
}

pub struct ThreadSyncCommuncationManager {
    app_channel: ChannelCommunication<AppInstruction>,
}
impl ThreadSyncCommuncationManager {
    pub fn new() -> Self {
        let (send_app_instructions, receive_app_instructions): SyncChannelWithContext<
            AppInstruction,
        > = mpsc::sync_channel(0);
        let send_app_instructions =
            SenderWithContext::new(SenderType::SyncSender(send_app_instructions));

        Self {
            app_channel: ChannelCommunication::new(send_app_instructions, receive_app_instructions),
        }
    }

    pub fn send_app_instructions_clone(&self) -> SenderWithContext<AppInstruction> {
        self.app_channel.sender.clone()
    }
    pub fn send_app_instructions(&self) -> &SenderWithContext<AppInstruction> {
        &self.app_channel.sender
    }
    pub fn receive_app_instructions(&self) -> &ReceiverWithContext<AppInstruction> {
        &self.app_channel.receiver
    }
}
