//! Classified representation of the host-terminal queries Zellij
//! forwards on behalf of apps running inside panes.
//!
//! The grid dispatcher already parses incoming OSC / CSI sequences
//! via `vte`; rather than stashing the raw bytes and re-parsing them
//! at the reply-synthesis stage, we capture the classification at
//! intercept time into a [`HostQuery`]. The pipeline from Grid →
//! Tab → Screen carries this enum; Screen's cache-fallback synthesis
//! matches on the variants directly (no byte-level regex). When we
//! have to actually send bytes on the wire (server → client → host
//! terminal), [`HostQuery::to_query_bytes`] re-derives them from the
//! enum.

/// The two OSC string terminators in common use. Apps that phrase
/// their query with one expect the reply to mirror it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscTerminator {
    /// `ESC \` (String Terminator, xterm-style).
    St,
    /// `BEL` (`\x07`).
    Bel,
}

impl OscTerminator {
    pub fn from_bell_terminated(bell_terminated: bool) -> Self {
        if bell_terminated {
            OscTerminator::Bel
        } else {
            OscTerminator::St
        }
    }

    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            OscTerminator::St => b"\x1b\\",
            OscTerminator::Bel => b"\x07",
        }
    }
}

/// A whitelisted host-terminal query. Variants correspond one-to-one
/// with the entries Grid intercepts and pushes on
/// `pending_forwarded_queries`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostQuery {
    /// `CSI 14 t` — text-area pixel dimensions.
    TextAreaPixelSize,
    /// `CSI 16 t` — character-cell pixel dimensions.
    CharacterCellPixelSize,
    /// `OSC 10 ; ? <term>` — default foreground colour.
    DefaultForeground { terminator: OscTerminator },
    /// `OSC 11 ; ? <term>` — default background colour.
    DefaultBackground { terminator: OscTerminator },
    /// `OSC 4 ; i ; ? ; j ; ? <term>` — one or more palette registers.
    ///
    /// Indices that arrived in one OSC stay in one query, so the host's
    /// reply has the same shape the app asked for. Empty `indices` is
    /// not a query.
    PaletteRegister {
        indices: Vec<u8>,
        terminator: OscTerminator,
    },
    ClipboardContent {
        selection: char,
        terminator: OscTerminator,
    },
    /// Several queries that can share one Primary-DA barrier.
    ///
    /// The host sees each query's original bytes, in order, then one
    /// device-attributes query. It answers in that order, and the client
    /// returns every answer as one reply. Separate palette OSCs stay
    /// separate sequences inside this batch; only the round trip is shared.
    RoundTrip(Vec<HostQuery>),
    /// `CSI ? 996 n` — query the host terminal's color-palette theme
    /// mode (light or dark). NOT forwarded to the host. Zellij has
    /// already queried the host once at startup (via the client's
    /// `\e[?996n` write) and tracks unsolicited DSR 997 updates while
    /// `\e[?2031h` is enabled, so it can answer this from cache. The
    /// Screen handler short-circuits this variant: rather than writing
    /// to the wire it synthesises `\e[?997;{0|1|2}n` directly into the
    /// originating pane's pty.
    ColorPaletteMode,
}

impl HostQuery {
    /// Re-serialize this query as the byte sequence the host terminal
    /// expects. Used by the client when it writes the query to its
    /// stdout, and by tests that assert on wire shape.
    pub fn to_query_bytes(&self) -> Vec<u8> {
        match self {
            HostQuery::TextAreaPixelSize => b"\x1b[14t".to_vec(),
            HostQuery::CharacterCellPixelSize => b"\x1b[16t".to_vec(),
            HostQuery::DefaultForeground { terminator } => {
                let mut v = b"\x1b]10;?".to_vec();
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            HostQuery::DefaultBackground { terminator } => {
                let mut v = b"\x1b]11;?".to_vec();
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            HostQuery::PaletteRegister {
                indices,
                terminator,
            } => {
                if indices.is_empty() {
                    return Vec::new();
                }
                let mut v = b"\x1b]4".to_vec();
                for index in indices {
                    v.extend_from_slice(format!(";{index};?").as_bytes());
                }
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            HostQuery::ClipboardContent {
                selection,
                terminator,
            } => {
                let mut v = format!("\x1b]52;{};?", selection).into_bytes();
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            // `ColorPaletteMode` is answered locally by Zellij and never
            // sent on the wire. Returning empty bytes keeps the call
            // total without dirtying the wire format. Callers that
            // bypass `Screen::forward_host_query` for this variant
            // shouldn't be calling `to_query_bytes` on it at all.
            HostQuery::ColorPaletteMode => Vec::new(),
            HostQuery::RoundTrip(parts) => {
                let mut v = Vec::new();
                for part in parts {
                    v.extend(part.to_query_bytes());
                }
                v
            },
        }
    }

    /// Queries the client can collect under one Primary-DA barrier.
    /// Clipboard reads use their own slot. Color-palette mode is answered
    /// here and never goes on the wire.
    pub fn shares_da_barrier(&self) -> bool {
        match self {
            HostQuery::TextAreaPixelSize
            | HostQuery::CharacterCellPixelSize
            | HostQuery::DefaultForeground { .. }
            | HostQuery::DefaultBackground { .. }
            | HostQuery::PaletteRegister { .. } => true,
            HostQuery::RoundTrip(parts) => {
                !parts.is_empty() && parts.iter().all(HostQuery::shares_da_barrier)
            },
            HostQuery::ClipboardContent { .. } | HostQuery::ColorPaletteMode => false,
        }
    }

    /// Nothing to ask the host. An empty palette register list, or a round
    /// trip made only of those, must not occupy the forward slot.
    pub fn is_noop(&self) -> bool {
        match self {
            HostQuery::PaletteRegister { indices, .. } => indices.is_empty(),
            HostQuery::RoundTrip(parts) => parts.is_empty() || parts.iter().all(HostQuery::is_noop),
            _ => false,
        }
    }

    pub fn empty_reply_bytes(&self) -> Vec<u8> {
        match self {
            HostQuery::ClipboardContent {
                selection,
                terminator,
            } => {
                let mut v = format!("\x1b]52;{};", selection).into_bytes();
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            HostQuery::RoundTrip(parts) => {
                let mut v = Vec::new();
                for part in parts {
                    v.extend(part.empty_reply_bytes());
                }
                v
            },
            _ => Vec::new(),
        }
    }
}

/// Group queries that can share one Primary-DA barrier.
///
/// A run of palette, foreground, background, and pixel-size queries becomes
/// one [`HostQuery::RoundTrip`] when it holds more than one query. Each part
/// keeps the bytes the app sent. A clipboard read or a locally answered
/// query ends the run and stays on its own.
pub(crate) fn pack_shared_round_trip(queries: Vec<HostQuery>) -> Vec<HostQuery> {
    let mut packed = Vec::new();
    let mut batch = Vec::new();
    for query in queries {
        if query.is_noop() {
            continue;
        }
        if query.shares_da_barrier() {
            match query {
                HostQuery::RoundTrip(mut parts) => batch.append(&mut parts),
                other => batch.push(other),
            }
        } else {
            flush_round_trip(&mut batch, &mut packed);
            packed.push(query);
        }
    }
    flush_round_trip(&mut batch, &mut packed);
    packed
}

fn flush_round_trip(batch: &mut Vec<HostQuery>, packed: &mut Vec<HostQuery>) {
    match batch.len() {
        0 => {},
        1 => packed.push(batch.pop().unwrap()),
        _ => packed.push(HostQuery::RoundTrip(std::mem::take(batch))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_area_pixel_size_serializes_as_csi_14t() {
        assert_eq!(HostQuery::TextAreaPixelSize.to_query_bytes(), b"\x1b[14t");
    }

    #[test]
    fn character_cell_pixel_size_serializes_as_csi_16t() {
        assert_eq!(
            HostQuery::CharacterCellPixelSize.to_query_bytes(),
            b"\x1b[16t"
        );
    }

    #[test]
    fn default_foreground_mirrors_terminator() {
        assert_eq!(
            HostQuery::DefaultForeground {
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]10;?\x1b\\",
        );
        assert_eq!(
            HostQuery::DefaultForeground {
                terminator: OscTerminator::Bel,
            }
            .to_query_bytes(),
            b"\x1b]10;?\x07",
        );
    }

    #[test]
    fn default_background_serializes_correctly() {
        assert_eq!(
            HostQuery::DefaultBackground {
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]11;?\x1b\\",
        );
    }

    #[test]
    fn palette_register_carries_index_and_terminator() {
        assert_eq!(
            HostQuery::PaletteRegister {
                indices: vec![5],
                terminator: OscTerminator::Bel,
            }
            .to_query_bytes(),
            b"\x1b]4;5;?\x07",
        );
        assert_eq!(
            HostQuery::PaletteRegister {
                indices: vec![255],
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]4;255;?\x1b\\",
        );
    }

    #[test]
    fn palette_registers_in_one_osc_are_one_query() {
        assert_eq!(
            HostQuery::PaletteRegister {
                indices: vec![1, 2],
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]4;1;?;2;?\x1b\\",
        );
        assert!(HostQuery::PaletteRegister {
            indices: vec![],
            terminator: OscTerminator::St,
        }
        .to_query_bytes()
        .is_empty());
    }

    #[test]
    fn clipboard_content_carries_selection_and_terminator() {
        assert_eq!(
            HostQuery::ClipboardContent {
                selection: 'c',
                terminator: OscTerminator::Bel,
            }
            .to_query_bytes(),
            b"\x1b]52;c;?\x07",
        );
        assert_eq!(
            HostQuery::ClipboardContent {
                selection: 'p',
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]52;p;?\x1b\\",
        );
    }

    #[test]
    fn clipboard_content_empty_reply_mirrors_the_query_shape() {
        assert_eq!(
            HostQuery::ClipboardContent {
                selection: 'c',
                terminator: OscTerminator::Bel,
            }
            .empty_reply_bytes(),
            b"\x1b]52;c;\x07",
        );
        assert_eq!(
            HostQuery::ClipboardContent {
                selection: 'p',
                terminator: OscTerminator::St,
            }
            .empty_reply_bytes(),
            b"\x1b]52;p;\x1b\\",
        );
    }

    #[test]
    fn non_clipboard_queries_have_no_empty_reply() {
        assert!(HostQuery::TextAreaPixelSize.empty_reply_bytes().is_empty());
        assert!(HostQuery::DefaultBackground {
            terminator: OscTerminator::St,
        }
        .empty_reply_bytes()
        .is_empty());
    }

    #[test]
    fn from_bell_terminated_maps_bool_to_variant() {
        assert_eq!(
            OscTerminator::from_bell_terminated(true),
            OscTerminator::Bel
        );
        assert_eq!(
            OscTerminator::from_bell_terminated(false),
            OscTerminator::St
        );
    }

    #[test]
    fn separate_palette_queries_share_a_round_trip_without_merging_oscs() {
        let packed = pack_shared_round_trip(vec![
            HostQuery::PaletteRegister {
                indices: vec![1],
                terminator: OscTerminator::Bel,
            },
            HostQuery::DefaultForeground {
                terminator: OscTerminator::St,
            },
            HostQuery::PaletteRegister {
                indices: vec![2],
                terminator: OscTerminator::St,
            },
            HostQuery::ClipboardContent {
                selection: 'c',
                terminator: OscTerminator::Bel,
            },
        ]);
        assert_eq!(packed.len(), 2);
        match &packed[0] {
            HostQuery::RoundTrip(parts) => {
                assert_eq!(parts.len(), 3);
                assert_eq!(
                    packed[0].to_query_bytes(),
                    b"\x1b]4;1;?\x07\x1b]10;?\x1b\\\x1b]4;2;?\x1b\\"
                );
            },
            other => panic!("expected one round trip, got {other:?}"),
        }
        assert!(matches!(packed[1], HostQuery::ClipboardContent { .. }));
    }

    #[test]
    fn one_query_is_not_wrapped_in_a_round_trip() {
        let query = HostQuery::TextAreaPixelSize;
        assert_eq!(pack_shared_round_trip(vec![query.clone()]), vec![query]);
    }
}
