use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use zellij_utils::{data::CopyDestination, input::options::Clipboard};

use super::copy_command::CopyCommand;

pub(crate) enum ClipboardProvider {
    Command(CopyCommand),
    Osc52(Clipboard),
}

impl ClipboardProvider {
    pub(crate) fn set_content(&self, content: &str) -> Result<Option<String>> {
        match &self {
            ClipboardProvider::Command(command) => {
                command.set(content.to_string())?;
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
