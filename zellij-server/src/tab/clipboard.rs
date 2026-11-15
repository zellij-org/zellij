use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;
use zellij_utils::{data::CopyDestination, input::options::Clipboard};

use super::copy_command::CopyCommand;

const CLIPBOARD_QUEUE_CAPACITY: usize = 16;

/// Feeds a `copy_command` helper from a worker thread, so a helper that is
/// slow to read its input cannot stall the caller (the screen thread).
pub(crate) struct CommandClipboard {
    command: CopyCommand,
    /// Started on the first copy, so tabs that never copy cost no thread.
    sender: OnceLock<SyncSender<String>>,
}

impl CommandClipboard {
    fn new(command: CopyCommand) -> Self {
        Self {
            command,
            sender: OnceLock::new(),
        }
    }

    fn sender(&self) -> Result<&SyncSender<String>> {
        if let Some(sender) = self.sender.get() {
            return Ok(sender);
        }
        let (sender, receiver) = sync_channel(CLIPBOARD_QUEUE_CAPACITY);
        let command = self.command.clone();
        std::thread::Builder::new()
            .name("clipboard-helper".to_owned())
            .spawn(move || {
                while let Ok(content) = receiver.recv() {
                    if let Err(error) = command.set(content) {
                        log::error!("Clipboard helper failed: {:?}", error);
                    }
                }
            })
            .context("failed to start the clipboard helper thread")?;
        Ok(self.sender.get_or_init(|| sender))
    }

    fn set_content(&self, content: &str) -> Result<()> {
        self.sender()?
            .try_send(content.to_owned())
            .context("clipboard helper queue is full")
    }
}

pub(crate) enum ClipboardProvider {
    Command(CommandClipboard),
    Osc52(Clipboard),
}

impl ClipboardProvider {
    pub(crate) fn command(command: CopyCommand) -> Self {
        Self::Command(CommandClipboard::new(command))
    }

    pub(crate) fn set_content(&self, content: &str) -> Result<Option<String>> {
        match &self {
            ClipboardProvider::Command(command) => {
                command.set_content(content)?;
                Ok(None)
            },
            ClipboardProvider::Osc52(clipboard) => {
                let dest = match clipboard {
                    #[cfg(not(target_os = "macos"))]
                    Clipboard::Primary => 'p',
                    #[cfg(target_os = "macos")] // primary selection does not exist on macos
                    Clipboard::Primary => 'c',
                    Clipboard::System => 'c',
                };
                Ok(Some(format!(
                    "\u{1b}]52;{};{}\u{1b}\\",
                    dest,
                    BASE64_STANDARD.encode(content)
                )))
            },
        }
    }

    pub(crate) fn as_copy_destination(&self) -> CopyDestination {
        match self {
            ClipboardProvider::Command(_) => CopyDestination::Command,
            ClipboardProvider::Osc52(clipboard) => match clipboard {
                Clipboard::Primary => CopyDestination::Primary,
                Clipboard::System => CopyDestination::System,
            },
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::tab::copy_command::CopyCommand;
    use std::time::{Duration, Instant};

    #[test]
    fn command_clipboard_transfer_does_not_block_the_caller() {
        // The selection is larger than any pipe buffer, so writing it on the
        // caller's thread would block until the helper starts reading.
        let dir = tempfile::tempdir().unwrap();
        let received = dir.path().join("received");
        let command = format!("sh -c 'sleep 2; cat > {}'", received.display());
        let provider = ClipboardProvider::command(CopyCommand::new(command));
        let selection = "x".repeat(1024 * 1024);

        let started = Instant::now();
        assert!(provider.set_content(&selection).unwrap().is_none());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the helper transfer must run off the caller thread"
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        let received_len = || std::fs::metadata(&received).map_or(0, |meta| meta.len());
        while received_len() != selection.len() as u64 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(&received).unwrap(), selection);
    }
}
