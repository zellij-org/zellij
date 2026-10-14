use ansi_term::ANSIString;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabMarker {
    pub symbol: &'static str,
    pub status: Option<AttentionStatus>,
    pub muted: bool,
}

impl TabMarker {
    pub fn for_tab(tab: &TabInfo, simplified_ui: bool) -> Option<TabMarker> {
        if tab.attention.muted {
            return Some(TabMarker {
                symbol: if simplified_ui { "-" } else { "⊘" },
                status: None,
                muted: true,
            });
        }
        let status = if tab.attention.status.is_counted() {
            tab.attention.status
        } else if tab.has_bell_notification || tab.is_flashing_bell {
            AttentionStatus::NeedsYou
        } else {
            return None;
        };
        Some(TabMarker {
            symbol: status.marker(simplified_ui),
            status: Some(status),
            muted: false,
        })
    }

    pub fn width(&self) -> usize {
        self.symbol.width() + 1
    }

    fn color(&self, palette: &Styling, fallback: PaletteColor) -> PaletteColor {
        match self.status {
            Some(AttentionStatus::NeedsYou) => palette.frame_highlight.emphasis_0,
            Some(AttentionStatus::Failed) => palette.exit_code_error.base,
            Some(AttentionStatus::Done) => palette.exit_code_success.base,
            Some(AttentionStatus::Working) => palette.text_unselected.emphasis_1,
            _ => fallback,
        }
    }

    pub fn paint(
        &self,
        palette: &Styling,
        fallback: PaletteColor,
        background: PaletteColor,
    ) -> ANSIString<'static> {
        let color = self.color(palette, fallback);
        let text = format!("{} ", self.symbol);
        if self.muted {
            style!(color, background).dimmed().paint(text)
        } else {
            style!(color, background).bold().paint(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(status: AttentionStatus, muted: bool, bell: bool) -> TabInfo {
        TabInfo {
            attention: TabAttention {
                status,
                muted,
                ..Default::default()
            },
            has_bell_notification: bell,
            ..Default::default()
        }
    }

    #[test]
    fn the_top_counted_status_is_shown() {
        let marker = TabMarker::for_tab(&tab(AttentionStatus::Failed, false, false), false);
        assert_eq!(marker.map(|m| m.symbol), Some("✗"));
        let marker = TabMarker::for_tab(&tab(AttentionStatus::Working, false, false), true);
        assert_eq!(marker.map(|m| m.symbol), Some(">"));
    }

    #[test]
    fn quiet_and_new_output_never_show_on_tabs() {
        assert_eq!(
            TabMarker::for_tab(&tab(AttentionStatus::NewOutput, false, false), false),
            None
        );
        assert_eq!(
            TabMarker::for_tab(&tab(AttentionStatus::Quiet, false, false), false),
            None
        );
        assert_eq!(
            TabMarker::for_tab(&tab(AttentionStatus::Idle, false, false), false),
            None
        );
    }

    #[test]
    fn muted_tabs_show_a_dim_mark() {
        let marker =
            TabMarker::for_tab(&tab(AttentionStatus::NeedsYou, true, false), false).unwrap();
        assert!(marker.muted);
        assert_eq!(marker.symbol, "⊘");
        let marker = TabMarker::for_tab(&tab(AttentionStatus::Idle, true, false), true).unwrap();
        assert_eq!(marker.symbol, "-");
    }

    #[test]
    fn a_bell_without_status_still_shows_needs_you() {
        let marker = TabMarker::for_tab(&tab(AttentionStatus::Idle, false, true), false);
        assert_eq!(marker.map(|m| m.symbol), Some("●"));
    }
}
