use zellij_utils::data::{BareKey, HostTerminalThemeMode, KeyWithModifier, PaletteColor, Styling};
use zellij_utils::input::theme::Theme;
use zellij_utils::input::window::{
    BellMode, CursorStyle, NotificationMode, StartupMode, WindowTheme,
};

use crate::color::{Paints, Srgb};
use crate::font::{FontOptions, DEFAULT_FONT_SIZE, DEFAULT_LIGATURES};
use crate::platform::Platform;
use crate::screen_buffer::CursorShape;
use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub font: FontOptions,
    pub paste_keys: Vec<KeyWithModifier>,
    pub zoom_in_keys: Vec<KeyWithModifier>,
    pub zoom_out_keys: Vec<KeyWithModifier>,
    pub zoom_reset_keys: Vec<KeyWithModifier>,
    pub middle_click_paste: bool,
    pub open_links: bool,
    pub bell: BellMode,
    pub notifications: NotificationMode,
    pub paints: Paints,
    pub cursor_shape: Option<CursorShape>,
    pub cursor_blink: Option<bool>,
    pub startup_mode: StartupMode,
}

impl Default for Options {
    fn default() -> Self {
        resolve(&Settings::default(), None)
    }
}

pub fn resolve(settings: &Settings, mode: Option<HostTerminalThemeMode>) -> Options {
    let section = &settings.section;
    Options {
        font: FontOptions {
            family: section
                .font
                .clone()
                .filter(|family| !family.trim().is_empty()),
            size: section.font_size.unwrap_or(DEFAULT_FONT_SIZE),
            system_fonts: section.system_fonts.unwrap_or(true),
            ligatures: section.ligatures.unwrap_or(DEFAULT_LIGATURES),
        },
        paste_keys: section
            .paste_keys
            .clone()
            .unwrap_or_else(default_paste_keys),
        zoom_in_keys: section
            .zoom_in_keys
            .clone()
            .unwrap_or_else(default_zoom_in_keys),
        zoom_out_keys: section
            .zoom_out_keys
            .clone()
            .unwrap_or_else(default_zoom_out_keys),
        zoom_reset_keys: section
            .zoom_reset_keys
            .clone()
            .unwrap_or_else(default_zoom_reset_keys),
        middle_click_paste: section.middle_click_paste.unwrap_or(true),
        open_links: section.open_links.unwrap_or(true),
        bell: section.bell.unwrap_or(BellMode::Visual),
        notifications: section.notifications.unwrap_or(NotificationMode::Desktop),
        paints: paints(section.theme.as_ref(), settings.theme(mode).as_ref()),
        cursor_shape: section.cursor_style.map(cursor_shape),
        cursor_blink: section.cursor_blink,
        startup_mode: section.startup_mode.unwrap_or_default(),
    }
}

fn cursor_shape(style: CursorStyle) -> CursorShape {
    match style {
        CursorStyle::Block => CursorShape::Block,
        CursorStyle::Bar => CursorShape::Beam,
        CursorStyle::Underline => CursorShape::Underline,
    }
}

pub fn default_paste_keys() -> Vec<KeyWithModifier> {
    paste_keys_for(Platform::current())
}

pub fn default_zoom_in_keys() -> Vec<KeyWithModifier> {
    zoom_in_keys_for(Platform::current())
}

pub fn default_zoom_out_keys() -> Vec<KeyWithModifier> {
    zoom_out_keys_for(Platform::current())
}

pub fn default_zoom_reset_keys() -> Vec<KeyWithModifier> {
    zoom_reset_keys_for(Platform::current())
}

fn command(platform: Platform, keys: &[BareKey]) -> Vec<KeyWithModifier> {
    if platform != Platform::MacOs {
        return Vec::new();
    }
    keys.iter()
        .map(|key| KeyWithModifier::new(key.clone()).with_super_modifier())
        .collect()
}

fn paste_keys_for(platform: Platform) -> Vec<KeyWithModifier> {
    let mut keys = vec![
        KeyWithModifier::new(BareKey::Char('v'))
            .with_ctrl_modifier()
            .with_shift_modifier(),
        KeyWithModifier::new(BareKey::Insert).with_shift_modifier(),
    ];
    keys.extend(command(platform, &[BareKey::Char('v')]));
    keys
}

fn zoom_in_keys_for(platform: Platform) -> Vec<KeyWithModifier> {
    let mut keys = vec![
        KeyWithModifier::new(BareKey::Char('=')).with_ctrl_modifier(),
        KeyWithModifier::new(BareKey::Char('+')).with_ctrl_modifier(),
        KeyWithModifier::new(BareKey::Char('+'))
            .with_ctrl_modifier()
            .with_shift_modifier(),
    ];
    keys.extend(command(platform, &[BareKey::Char('='), BareKey::Char('+')]));
    keys.extend(
        command(platform, &[BareKey::Char('+')])
            .into_iter()
            .map(KeyWithModifier::with_shift_modifier),
    );
    keys
}

fn zoom_out_keys_for(platform: Platform) -> Vec<KeyWithModifier> {
    let mut keys = vec![
        KeyWithModifier::new(BareKey::Char('-')).with_ctrl_modifier(),
        KeyWithModifier::new(BareKey::Char('_'))
            .with_ctrl_modifier()
            .with_shift_modifier(),
    ];
    keys.extend(command(platform, &[BareKey::Char('-')]));
    keys
}

fn zoom_reset_keys_for(platform: Platform) -> Vec<KeyWithModifier> {
    let mut keys = vec![KeyWithModifier::new(BareKey::Char('0')).with_ctrl_modifier()];
    keys.extend(command(platform, &[BareKey::Char('0')]));
    keys
}

fn paints(section: Option<&WindowTheme>, theme: Option<&Theme>) -> Paints {
    let mut paints = Paints::default();

    if let Some(table) = theme.and_then(|theme| theme.terminal_colors) {
        for (slot, color) in table.iter().enumerate() {
            if let Some(color) = color {
                paints.ansi[slot] = srgb(color, &paints);
            }
        }
    }
    if let Some(section) = section {
        for (slot, color) in section.ansi.iter().enumerate() {
            if let Some(color) = color {
                paints.ansi[slot] = srgb(*color, &paints);
            }
        }
    }

    let table = paints;
    let configured = |pick: fn(&WindowTheme) -> Option<PaletteColor>| {
        section.and_then(pick).map(|color| srgb(color, &table))
    };
    let derived =
        |pick: fn(&Styling) -> PaletteColor| theme.map(|t| srgb(pick(&t.palette), &table));

    paints.foreground = configured(|section| section.foreground)
        .or_else(|| derived(|theme| theme.text_unselected.base))
        .unwrap_or(table.foreground);
    paints.background = configured(|section| section.background)
        .or_else(|| derived(|theme| theme.text_unselected.background))
        .unwrap_or(table.background);
    paints.cursor = configured(|section| section.cursor);

    paints
}

fn srgb(color: PaletteColor, paints: &Paints) -> Srgb {
    match color {
        PaletteColor::Rgb((r, g, b)) => [r, g, b],
        PaletteColor::EightBit(index) => paints.indexed(index),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    pub fonts: bool,
    pub paints: bool,
    pub cursor_shape: bool,
    pub cursor_blink: bool,
    pub paste_keys: bool,
    pub zoom_keys: bool,
    pub middle_click_paste: bool,
    pub open_links: bool,
    pub bell: bool,
    pub notifications: bool,
}

impl Change {
    pub fn between(current: &Options, next: &Options) -> Self {
        Self {
            fonts: current.font != next.font,
            paints: current.paints != next.paints,
            cursor_shape: current.cursor_shape != next.cursor_shape,
            cursor_blink: current.cursor_blink != next.cursor_blink,
            paste_keys: current.paste_keys != next.paste_keys,
            zoom_keys: current.zoom_in_keys != next.zoom_in_keys
                || current.zoom_out_keys != next.zoom_out_keys
                || current.zoom_reset_keys != next.zoom_reset_keys,
            middle_click_paste: current.middle_click_paste != next.middle_click_paste,
            open_links: current.open_links != next.open_links,
            bell: current.bell != next.bell,
            notifications: current.notifications != next.notifications,
        }
    }

    pub fn is_anything(&self) -> bool {
        self.fonts
            || self.paints
            || self.cursor_shape
            || self.cursor_blink
            || self.paste_keys
            || self.zoom_keys
            || self.middle_click_paste
            || self.open_links
            || self.bell
            || self.notifications
    }

    pub fn needs_redraw(&self) -> bool {
        self.paints || self.cursor_shape || self.cursor_blink
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use zellij_utils::data::{KeyModifier, StyleDeclaration, DEFAULT_STYLES};
    use zellij_utils::input::theme::TerminalColors;
    use zellij_utils::input::window::WindowConfig;

    use crate::color::{ANSI_16, DEFAULT_BACKGROUND, DEFAULT_FOREGROUND};

    fn cmd(key: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(key)).with_super_modifier()
    }

    #[test]
    fn linux_and_windows_defaults_have_no_command_key_bindings() {
        for platform in [Platform::Linux, Platform::Windows] {
            for keys in [
                paste_keys_for(platform),
                zoom_in_keys_for(platform),
                zoom_out_keys_for(platform),
                zoom_reset_keys_for(platform),
            ] {
                assert!(
                    keys.iter()
                        .all(|key| !key.key_modifiers.contains(&KeyModifier::Super)),
                    "{:?}: {:?}",
                    platform,
                    keys
                );
            }
        }
        assert_eq!(paste_keys_for(Platform::Linux).len(), 2);
        assert_eq!(zoom_in_keys_for(Platform::Linux).len(), 3);
        assert_eq!(zoom_out_keys_for(Platform::Linux).len(), 2);
        assert_eq!(zoom_reset_keys_for(Platform::Linux).len(), 1);
    }

    #[test]
    fn macos_adds_the_command_key_shortcuts_after_the_control_ones() {
        let mac = Platform::MacOs;
        let linux = Platform::Linux;
        assert_eq!(paste_keys_for(mac)[..2], paste_keys_for(linux)[..]);
        assert_eq!(paste_keys_for(mac)[2..], [cmd('v')]);
        assert_eq!(
            zoom_in_keys_for(mac)[3..],
            [cmd('='), cmd('+'), cmd('+').with_shift_modifier()]
        );
        assert_eq!(zoom_out_keys_for(mac)[2..], [cmd('-')]);
        assert_eq!(zoom_reset_keys_for(mac)[1..], [cmd('0')]);
    }

    #[test]
    fn the_defaults_are_those_of_the_platform_being_built_for() {
        assert_eq!(default_paste_keys(), paste_keys_for(Platform::current()));
        assert_eq!(
            default_zoom_in_keys(),
            zoom_in_keys_for(Platform::current())
        );
    }

    fn settings(section: WindowConfig) -> Settings {
        Settings {
            section,
            ..Settings::default()
        }
    }

    fn styling(base: PaletteColor, background: PaletteColor) -> Styling {
        Styling {
            text_unselected: StyleDeclaration {
                base,
                background,
                ..DEFAULT_STYLES.text_unselected
            },
            ..DEFAULT_STYLES
        }
    }

    fn theme(palette: Styling) -> Theme {
        Theme {
            sourced_from_external_file: false,
            palette,
            terminal_colors: None,
        }
    }

    fn themed(base: PaletteColor, background: PaletteColor) -> Settings {
        Settings {
            theme: Some(theme(styling(base, background))),
            ..Settings::default()
        }
    }

    fn tabled(base: PaletteColor, background: PaletteColor, table: TerminalColors) -> Settings {
        Settings {
            theme: Some(Theme {
                terminal_colors: Some(table),
                ..theme(styling(base, background))
            }),
            ..Settings::default()
        }
    }

    fn table(entries: &[(usize, PaletteColor)]) -> TerminalColors {
        let mut table = TerminalColors::default();
        for (slot, color) in entries {
            table.set(*slot, *color);
        }
        table
    }

    #[test]
    fn no_configuration_and_no_flags_reproduce_the_shipped_behaviour() {
        let options = resolve(&Settings::default(), None);
        assert_eq!(options.font, FontOptions::default());
        assert_eq!(options.font.size, DEFAULT_FONT_SIZE);
        assert!(options.font.system_fonts);
        assert_eq!(options.font.family, None);
        assert_eq!(options.paste_keys, default_paste_keys());
        assert_eq!(options.zoom_in_keys, default_zoom_in_keys());
        assert_eq!(options.zoom_out_keys, default_zoom_out_keys());
        assert_eq!(options.zoom_reset_keys, default_zoom_reset_keys());
        assert!(
            options.middle_click_paste,
            "a middle click must reach the primary selection unless it is turned off"
        );
        assert_eq!(
            options.bell,
            BellMode::Visual,
            "the shipped bell must stay the window attention the window always requested"
        );
        assert_eq!(
            options.notifications,
            NotificationMode::Desktop,
            "a notification a pane sends is meant for the desktop; \
             with no service to hand it to, it falls back to the window attention on its own"
        );
        assert_eq!(options.paints, Paints::default());
        assert_eq!(
            options.cursor_shape, None,
            "with nothing configured the shape the session asks for is the shape that is drawn"
        );
        assert_eq!(
            options.cursor_blink, None,
            "with nothing configured the session decides whether the cursor blinks"
        );
        assert_eq!(options.startup_mode, StartupMode::Windowed);
    }

    #[test]
    fn the_configuration_supersedes_the_defaults() {
        let options = resolve(
            &settings(WindowConfig {
                font: Some("Configured".to_owned()),
                font_size: Some(20.0),
                system_fonts: Some(false),
                paste_keys: Some(vec![KeyWithModifier::new(BareKey::F(5))]),
                startup_mode: Some(StartupMode::Fullscreen),
                ..WindowConfig::default()
            }),
            None,
        );
        assert_eq!(options.startup_mode, StartupMode::Fullscreen);
        assert_eq!(options.font.family.as_deref(), Some("Configured"));
        assert_eq!(options.font.size, 20.0);
        assert!(!options.font.system_fonts);
        assert_eq!(
            options.paste_keys,
            vec![KeyWithModifier::new(BareKey::F(5))]
        );
    }

    #[test]
    fn an_empty_family_is_no_family_at_all() {
        let options = resolve(
            &settings(WindowConfig {
                font: Some("   ".to_owned()),
                ..WindowConfig::default()
            }),
            None,
        );
        assert_eq!(options.font.family, None);
    }

    #[test]
    fn the_session_theme_supplies_the_default_foreground_and_background() {
        let options = resolve(
            &themed(PaletteColor::Rgb((1, 2, 3)), PaletteColor::Rgb((4, 5, 6))),
            None,
        );
        assert_eq!(options.paints.foreground, [1, 2, 3]);
        assert_eq!(options.paints.background, [4, 5, 6]);
        assert_eq!(
            options.paints.cursor, None,
            "an unnamed cursor stays whatever the cell under it is"
        );
        assert_eq!(
            options.paints.ansi, ANSI_16,
            "a zellij theme carries no ansi table and must not disturb it"
        );
    }

    #[test]
    fn a_theme_index_resolves_through_the_table_the_window_paints_with() {
        let mut settings = themed(PaletteColor::EightBit(1), PaletteColor::EightBit(0));
        settings.section.theme = Some(WindowTheme {
            ansi: {
                let mut ansi = [None; 16];
                ansi[1] = Some(PaletteColor::Rgb((9, 9, 9)));
                ansi
            },
            ..WindowTheme::default()
        });
        let options = resolve(&settings, None);
        assert_eq!(options.paints.ansi[1], [9, 9, 9]);
        assert_eq!(
            options.paints.foreground,
            [9, 9, 9],
            "the configured table must be in place before a theme index is read through it"
        );
        assert_eq!(options.paints.background, ANSI_16[0]);
    }

    #[test]
    fn the_window_section_supersedes_the_session_theme() {
        let mut settings = themed(PaletteColor::Rgb((1, 2, 3)), PaletteColor::Rgb((4, 5, 6)));
        settings.section.theme = Some(WindowTheme {
            foreground: Some(PaletteColor::Rgb((7, 7, 7))),
            cursor: Some(PaletteColor::Rgb((8, 8, 8))),
            ..WindowTheme::default()
        });
        let options = resolve(&settings, None);
        assert_eq!(options.paints.foreground, [7, 7, 7]);
        assert_eq!(
            options.paints.background,
            [4, 5, 6],
            "a slot the section does not name still comes from the theme"
        );
        assert_eq!(options.paints.cursor, Some([8, 8, 8]));
    }

    #[test]
    fn a_themes_terminal_colors_become_the_table_the_window_paints_with() {
        let options = resolve(
            &tabled(
                PaletteColor::Rgb((1, 2, 3)),
                PaletteColor::Rgb((4, 5, 6)),
                table(&[
                    (1, PaletteColor::Rgb((9, 9, 9))),
                    (12, PaletteColor::Rgb((8, 8, 8))),
                ]),
            ),
            None,
        );
        assert_eq!(options.paints.ansi[1], [9, 9, 9]);
        assert_eq!(options.paints.ansi[12], [8, 8, 8]);
        for slot in [0, 2, 15] {
            assert_eq!(
                options.paints.ansi[slot], ANSI_16[slot],
                "a slot the theme leaves undeclared must keep the xterm entry"
            );
        }
    }

    #[test]
    fn the_window_section_supersedes_the_themes_terminal_colors_entry_by_entry() {
        let mut settings = tabled(
            PaletteColor::Rgb((1, 2, 3)),
            PaletteColor::Rgb((4, 5, 6)),
            table(&[
                (1, PaletteColor::Rgb((9, 9, 9))),
                (2, PaletteColor::Rgb((7, 7, 7))),
            ]),
        );
        settings.section.theme = Some(WindowTheme {
            ansi: {
                let mut ansi = [None; 16];
                ansi[1] = Some(PaletteColor::Rgb((1, 1, 1)));
                ansi
            },
            ..WindowTheme::default()
        });
        let options = resolve(&settings, None);
        assert_eq!(options.paints.ansi[1], [1, 1, 1], "the override must win");
        assert_eq!(
            options.paints.ansi[2],
            [7, 7, 7],
            "an entry the section does not name still comes from the theme"
        );
        assert_eq!(options.paints.ansi[0], ANSI_16[0]);
    }

    #[test]
    fn a_theme_index_resolves_through_the_themes_own_table() {
        let settings = tabled(
            PaletteColor::EightBit(1),
            PaletteColor::EightBit(0),
            table(&[(1, PaletteColor::Rgb((9, 9, 9)))]),
        );
        let options = resolve(&settings, None);
        assert_eq!(
            options.paints.foreground,
            [9, 9, 9],
            "the theme's table must be in place before its own index is read through it"
        );
        assert_eq!(options.paints.background, ANSI_16[0]);
    }

    #[test]
    fn a_light_dark_switch_carries_the_terminal_colors_of_each_theme() {
        let dark = tabled(
            PaletteColor::Rgb((1, 1, 1)),
            PaletteColor::Rgb((1, 1, 1)),
            table(&[(1, PaletteColor::Rgb((11, 11, 11)))]),
        );
        let light = tabled(
            PaletteColor::Rgb((2, 2, 2)),
            PaletteColor::Rgb((2, 2, 2)),
            table(&[(1, PaletteColor::Rgb((22, 22, 22)))]),
        );
        let settings = Settings {
            theme_dark: dark.theme,
            theme_light: light.theme,
            ..Settings::default()
        };
        let painted = |mode| resolve(&settings, Some(mode)).paints;
        assert_eq!(painted(HostTerminalThemeMode::Dark).ansi[1], [11, 11, 11]);
        assert_eq!(painted(HostTerminalThemeMode::Light).ansi[1], [22, 22, 22]);
    }

    #[test]
    fn a_theme_with_no_terminal_colors_leaves_the_xterm_entries_alone() {
        let options = resolve(
            &themed(PaletteColor::Rgb((1, 2, 3)), PaletteColor::Rgb((4, 5, 6))),
            None,
        );
        assert_eq!(options.paints.ansi, ANSI_16);
    }

    #[test]
    fn a_configuration_with_no_theme_anywhere_paints_what_it_always_did() {
        let options = resolve(
            &settings(WindowConfig {
                font_size: Some(20.0),
                ..WindowConfig::default()
            }),
            None,
        );
        assert_eq!(options.paints.foreground, DEFAULT_FOREGROUND);
        assert_eq!(options.paints.background, DEFAULT_BACKGROUND);
        assert_eq!(options.paints.ansi, ANSI_16);
    }

    #[test]
    fn a_configured_cursor_style_becomes_the_shape_the_window_draws() {
        for (style, expected) in [
            (CursorStyle::Block, CursorShape::Block),
            (CursorStyle::Bar, CursorShape::Beam),
            (CursorStyle::Underline, CursorShape::Underline),
        ] {
            let options = resolve(
                &settings(WindowConfig {
                    cursor_style: Some(style),
                    ..WindowConfig::default()
                }),
                None,
            );
            assert_eq!(options.cursor_shape, Some(expected), "{:?}", style);
        }
    }

    #[test]
    fn a_configured_blink_preference_is_carried_as_given() {
        for blink in [None, Some(true), Some(false)] {
            let options = resolve(
                &settings(WindowConfig {
                    cursor_blink: blink,
                    ..WindowConfig::default()
                }),
                None,
            );
            assert_eq!(options.cursor_blink, blink, "{:?}", blink);
        }
    }

    #[test]
    fn the_zoom_chords_are_bound_by_default_and_cover_the_shifted_plus() {
        let options = Options::default();
        let bound = |chord: &str| {
            let key = KeyWithModifier::from_str(chord).expect("the chord must parse");
            options.zoom_in_keys.contains(&key)
                || options.zoom_out_keys.contains(&key)
                || options.zoom_reset_keys.contains(&key)
        };
        for chord in ["Ctrl =", "Ctrl +", "Ctrl Shift +", "Ctrl -", "Ctrl 0"] {
            assert!(bound(chord), "{} is not bound by default", chord);
        }
        assert!(!bound("Ctrl 9"));
        assert!(!bound("Ctrl Shift v"));
    }

    #[test]
    fn listing_no_zoom_chords_unbinds_them() {
        let options = resolve(
            &settings(WindowConfig {
                zoom_in_keys: Some(Vec::new()),
                zoom_out_keys: Some(Vec::new()),
                zoom_reset_keys: Some(Vec::new()),
                ..WindowConfig::default()
            }),
            None,
        );
        assert!(options.zoom_in_keys.is_empty());
        assert!(options.zoom_out_keys.is_empty());
        assert!(options.zoom_reset_keys.is_empty());
    }

    #[test]
    fn the_bell_and_the_middle_click_follow_the_configuration() {
        let options = resolve(
            &settings(WindowConfig {
                bell: Some(BellMode::None),
                middle_click_paste: Some(false),
                ..WindowConfig::default()
            }),
            None,
        );
        assert_eq!(options.bell, BellMode::None);
        assert!(!options.middle_click_paste);
    }

    #[test]
    fn a_cursor_change_asks_for_a_redraw_and_nothing_else() {
        let current = Options::default();

        let mut next = current.clone();
        next.cursor_shape = Some(CursorShape::Beam);
        let change = Change::between(&current, &next);
        assert!(change.needs_redraw() && change.is_anything());
        assert!(!change.fonts && !change.paints && !change.paste_keys);

        let mut next = current.clone();
        next.cursor_blink = Some(true);
        let change = Change::between(&current, &next);
        assert!(change.needs_redraw() && change.is_anything());
        assert!(!change.fonts && !change.paints && !change.paste_keys);

        let mut next = current.clone();
        next.paste_keys.clear();
        assert!(
            !Change::between(&current, &next).needs_redraw(),
            "a chord the window does not draw asked for a redraw"
        );
    }

    #[test]
    fn a_reload_reports_only_what_moved() {
        let current = Options::default();
        assert_eq!(Change::between(&current, &current), Change::default());
        assert!(!Change::between(&current, &current).is_anything());

        let mut next = current.clone();
        next.font.size += 1.0;
        assert_eq!(
            Change::between(&current, &next),
            Change {
                fonts: true,
                ..Change::default()
            }
        );

        let mut next = current.clone();
        next.paints.background = [1, 1, 1];
        assert!(Change::between(&current, &next).paints);
        assert!(!Change::between(&current, &next).fonts);

        let mut next = current.clone();
        next.paste_keys.clear();
        assert!(Change::between(&current, &next).paste_keys);
    }
}
