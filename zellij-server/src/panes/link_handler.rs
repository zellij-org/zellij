use std::collections::HashMap;

use super::{AnsiCode, CharacterStyles, LinkAnchor, TerminalCharacter};

pub const ANCHOR_END_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter {
    character: '\u{0000}',
    width: 0,
    styles: CharacterStyles {
        foreground: Some(AnsiCode::Reset),
        background: Some(AnsiCode::Reset),
        strike: Some(AnsiCode::Reset),
        hidden: Some(AnsiCode::Reset),
        reverse: Some(AnsiCode::Reset),
        slow_blink: Some(AnsiCode::Reset),
        fast_blink: Some(AnsiCode::Reset),
        underline: Some(AnsiCode::Reset),
        bold: Some(AnsiCode::Reset),
        dim: Some(AnsiCode::Reset),
        italic: Some(AnsiCode::Reset),
    },
    link_anchor: Some(LinkAnchor::End),
};

#[derive(Debug, Clone)]
pub struct LinkHandler {
    status: Status,
    links: HashMap<u16, Link>,
    link_index: u16,
    bell_terminated: bool,
}

#[derive(Debug, Clone)]
enum Status {
    Empty,
    Started,
    Ended,
}

#[derive(Debug, Clone)]
struct Link {
    id: Option<String>,
    uri: String,
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            status: Status::Empty,
            links: HashMap::new(),
            link_index: 0,
            bell_terminated: false,
        }
    }

    pub fn dispatch_osc8(&mut self, params: &[u8], uri: &[u8], bell_terminated: bool) {
        log::info!(
            "dispatching osc8, params: {:?}, uri: {:?}",
            std::str::from_utf8(params),
            std::str::from_utf8(uri)
        );
        self.bell_terminated = bell_terminated;

        if !uri.is_empty() {
            self.start(params, uri)
        } else {
            self.status = Status::Ended;
        }
    }

    pub fn pending_link_anchor(&mut self) -> Option<LinkAnchor> {
        match self.status {
            Status::Started => {
                let current_link_index = self.link_index;
                self.status = Status::Empty;
                self.link_index += 1;
                Some(LinkAnchor::Start(current_link_index))
            }
            Status::Ended => {
                self.status = Status::Empty;
                Some(LinkAnchor::End)
            }
            _ => None,
        }
    }

    pub fn insert_anchor_end(&mut self) -> Option<TerminalCharacter> {
        if let Status::Ended = self.status {
            self.status = Status::Empty;
            Some(ANCHOR_END_TERMINAL_CHARACTER)
        } else {
            None
        }
    }

    pub fn output_osc8(&self, t_character: TerminalCharacter) -> Option<String> {
        t_character.link_anchor.map(|link| {
            let terminator = if self.bell_terminated {
                "\x07"
            } else {
                "\x1b\\"
            };
            match link {
                LinkAnchor::Start(index) => {
                    let link = self.links.get(&index).unwrap();
                    let id = link
                        .id
                        .as_ref()
                        .map_or("".to_string(), |id| format!("id={}", id));
                    format!("\u{1b}]8;{};{}{}", id, link.uri, terminator)
                }
                LinkAnchor::End => format!("\u{1b}]8;;{}", terminator),
            }
        })
    }

    fn start(&mut self, params: &[u8], uri: &[u8]) {
        if let Ok(uri) = String::from_utf8(uri.to_vec()) {
            let id = params
                .split(|&b| b == b':')
                .find(|kv| kv.starts_with(b"id="))
                .and_then(|kv| String::from_utf8(kv[3..].to_vec()).ok());
            self.status = Status::Started;
            self.links.insert(self.link_index, Link { id, uri });
        }
    }
}

impl Default for LinkHandler {
    fn default() -> Self {
        Self::new()
    }
}
