use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use zellij_utils::{data::CopyDestination, input::options::Clipboard};

use crate::ClientId;

use super::{copy_command::CopyCommand, Output};

pub(crate) enum ClipboardProvider {
    Command(CopyCommand),
    Osc52(Clipboard),
}

impl ClipboardProvider {
    pub(crate) fn set_content(
        &self,
        content: &str,
        output: &mut Output,
        client_ids: impl Iterator<Item = ClientId>,
    ) -> Result<()> {
        match &self {
            ClipboardProvider::Command(command) => {
                command.set(content.to_string())?;
            },
            ClipboardProvider::Osc52(clipboard) => {
                let destinations: &[char] = match clipboard {
                    #[cfg(not(target_os = "macos"))]
                    Clipboard::Primary => &['p'],
                    #[cfg(target_os = "macos")] // primary selection does not exist on macos
                    Clipboard::Primary => &['c'],
                    Clipboard::System => &['c'],
                    #[cfg(not(target_os = "macos"))]
                    Clipboard::Both => &['c', 'p'],
                    #[cfg(target_os = "macos")]
                    Clipboard::Both => &['c'],
                };
                let client_ids_vec: Vec<ClientId> = client_ids.collect();
                for &dest in destinations {
                    output.add_pre_vte_instruction_to_multiple_clients(
                        client_ids_vec.iter().copied(),
                        &format!(
                            "\u{1b}]52;{};{}\u{1b}\\",
                            dest,
                            BASE64_STANDARD.encode(content)
                        ),
                    );
                }
            },
        };
        Ok(())
    }

    pub(crate) fn as_copy_destination(&self) -> CopyDestination {
        match self {
            ClipboardProvider::Command(_) => CopyDestination::Command,
            ClipboardProvider::Osc52(clipboard) => match clipboard {
                Clipboard::Primary => CopyDestination::Primary,
                Clipboard::System | Clipboard::Both => CopyDestination::System,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osc52_copy_both_destinations() {
        use crate::tab::LinkHandler;
        use std::cell::RefCell;
        use std::collections::HashSet;
        use std::rc::Rc;

        let provider = ClipboardProvider::Osc52(Clipboard::Both);
        let mut output = Output::default();
        let client_ids: HashSet<ClientId> = vec![1].into_iter().collect();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
        output.add_clients(&client_ids, link_handler, None);
        provider
            .set_content("test data", &mut output, client_ids.iter().copied())
            .unwrap();

        #[cfg(not(target_os = "macos"))]
        {
            let serialized = output.serialize().unwrap();
            let encoded = BASE64_STANDARD.encode("test data");
            let expected_c = format!("\u{1b}]52;c;{}\u{1b}\\", encoded);
            let expected_p = format!("\u{1b}]52;p;{}\u{1b}\\", encoded);
            let serialized_str = serialized.get(&1).expect("client output");
            assert!(serialized_str.contains(&expected_c));
            assert!(serialized_str.contains(&expected_p));
        }
    }
}
