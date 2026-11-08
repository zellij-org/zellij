use std::collections::HashMap;

use super::LinkAnchor;

const TERMINATOR: &str = "\u{1b}\\";

/// Longest URI accepted for a hyperlink; longer targets are left as text.
const MAX_LINK_URI_BYTES: usize = 8 * 1024;
/// Longest OSC 8 `id=` parameter retained; longer ids are dropped.
const MAX_LINK_ID_BYTES: usize = 256;
/// Total URI and id bytes retained per handler (that is, per pane), counting both stored copies
/// of each target. Link ids are never reused while cells may still
/// reference them, so without this bound output could pin up to 65,536
/// maximum-length targets.
const MAX_LINK_BYTES_TOTAL: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LinkTarget {
    Explicit { id: Option<String>, uri: String },
    Detected(String),
}

/// The table of hyperlink targets for one pane. Cells store only an id into
/// it, so a pane owns its handler and keeps it wherever the pane is shown;
/// sharing one between panes (or per tab) would let ids resolve to targets
/// another pane registered once a pane changes tabs.
#[derive(Debug, Clone)]
pub struct LinkHandler {
    links: HashMap<u16, Link>,
    by_target: HashMap<LinkTarget, u16>,
    next_link_index: Option<u16>,
    retained_bytes: usize,
}
#[derive(Debug, Clone)]
pub struct Link {
    pub id: Option<String>,
    pub uri: String,
    /// Created by URL detection rather than by an OSC 8 sequence.
    pub detected: bool,
}

impl LinkHandler {
    pub fn new() -> Self {
        Self {
            links: HashMap::new(),
            by_target: HashMap::new(),
            next_link_index: Some(0),
            retained_bytes: 0,
        }
    }

    /// Return the anchor for `target`, reusing the id of an identical
    /// target. Reuse is never a spoof: the id keeps pointing at the same
    /// target. A new id is allocated only for a new target, and never wraps
    /// onto one that retained cells may still reference; once the id space
    /// or the byte budget is used up, new targets are left as plain text.
    fn anchor_for(&mut self, target: LinkTarget) -> Option<LinkAnchor> {
        if let Some(index) = self.by_target.get(&target) {
            return Some(LinkAnchor::Start(*index));
        }
        let index = self.next_link_index?;
        // Explicit ids are stored in `links` and in the `by_target` key;
        // generated ids for detected links only exist in `links`.
        let (id, uri, id_cost) = match &target {
            LinkTarget::Explicit { id, uri } => (
                id.clone(),
                uri.clone(),
                2 * id.as_ref().map_or(0, String::len),
            ),
            LinkTarget::Detected(uri) => {
                let generated = index.to_string();
                let cost = generated.len();
                (Some(generated), uri.clone(), cost)
            },
        };
        if uri.len() > MAX_LINK_URI_BYTES {
            return None;
        }
        // Each URI is stored twice: in `links` and as a `by_target` key.
        let cost = 2 * uri.len() + id_cost;
        let retained_bytes = self.retained_bytes.checked_add(cost)?;
        if retained_bytes > MAX_LINK_BYTES_TOTAL {
            return None;
        }
        self.next_link_index = index.checked_add(1);
        self.retained_bytes = retained_bytes;
        let detected = matches!(target, LinkTarget::Detected(_));
        self.links.insert(index, Link { id, uri, detected });
        self.by_target.insert(target, index);
        Some(LinkAnchor::Start(index))
    }

    pub fn dispatch_osc8(&mut self, params: &[&[u8]]) -> Option<LinkAnchor> {
        let [_osc, link_params, uri, ..] = params else {
            return None;
        };
        log::debug!(
            "dispatching osc8, params: {:?}, uri: {:?}",
            std::str::from_utf8(link_params),
            std::str::from_utf8(uri)
        );

        if !uri.is_empty() {
            let uri = String::from_utf8(uri.to_vec()).ok()?;
            let id = link_params
                .split(|&b| b == b':')
                .find(|kv| kv.starts_with(b"id="))
                .filter(|kv| kv.len() - 3 <= MAX_LINK_ID_BYTES)
                .and_then(|kv| String::from_utf8(kv[3..].to_vec()).ok());
            self.anchor_for(LinkTarget::Explicit { id, uri })
        } else {
            // there is no link, so consider it a link end
            Some(LinkAnchor::End)
        }
    }

    pub fn new_link_from_url(&mut self, url: String) -> Option<LinkAnchor> {
        self.anchor_for(LinkTarget::Detected(url))
    }

    /// Whether the anchor starts a link made by URL detection (as opposed to
    /// one an application requested with OSC 8).
    pub fn is_detected(&self, link_anchor: Option<LinkAnchor>) -> bool {
        match link_anchor {
            Some(LinkAnchor::Start(index)) => {
                self.links.get(&index).is_some_and(|link| link.detected)
            },
            _ => false,
        }
    }

    pub fn uri(&self, link_anchor: Option<LinkAnchor>) -> Option<&str> {
        match link_anchor {
            Some(LinkAnchor::Start(index)) => self.links.get(&index).map(|link| link.uri.as_str()),
            _ => None,
        }
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

    /// Bytes retained for link targets and the cap they are held to.
    #[cfg(test)]
    pub(crate) fn retained_bytes_and_cap(&self) -> (usize, usize) {
        (self.retained_bytes, MAX_LINK_BYTES_TOTAL)
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
    fn dispatch_osc8_ignores_short_parameter_lists() {
        let mut link_handler = LinkHandler::default();
        assert_eq!(link_handler.dispatch_osc8(&[]), None);
        assert_eq!(link_handler.dispatch_osc8(&[b"8"]), None);
        assert_eq!(link_handler.dispatch_osc8(&[b"8", b"id=test"]), None);
    }

    #[test]
    fn return_none_on_missing_link_id() {
        let link_handler = LinkHandler::default();
        let anchor = LinkAnchor::Start(100);
        assert_eq!(link_handler.output_osc8(Some(anchor)), None);
    }

    #[test]
    fn identical_targets_share_an_id_instead_of_consuming_new_ones() {
        let mut link_handler = LinkHandler::default();
        let osc8: Vec<&[u8]> = vec![b"8", b"id=a", b"https://example.com"];
        let first = link_handler.dispatch_osc8(&osc8);
        for _ in 0..100_000 {
            assert_eq!(link_handler.dispatch_osc8(&osc8), first);
        }
        let detected = link_handler.new_link_from_url("https://example.com".to_owned());
        for _ in 0..100_000 {
            assert_eq!(
                link_handler.new_link_from_url("https://example.com".to_owned()),
                detected
            );
        }
        // An explicit link with another id, or without one, is a different
        // target from both.
        let other_id: Vec<&[u8]> = vec![b"8", b"id=b", b"https://example.com"];
        let no_id: Vec<&[u8]> = vec![b"8", b"", b"https://example.com"];
        let distinct = [
            first,
            detected,
            link_handler.dispatch_osc8(&other_id),
            link_handler.dispatch_osc8(&no_id),
        ];
        for (i, a) in distinct.iter().enumerate() {
            assert!(a.is_some());
            for b in &distinct[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(link_handler.links.len(), 4);
    }

    #[test]
    fn retained_link_bytes_are_bounded() {
        let mut link_handler = LinkHandler::default();
        let too_long = format!("https://example.com/{}", "a".repeat(MAX_LINK_URI_BYTES));
        assert_eq!(link_handler.new_link_from_url(too_long), None);

        let long_path = "b".repeat(MAX_LINK_URI_BYTES - 64);
        let mut accepted = 0;
        while link_handler
            .new_link_from_url(format!("https://example.com/{accepted}/{long_path}"))
            .is_some()
        {
            accepted += 1;
        }
        assert!(link_handler.retained_bytes <= MAX_LINK_BYTES_TOTAL);
        assert!(accepted >= MAX_LINK_BYTES_TOTAL / (2 * MAX_LINK_URI_BYTES));
        assert!(accepted < usize::from(u16::MAX));
        // Earlier targets still resolve after the budget is reached.
        assert!(link_handler
            .uri(Some(LinkAnchor::Start(0)))
            .is_some_and(|uri| uri.starts_with("https://example.com/0/")));
    }

    #[test]
    fn generated_detected_link_ids_are_charged() {
        let mut link_handler = LinkHandler::default();
        let url = "https://example.com/a".to_owned();
        link_handler.new_link_from_url(url.clone()).unwrap();
        // Index 0 generates the id "0": two URI copies plus one id byte.
        assert_eq!(link_handler.retained_bytes, 2 * url.len() + 1);

        // Fill the budget exactly up to the point where only the generated
        // id of the next link would exceed it.
        let mut link_handler = LinkHandler {
            retained_bytes: MAX_LINK_BYTES_TOTAL - 2 * url.len(),
            ..Default::default()
        };
        assert_eq!(link_handler.new_link_from_url(url), None);
    }

    #[test]
    fn link_ids_are_not_reused_after_exhaustion() {
        let mut link_handler = LinkHandler {
            next_link_index: Some(u16::MAX),
            ..Default::default()
        };

        let first = link_handler.new_link_from_url("https://first.example".to_owned());
        assert_eq!(first, Some(LinkAnchor::Start(u16::MAX)));
        let first_uri = link_handler.uri(first).map(str::to_owned);

        assert_eq!(
            link_handler.new_link_from_url("https://second.example".to_owned()),
            None
        );
        assert_eq!(link_handler.uri(first).map(str::to_owned), first_uri);
    }
}
