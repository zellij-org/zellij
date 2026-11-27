use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, OnceLock};
use zellij_utils::{
    data::{CopyDestination, Event},
    input::options::Clipboard,
};

use super::copy_command::CopyCommand;
use crate::plugins::PluginInstruction;
use crate::thread_bus::ThreadSenders;

const CLIPBOARD_QUEUE_CAPACITY: usize = 16;
const CLIPBOARD_QUEUE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Feeds a `copy_command` helper from a worker thread, so a helper that is
/// slow to read its input cannot stall the caller (the screen thread).
///
/// The guarantee is that a helper which stops consuming stdin cannot
/// indefinitely block Screen: the worker has a per-transfer write timeout and
/// kills an unresponsive immediate child. This does not bound the helper's
/// total lifetime. A descendant that inherits the pipe read end can also keep
/// the writer thread blocked after the immediate child is killed; that known
/// residual is not addressed here.
///
/// Completion notifications are best-effort under plugin-channel backpressure:
/// the helper may accept input while a full plugin mailbox drops
/// `CopyToClipboard` or `SystemClipboardFailure`. The clipboard worker must not
/// block or stop progressing because notification delivery failed.
pub(crate) struct CommandClipboard {
    command: CopyCommand,
    /// Started on the first copy, so tabs that never copy cost no thread.
    sender: OnceLock<std::result::Result<SyncSender<String>, String>>,
    /// Bytes accepted into the queue and not yet written to a helper.
    queued_bytes: Arc<AtomicUsize>,
    completion_senders: Option<ThreadSenders>,
    completion_destination: CopyDestination,
}

impl CommandClipboard {
    fn new(
        command: CopyCommand,
        completion_senders: Option<ThreadSenders>,
        completion_destination: CopyDestination,
    ) -> Self {
        Self {
            command,
            sender: OnceLock::new(),
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            completion_senders,
            completion_destination,
        }
    }

    fn sender(&self) -> Result<&SyncSender<String>> {
        self.sender
            .get_or_init(|| {
                let (sender, receiver) = sync_channel::<String>(CLIPBOARD_QUEUE_CAPACITY);
                let command = self.command.clone();
                let queued_bytes = Arc::clone(&self.queued_bytes);
                let completion_senders = self.completion_senders.clone();
                let completion_destination = self.completion_destination;
                std::thread::Builder::new()
                    .name("clipboard-helper".to_owned())
                    .spawn(move || {
                        while let Ok(content) = receiver.recv() {
                            let content_bytes = content.len();
                            let clipboard_event = match command.set(content) {
                                Ok(()) => Event::CopyToClipboard(completion_destination),
                                Err(error) => {
                                    log::error!("Clipboard helper failed: {:?}", error);
                                    Event::SystemClipboardFailure
                                },
                            };
                            queued_bytes.fetch_sub(content_bytes, Ordering::AcqRel);
                            if let Some(senders) = &completion_senders {
                                // Completion delivery is best-effort under plugin-channel
                                // backpressure: the helper may accept input while a full plugin
                                // mailbox drops this event. The clipboard worker must not block or
                                // stop progressing because it was dropped.
                                if let Err(error) = senders.try_send_to_plugin(
                                    PluginInstruction::Update(vec![(None, None, clipboard_event)]),
                                ) {
                                    log::error!(
                                        "Failed to notify plugins about clipboard event: {}",
                                        error
                                    );
                                }
                            }
                        }
                    })
                    .map(|_| sender)
                    .map_err(|error| {
                        format!("failed to start the clipboard helper thread: {error}")
                    })
            })
            .as_ref()
            .map_err(|error| anyhow!("{error}"))
    }

    fn reserve_bytes(&self, content_bytes: usize) -> bool {
        self.queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued_bytes| {
                queued_bytes
                    .checked_add(content_bytes)
                    .filter(|total| *total <= CLIPBOARD_QUEUE_MAX_BYTES)
            })
            .is_ok()
    }

    fn release_bytes(&self, content_bytes: usize) {
        self.queued_bytes.fetch_sub(content_bytes, Ordering::AcqRel);
    }

    /// Queue `content` for the helper. `Ok(())` means the transfer was accepted
    /// for processing, not that the external clipboard was updated.
    fn set_content(&self, content: &str) -> Result<()> {
        let sender = self.sender()?;
        let content_bytes = content.len();
        if !self.reserve_bytes(content_bytes) {
            bail!(
                "clipboard helper queue byte budget of {} bytes exceeded",
                CLIPBOARD_QUEUE_MAX_BYTES
            );
        }
        match sender.try_send(content.to_owned()) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.release_bytes(content_bytes);
                Err(error).context("clipboard helper queue is full")
            },
        }
    }
}

pub(crate) enum ClipboardProvider {
    Command(CommandClipboard),
    Osc52(Clipboard),
}

impl ClipboardProvider {
    #[cfg(test)]
    pub(crate) fn command(command: CopyCommand) -> Self {
        Self::Command(CommandClipboard::new(
            command,
            None,
            CopyDestination::Command,
        ))
    }

    pub(crate) fn command_with_completion(
        command: CopyCommand,
        senders: ThreadSenders,
        destination: CopyDestination,
    ) -> Self {
        Self::Command(CommandClipboard::new(command, Some(senders), destination))
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

    pub(crate) fn reports_completion_asynchronously(&self) -> bool {
        matches!(
            self,
            ClipboardProvider::Command(command) if command.completion_senders.is_some()
        )
    }
}

#[cfg(test)]
mod osc52_tests {
    use super::*;

    #[test]
    fn system_clipboard_osc52_is_unchanged() {
        let provider = ClipboardProvider::Osc52(Clipboard::System);
        assert_eq!(
            provider.set_content("hi").unwrap().unwrap(),
            "\u{1b}]52;c;aGk=\u{1b}\\"
        );
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn primary_clipboard_osc52_is_unchanged() {
        let provider = ClipboardProvider::Osc52(Clipboard::Primary);
        assert_eq!(
            provider.set_content("hi").unwrap().unwrap(),
            "\u{1b}]52;p;aGk=\u{1b}\\"
        );
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::tab::copy_command::CopyCommand;
    use std::sync::Barrier;
    use std::time::{Duration, Instant};
    use zellij_utils::{
        channels::{bounded, Receiver, SenderWithContext},
        errors::ErrorContext,
    };

    fn queued_bytes(provider: &ClipboardProvider) -> usize {
        match provider {
            ClipboardProvider::Command(clipboard) => clipboard.queued_bytes.load(Ordering::Acquire),
            ClipboardProvider::Osc52(_) => unreachable!("expected a command clipboard"),
        }
    }

    fn plugin_channel(
        capacity: usize,
    ) -> (ThreadSenders, Receiver<(PluginInstruction, ErrorContext)>) {
        let (sender, receiver) = bounded::<(PluginInstruction, ErrorContext)>(capacity);
        let senders = ThreadSenders {
            to_plugin: Some(SenderWithContext::new(sender)),
            ..Default::default()
        };
        (senders, receiver)
    }

    #[test]
    fn command_clipboard_transfer_does_not_block_the_caller() {
        // The selection is larger than any pipe buffer, so writing it on the
        // caller's thread would block until the helper starts reading.
        let dir = tempfile::tempdir().unwrap();
        let received = dir.path().join("received");
        let script = dir.path().join("slow-helper");
        std::fs::write(
            &script,
            format!("sleep 2\ncat > '{}'\n", received.display()),
        )
        .unwrap();
        let command = format!("sh {}", script.display());
        let provider = ClipboardProvider::command(CopyCommand::new(command));
        let selection = "x".repeat(1024 * 1024);

        let started = Instant::now();
        assert!(provider.set_content(&selection).unwrap().is_none());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the helper transfer must run off the caller thread"
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while std::fs::metadata(&received)
            .map(|metadata| metadata.len())
            .unwrap_or(0)
            != selection.len() as u64
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(&received).unwrap(), selection);
    }

    #[test]
    fn command_clipboard_queue_preserves_order_and_rejects_when_full() {
        let dir = tempfile::tempdir().unwrap();
        let started_flag = dir.path().join("started");
        let release = dir.path().join("release");
        let received = dir.path().join("received");
        let script = dir.path().join("slow-helper");
        // Block before reading so the worker stays inside the in-flight
        // transfer. `flock` then keeps each append intact and in submission
        // order even though a helper is not killed after it accepts stdin.
        std::fs::write(
            &script,
            format!(
                "touch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.02; done\nexec 9>>'{}'\nflock 9\ncat >&9\n",
                started_flag.display(),
                release.display(),
                received.display()
            ),
        )
        .unwrap();
        let provider =
            ClipboardProvider::command(CopyCommand::new(format!("sh {}", script.display())));
        // Larger than the default pipe buffer, so the in-flight write cannot
        // finish until this helper reaches `cat`.
        let body = "y".repeat(128 * 1024);
        let first = format!("00{body}");

        let enqueue_started = Instant::now();
        assert!(provider.set_content(&first).unwrap().is_none());
        let ready_deadline = Instant::now() + Duration::from_secs(5);
        while !started_flag.exists() && Instant::now() < ready_deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            started_flag.exists(),
            "the helper should be holding the in-flight transfer before the queue fills"
        );

        let mut accepted = vec![first];
        let mut rejected = false;
        for i in 1..(CLIPBOARD_QUEUE_CAPACITY + 4) {
            let content = format!("{i:02}{body}");
            match provider.set_content(&content) {
                Ok(None) => {
                    accepted.push(content);
                },
                Ok(Some(instruction)) => panic!("unexpected immediate transfer: {instruction:?}"),
                Err(_) => {
                    rejected = true;
                    break;
                },
            }
        }
        assert!(
            enqueue_started.elapsed() < Duration::from_secs(2),
            "a full queue must fail the caller without waiting for the helper"
        );
        assert!(rejected, "a full clipboard queue must reject the transfer");
        assert_eq!(
            accepted.len(),
            CLIPBOARD_QUEUE_CAPACITY + 1,
            "one in-flight transfer plus the bounded queue"
        );

        std::fs::write(&release, b"").unwrap();
        let expected = accepted.concat();
        let deadline = Instant::now() + Duration::from_secs(15);
        while std::fs::metadata(&received)
            .map(|meta| meta.len())
            .unwrap_or(0)
            < expected.len() as u64
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read_to_string(&received).unwrap(), expected);
        let deadline = Instant::now() + Duration::from_secs(5);
        while queued_bytes(&provider) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            queued_bytes(&provider),
            0,
            "rejected and completed transfers must release their byte reservations"
        );
    }

    #[test]
    fn command_clipboard_rejects_transfers_over_the_byte_budget() {
        let dir = tempfile::tempdir().unwrap();
        let started_flag = dir.path().join("started");
        let release = dir.path().join("release");
        let received = dir.path().join("received");
        let script = dir.path().join("blocked-helper");
        std::fs::write(
            &script,
            format!(
                "touch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.02; done\nexec 9>>'{}'\nflock 9\ncat >&9\n",
                started_flag.display(),
                release.display(),
                received.display()
            ),
        )
        .unwrap();
        let provider =
            ClipboardProvider::command(CopyCommand::new(format!("sh {}", script.display())));
        let chunk = "z".repeat(CLIPBOARD_QUEUE_MAX_BYTES / 8);

        assert!(provider.set_content(&chunk).unwrap().is_none());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !started_flag.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            started_flag.exists(),
            "the helper should hold the first transfer"
        );
        for _ in 1..8 {
            assert!(provider.set_content(&chunk).unwrap().is_none());
        }
        assert_eq!(queued_bytes(&provider), CLIPBOARD_QUEUE_MAX_BYTES);

        let rejected_started = Instant::now();
        let error = provider.set_content("x").unwrap_err();
        assert!(
            rejected_started.elapsed() < Duration::from_secs(1),
            "the byte budget must reject without waiting for the helper"
        );
        assert!(
            format!("{error:#}").contains("byte budget"),
            "unexpected error: {error:#}"
        );
        assert_eq!(queued_bytes(&provider), CLIPBOARD_QUEUE_MAX_BYTES);

        std::fs::write(&release, b"").unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while (queued_bytes(&provider) != 0
            || std::fs::metadata(&received)
                .map(|meta| meta.len())
                .unwrap_or(0)
                < (chunk.len() * 8) as u64)
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(queued_bytes(&provider), 0);
        assert_eq!(
            std::fs::metadata(&received).unwrap().len(),
            (chunk.len() * 8) as u64
        );
    }

    #[test]
    fn command_clipboard_notifies_plugin_after_success() {
        let dir = tempfile::tempdir().unwrap();
        let received = dir.path().join("received");
        let script = dir.path().join("successful-helper");
        std::fs::write(&script, format!("cat > '{}'\n", received.display())).unwrap();
        let (senders, plugin_receiver) = plugin_channel(8);
        let provider = ClipboardProvider::command_with_completion(
            CopyCommand::new(format!("sh {}", script.display())),
            senders,
            CopyDestination::Command,
        );

        assert!(provider.set_content("copied").unwrap().is_none());
        let (instruction, _) = plugin_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the worker should notify plugins after success");
        let PluginInstruction::Update(updates) = instruction else {
            panic!("expected a plugin update")
        };
        assert_eq!(
            updates,
            vec![(None, None, Event::CopyToClipboard(CopyDestination::Command))]
        );
    }

    #[test]
    fn command_clipboard_notifies_plugin_after_failure() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("closes-stdin");
        std::fs::write(&script, "exec 0<&-\nexit 0\n").unwrap();
        let (senders, plugin_receiver) = plugin_channel(8);
        let provider = ClipboardProvider::command_with_completion(
            CopyCommand::new(format!("sh {}", script.display())),
            senders,
            CopyDestination::Command,
        );
        let selection = "x".repeat(1024 * 1024);

        assert!(provider.set_content(&selection).unwrap().is_none());
        let (instruction, _) = plugin_receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("the worker should notify plugins after failure");
        let PluginInstruction::Update(updates) = instruction else {
            panic!("expected a plugin update")
        };
        assert_eq!(updates, vec![(None, None, Event::SystemClipboardFailure)]);
    }

    #[test]
    fn command_clipboard_drops_notifications_without_stalling_on_full_plugin_mailbox() {
        let dir = tempfile::tempdir().unwrap();
        let received = dir.path().join("received");
        let script = dir.path().join("appending-helper");
        std::fs::write(&script, format!("cat >> '{}'\n", received.display())).unwrap();

        let (senders, _plugin_receiver) = plugin_channel(1);
        let filler = PluginInstruction::Update(vec![(
            None,
            None,
            Event::CopyToClipboard(CopyDestination::Command),
        )]);
        senders
            .try_send_to_plugin(filler)
            .expect("the filler should occupy the plugin mailbox");

        let provider = ClipboardProvider::command_with_completion(
            CopyCommand::new(format!("sh {}", script.display())),
            senders,
            CopyDestination::Command,
        );
        for content in ["first\n", "second\n"] {
            assert!(provider.set_content(content).unwrap().is_none());
            let deadline = Instant::now() + Duration::from_secs(5);
            while queued_bytes(&provider) != 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                queued_bytes(&provider),
                0,
                "the worker must keep processing with a full plugin mailbox"
            );
        }

        let both_copies_received = || {
            let contents = std::fs::read_to_string(&received).unwrap_or_default();
            contents.contains("first\n") && contents.contains("second\n")
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        while !both_copies_received() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let contents = std::fs::read_to_string(&received).unwrap();
        assert!(
            contents.contains("first\n") && contents.contains("second\n"),
            "both copies should be processed despite dropped notifications: {contents:?}"
        );
    }

    #[test]
    fn command_clipboard_concurrent_first_use_is_safe() {
        let provider = Arc::new(ClipboardProvider::command(CopyCommand::new(
            "dd of=/dev/null status=none".to_owned(),
        )));
        let threads = 16;
        let barrier = Arc::new(Barrier::new(threads));
        let mut handles = Vec::new();
        for i in 0..threads {
            let provider = Arc::clone(&provider);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                assert!(provider
                    .set_content(&format!("copy-{i}"))
                    .unwrap()
                    .is_none());
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let deadline = Instant::now() + Duration::from_secs(5);
        while queued_bytes(&provider) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(queued_bytes(&provider), 0);
    }
}
