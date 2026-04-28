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
    /// `OSC 4 ; N ; ? <term>` — palette register `N`.
    PaletteRegister {
        index: u8,
        terminator: OscTerminator,
    },
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
            HostQuery::PaletteRegister { index, terminator } => {
                let mut v = format!("\x1b]4;{};?", index).into_bytes();
                v.extend_from_slice(terminator.as_bytes());
                v
            },
            // `ColorPaletteMode` is answered locally by Zellij and never
            // sent on the wire. Returning empty bytes keeps the call
            // total without dirtying the wire format. Callers that
            // bypass `Screen::forward_host_query` for this variant
            // shouldn't be calling `to_query_bytes` on it at all.
            HostQuery::ColorPaletteMode => Vec::new(),
        }
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
                index: 5,
                terminator: OscTerminator::Bel,
            }
            .to_query_bytes(),
            b"\x1b]4;5;?\x07",
        );
        assert_eq!(
            HostQuery::PaletteRegister {
                index: 255,
                terminator: OscTerminator::St,
            }
            .to_query_bytes(),
            b"\x1b]4;255;?\x1b\\",
        );
    }

    #[test]
    fn from_bell_terminated_maps_bool_to_variant() {
        assert_eq!(OscTerminator::from_bell_terminated(true), OscTerminator::Bel);
        assert_eq!(OscTerminator::from_bell_terminated(false), OscTerminator::St);
    }
}
