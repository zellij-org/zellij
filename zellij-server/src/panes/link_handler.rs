use std::collections::HashMap;

use super::LinkAnchor;

const TERMINATOR: &str = "\u{1b}\\";

#[derive(Debug, Clone)]
pub struct LinkHandler {
    links: HashMap<u16, Link>,
    link_index: u16,
}
#[derive(Debug, Clone)]
pub struct Link {
    pub id: Option<String>,
    pub uri: String,
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            links: HashMap::new(),
            link_index: 0,
        }
    }

    pub fn dispatch_osc8(&mut self, params: &[&[u8]]) -> Option<LinkAnchor> {
        let (link_params, uri) = (params[1], params[2]);
        log::debug!(
            "dispatching osc8, params: {:?}, uri: {:?}",
            std::str::from_utf8(link_params),
            std::str::from_utf8(uri)
        );

        if !uri.is_empty() {
            // save the link, and the id if present to hashmap
            String::from_utf8(uri.to_vec()).ok().map(|uri| {
                let id = link_params
                    .split(|&b| b == b':')
                    .find(|kv| kv.starts_with(b"id="))
                    .and_then(|kv| String::from_utf8(kv[3..].to_vec()).ok());
                let anchor = LinkAnchor::Start(self.link_index);
                self.links.insert(self.link_index, Link { id, uri });
                self.link_index += 1;
                anchor
            })
        } else {
            // there is no link, so consider it a link end
            Some(LinkAnchor::End)
        }
    }

    pub fn new_link_from_url(&mut self, url: String) -> LinkAnchor {
        let anchor = LinkAnchor::Start(self.link_index);
        self.links.insert(
            self.link_index,
            Link {
                id: Some(self.link_index.to_string()),
                uri: url,
            },
        );
        self.link_index += 1;
        anchor
    }

    pub fn output_osc8(&self, link_anchor: Option<LinkAnchor>) -> Option<String> {
        link_anchor.and_then(|link| match link {
            LinkAnchor::Start(index) => {
                let link = self.links.get(&index);

                let output = link.map(|link| {
                    let id = link
                        .id
                        .as_ref()
                        .map_or("".to_string(), |id| format!("id={}", id));
                    format!("\u{1b}]8;{};{}{}", id, link.uri, TERMINATOR)
                });

                if output.is_none() {
                    log::warn!(
                        "attempted to output osc8 link start, but id: {} was not found!",
                        index
                    );
                }

                output
            },
            LinkAnchor::End => Some(format!("\u{1b}]8;;{}", TERMINATOR)),
        })
    }

    #[cfg(test)]
    pub fn links(&self) -> HashMap<u16, Link> {
        self.links.clone()
    }
}

impl Default for LinkHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_osc8_link_start() {
        let mut link_handler = LinkHandler::default();
        let link_params = "id=test";
        let uri = "http://test.com";
        let params = vec!["8".as_bytes(), link_params.as_bytes(), uri.as_bytes()];

        let anchor = link_handler.dispatch_osc8(&params);

        match anchor {
            Some(LinkAnchor::Start(link_id)) => {
                let link = link_handler.links.get(&link_id).expect("link was not some");
                assert_eq!(link.id, Some("test".to_string()));
                assert_eq!(link.uri, uri);
            },
            _ => panic!("pending link handler was not start"),
        }

        let expected = format!("\u{1b}]8;id=test;http://test.com{}", TERMINATOR);
        assert_eq!(link_handler.output_osc8(anchor).unwrap(), expected);
    }

    #[test]
    fn dispatch_osc8_link_end() {
        let mut link_handler = LinkHandler::default();
        let params: Vec<&[_]> = vec![b"8", b"", b""];

        let anchor = link_handler.dispatch_osc8(&params);

        assert_eq!(anchor, Some(LinkAnchor::End));

        let expected = format!("\u{1b}]8;;{}", TERMINATOR);
        assert_eq!(link_handler.output_osc8(anchor).unwrap(), expected);
    }

    #[test]
    fn return_none_on_missing_link_id() {
        let link_handler = LinkHandler::default();
        let anchor = LinkAnchor::Start(100);
        assert_eq!(link_handler.output_osc8(Some(anchor)), None);
    }
}
