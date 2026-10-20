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

    fn is_urgent(&self) -> bool {
        matches!(
            self.status,
            Some(AttentionStatus::NeedsYou) | Some(AttentionStatus::Failed)
        )
    }

    fn style(&self, color: PaletteColor, background: PaletteColor) -> ansi_term::Style {
        let base = style!(color, background);
        if self.muted {
            base.dimmed()
        } else if self.is_urgent() {
            base.bold()
        } else {
            base
        }
    }

    pub fn paint(&self, color: PaletteColor, background: PaletteColor) -> ANSIString<'static> {
        self.style(color, background)
            .paint(format!("{} ", self.symbol))
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
    fn marks_use_the_tab_text_colour_and_only_urgent_ones_are_bold() {
        let text = PaletteColor::Rgb((11, 11, 11));
        let background = PaletteColor::Rgb((150, 220, 90));
        let expected = style!(text, background);
        let cases = [
            (AttentionStatus::NeedsYou, false, expected.bold()),
            (AttentionStatus::Failed, false, expected.bold()),
            (AttentionStatus::Done, false, expected),
            (AttentionStatus::Working, false, expected),
            (AttentionStatus::NeedsYou, true, expected.dimmed()),
        ];
        for (status, muted, style) in cases {
            let marker = TabMarker::for_tab(&tab(status, muted, false), false).unwrap();
            assert_eq!(marker.style(text, background), style, "{:?}", status);
        }
    }

    #[test]
    fn a_bell_without_status_still_shows_needs_you() {
        let marker = TabMarker::for_tab(&tab(AttentionStatus::Idle, false, true), false);
        assert_eq!(marker.map(|m| m.symbol), Some("●"));
    }
}
