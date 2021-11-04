use std::collections::HashMap;

use super::{Cursor, LinkAnchor};

const TERMINATOR: &str = "\x1b\\";

#[derive(Debug, Clone)]
pub struct LinkHandler {
    pending_link_anchor: Option<LinkAnchor>,
    links: HashMap<u16, Link>,
    link_index: u16,
}
#[derive(Debug, Clone)]
struct Link {
    id: Option<String>,
    uri: String,
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            pending_link_anchor: None,
            links: HashMap::new(),
            link_index: 0,
        }
    }

    pub fn dispatch_osc8(&mut self, params: &[&[u8]], _bell_terminated: bool, cursor: &mut Cursor) {
        let (link_params, uri) = (params[1], params[2]);
        log::info!(
            "dispatching osc8, params: {:?}, uri: {:?}",
            std::str::from_utf8(link_params),
            std::str::from_utf8(uri)
        );

        if !uri.is_empty() {
            self.start(link_params, uri)
        } else {
            self.pending_link_anchor = Some(LinkAnchor::End);
        }
        cursor.pending_styles.link_anchor = self.pending_link_anchor();
    }

    pub fn pending_link_anchor(&mut self) -> Option<LinkAnchor> {
        let pending_link_anchor = self.pending_link_anchor;
        if let Some(LinkAnchor::End) = self.pending_link_anchor {
            self.pending_link_anchor = None;
        }
        pending_link_anchor
    }

    pub fn output_osc8(&self, link_anchor: Option<LinkAnchor>) -> String {
        link_anchor.map_or("".to_string(), |link| match link {
            LinkAnchor::Start(index) => {
                let link = self.links.get(&index).unwrap();
                let id = link
                    .id
                    .as_ref()
                    .map_or("".to_string(), |id| format!("id={}", id));
                format!("\u{1b}]8;{};{}{}", id, link.uri, TERMINATOR)
            }
            LinkAnchor::End => format!("\u{1b}]8;;{}", TERMINATOR),
        })
    }

    fn start(&mut self, params: &[u8], uri: &[u8]) {
        if let Ok(uri) = String::from_utf8(uri.to_vec()) {
            let id = params
                .split(|&b| b == b':')
                .find(|kv| kv.starts_with(b"id="))
                .and_then(|kv| String::from_utf8(kv[3..].to_vec()).ok());
            self.pending_link_anchor = Some(LinkAnchor::Start(self.link_index));
            self.links.insert(self.link_index, Link { id, uri });
            self.link_index += 1;
        }
    }
}

impl Default for LinkHandler {
    fn default() -> Self {
        Self::new()
    }
}
