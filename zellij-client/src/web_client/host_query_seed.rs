//! Builds the IPC messages that seed Screen's host-terminal-query cache
//! with web-client data on attach (and on subsequent config reloads).
//!
//! The native client populates `Screen.terminal_emulator_colors`,
//! `Screen.pixel_dimensions` and `Screen.terminal_emulator_color_codes`
//! by forwarding what its real terminal reports. The web client has no
//! "real terminal" — its display surface is xterm.js in the browser —
//! so we synthesize the equivalent state from two sources:
//!
//! 1. The same `Config` that drives `SetConfigPayload`, which the
//!    browser ultimately renders with. fg/bg are guaranteed to match
//!    what xterm.js paints.
//! 2. The canonical xterm-256 palette formula for indices 16-255 (not
//!    overridable in xterm.js's configurable theme, so the formula is
//!    authoritative).
//!
//! Pixel dimensions are not handled here — they require browser-side
//! measurement and flow through the `TerminalMetrics` control message.

use zellij_utils::{
    data::PaletteColor,
    input::{config::Config, options::Options},
    ipc::{ClientToServerMsg, ColorRegister},
};

/// Build the seed messages to send to the server immediately after attach
/// (and on every `SetConfig` update). May return an empty vec if no theme
/// information is resolvable from the config — in which case the server's
/// cache stays at its defaults and `synthesize_cached_reply` returns empty
/// bytes, matching pre-existing behavior.
pub fn build_host_query_seed_msgs(
    config: &Config,
    config_options: &Options,
) -> Vec<ClientToServerMsg> {
    let mut msgs = Vec::new();
    let resolved = resolve_theme(config, config_options);

    if let Some(fg) = resolved
        .foreground
        .as_ref()
        .and_then(|s| css_rgb_to_xparse(s))
    {
        msgs.push(ClientToServerMsg::ForegroundColor { color: fg });
    }
    if let Some(bg) = resolved
        .background
        .as_ref()
        .and_then(|s| css_rgb_to_xparse(s))
    {
        msgs.push(ClientToServerMsg::BackgroundColor { color: bg });
    }

    let registers = build_color_registers(&resolved);
    if !registers.is_empty() {
        msgs.push(ClientToServerMsg::ColorRegisters {
            color_registers: registers,
        });
    }

    msgs
}

#[derive(Default, Debug, Clone)]
struct ResolvedTheme {
    foreground: Option<String>,
    background: Option<String>,
    indexed: [Option<String>; 16],
}

/// Resolve the web-client theme using the same precedence as
/// `SetConfigPayload::from(&Config)` so server-side synthesis matches
/// the colors xterm.js paints in the browser.
fn resolve_theme(config: &Config, config_options: &Options) -> ResolvedTheme {
    let mut out = ResolvedTheme::default();

    let palette = config.theme_config(config_options.theme.as_ref());
    let web_client_theme = config.web_client.theme.as_ref();

    out.foreground = web_client_theme
        .and_then(|t| t.foreground.clone())
        .or_else(|| palette.map(|p| p.text_unselected.base.as_rgb_str()));
    out.background = web_client_theme
        .and_then(|t| t.background.clone())
        .or_else(|| palette.map(|p| p.text_unselected.background.as_rgb_str()));

    if let Some(t) = web_client_theme {
        out.indexed[0] = t.black.clone();
        out.indexed[1] = t.red.clone();
        out.indexed[2] = t.green.clone();
        out.indexed[3] = t.yellow.clone();
        out.indexed[4] = t.blue.clone();
        out.indexed[5] = t.magenta.clone();
        out.indexed[6] = t.cyan.clone();
        out.indexed[7] = t.white.clone();
        out.indexed[8] = t.bright_black.clone();
        out.indexed[9] = t.bright_red.clone();
        out.indexed[10] = t.bright_green.clone();
        out.indexed[11] = t.bright_yellow.clone();
        out.indexed[12] = t.bright_blue.clone();
        out.indexed[13] = t.bright_magenta.clone();
        out.indexed[14] = t.bright_cyan.clone();
        out.indexed[15] = t.bright_white.clone();
    }

    out
}

fn build_color_registers(resolved: &ResolvedTheme) -> Vec<ColorRegister> {
    let mut registers = Vec::with_capacity(256);

    // Indices 0-15: only seed entries that have an explicit override in
    // `web_client.theme.*`. Unseeded indices fall through to empty
    // synthesis (existing semantics) — apps will use their own defaults.
    for (i, slot) in resolved.indexed.iter().enumerate() {
        if let Some(css) = slot {
            if let Some(color) = css_rgb_to_xparse_color(css) {
                registers.push(ColorRegister { index: i, color });
            }
        }
    }

    // Indices 16-255: canonical xterm-256 palette. xterm.js does not let
    // these be overridden via its theme options, and Zellij's config does
    // not expose them, so the formula is the source of truth.
    for index in 16u8..=255 {
        let (r, g, b) = xterm_256_rgb(index);
        registers.push(ColorRegister {
            index: index as usize,
            color: rgb_to_xparse_color(r, g, b),
        });
    }

    registers
}

/// Convert a CSS `rgb(R, G, B)` string (the format `PaletteColor::as_rgb_str`
/// produces, which is what `SetConfigPayload` ships to xterm.js) into the
/// `rgb:RRRR/GGGG/BBBB` form `xparse_color` accepts, prefixed for
/// `ClientToServerMsg::{Foreground,Background}Color`'s consumer.
fn css_rgb_to_xparse(css: &str) -> Option<String> {
    css_rgb_to_xparse_color(css)
}

/// Same as `css_rgb_to_xparse`. Kept as a separate alias to make the
/// difference between OSC 10/11 (xparse-format string going into screen's
/// terminal_emulator_colors) and OSC 4 (color string going into
/// terminal_emulator_color_codes) explicit at call sites — currently both
/// consumers want the same `rgb:RRRR/GGGG/BBBB` shape.
fn css_rgb_to_xparse_color(css: &str) -> Option<String> {
    let PaletteColor::Rgb((r, g, b)) = PaletteColor::from_rgb_str(css) else {
        return None;
    };
    Some(rgb_to_xparse_color(r, g, b))
}

fn rgb_to_xparse_color(r: u8, g: u8, b: u8) -> String {
    format!(
        "rgb:{:04x}/{:04x}/{:04x}",
        (r as u16) * 0x0101,
        (g as u16) * 0x0101,
        (b as u16) * 0x0101,
    )
}

/// Canonical xterm-256 palette for index 16..=255. Indices 0..=15 are
/// intentionally not handled here — those are theme-dependent and seeded
/// separately from `WebClientTheme` when overrides are present.
fn xterm_256_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => (0, 0, 0), // not used; caller restricts to 16..=255
        16..=231 => {
            let idx = (index - 16) as u32;
            let level = |x: u32| if x == 0 { 0u8 } else { (55 + x * 40) as u8 };
            (level(idx / 36), level((idx / 6) % 6), level(idx % 6))
        },
        232..=255 => {
            let v = 8u8 + 10 * (index - 232);
            (v, v, v)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::input::web_client::{WebClientConfig, WebClientTheme};

    fn config_with_web_theme(theme: WebClientTheme) -> Config {
        let mut config = Config::default();
        config.web_client = WebClientConfig {
            theme: Some(theme),
            ..WebClientConfig::default()
        };
        config
    }

    /// Locate the single `ColorRegister` matching `index` in the
    /// `ColorRegisters` message produced by the seed builder.
    fn find_register(msgs: &[ClientToServerMsg], index: usize) -> Option<String> {
        for msg in msgs {
            if let ClientToServerMsg::ColorRegisters { color_registers } = msg {
                if let Some(reg) = color_registers.iter().find(|r| r.index == index) {
                    return Some(reg.color.clone());
                }
            }
        }
        None
    }

    fn fg_color(msgs: &[ClientToServerMsg]) -> Option<String> {
        msgs.iter().find_map(|m| match m {
            ClientToServerMsg::ForegroundColor { color } => Some(color.clone()),
            _ => None,
        })
    }

    fn bg_color(msgs: &[ClientToServerMsg]) -> Option<String> {
        msgs.iter().find_map(|m| match m {
            ClientToServerMsg::BackgroundColor { color } => Some(color.clone()),
            _ => None,
        })
    }

    fn registers_msg(msgs: &[ClientToServerMsg]) -> Option<usize> {
        msgs.iter().find_map(|m| match m {
            ClientToServerMsg::ColorRegisters { color_registers } => Some(color_registers.len()),
            _ => None,
        })
    }

    #[test]
    fn xterm_256_first_cube_entry_is_black() {
        assert_eq!(xterm_256_rgb(16), (0, 0, 0));
    }

    #[test]
    fn xterm_256_last_cube_entry_is_white() {
        // 5,5,5 — corners at maximal level
        assert_eq!(xterm_256_rgb(231), (255, 255, 255));
    }

    #[test]
    fn xterm_256_greyscale_endpoints() {
        assert_eq!(xterm_256_rgb(232), (8, 8, 8));
        assert_eq!(xterm_256_rgb(255), (238, 238, 238));
    }

    #[test]
    fn cube_level_formula_matches_xterm_table() {
        // index 17: r=0, g=0, b=1  => (0,0,95)
        assert_eq!(xterm_256_rgb(17), (0, 0, 95));
        // index 21: r=0, g=0, b=5  => (0,0,255)
        assert_eq!(xterm_256_rgb(21), (0, 0, 255));
        // index 196: 196-16=180; r=180/36=5, g=(180/6)%6=0, b=180%6=0 => (255,0,0)
        assert_eq!(xterm_256_rgb(196), (255, 0, 0));
    }

    #[test]
    fn rgb_to_xparse_round_trip() {
        assert_eq!(rgb_to_xparse_color(0x12, 0x34, 0x56), "rgb:1212/3434/5656");
        assert_eq!(rgb_to_xparse_color(0, 0, 0), "rgb:0000/0000/0000");
        assert_eq!(rgb_to_xparse_color(255, 255, 255), "rgb:ffff/ffff/ffff");
    }

    #[test]
    fn css_rgb_str_parses_through_palette_color() {
        // PaletteColor::from_rgb_str accepts "rgb(R, G, B)".
        assert_eq!(
            css_rgb_to_xparse_color("rgb(255, 128, 0)").as_deref(),
            Some("rgb:ffff/8080/0000"),
        );
    }

    #[test]
    fn invalid_css_rgb_yields_no_conversion() {
        // PaletteColor::from_rgb_str returns the default variant for
        // strings it cannot parse; the bridge must reject those rather
        // than emit a garbage xparse string.
        assert!(css_rgb_to_xparse_color("#ff8800").is_none());
        assert!(css_rgb_to_xparse_color("not a color").is_none());
        assert!(css_rgb_to_xparse_color("").is_none());
    }

    #[test]
    fn seed_with_explicit_fg_bg_emits_both_messages() {
        // User has set web_client.theme.foreground and .background
        // explicitly — the seed builder must propagate them as
        // ForegroundColor / BackgroundColor messages in xparse format.
        let mut theme = WebClientTheme::default();
        theme.foreground = Some("rgb(200, 200, 200)".to_string());
        theme.background = Some("rgb(20, 20, 20)".to_string());
        let config = config_with_web_theme(theme);
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        assert_eq!(fg_color(&msgs).as_deref(), Some("rgb:c8c8/c8c8/c8c8"));
        assert_eq!(bg_color(&msgs).as_deref(), Some("rgb:1414/1414/1414"));
    }

    #[test]
    fn seed_with_indexed_overrides_populates_those_registers() {
        // Per-index overrides in WebClientTheme must show up in the
        // ColorRegisters payload at the correct ANSI index.
        let mut theme = WebClientTheme::default();
        theme.red = Some("rgb(204, 0, 0)".to_string()); // index 1
        theme.bright_blue = Some("rgb(50, 100, 200)".to_string()); // index 12
        let config = config_with_web_theme(theme);
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        assert_eq!(
            find_register(&msgs, 1).as_deref(),
            Some("rgb:cccc/0000/0000"),
        );
        assert_eq!(
            find_register(&msgs, 12).as_deref(),
            Some("rgb:3232/6464/c8c8"),
        );
    }

    #[test]
    fn seed_omits_unset_low_indices() {
        // Indices 0-15 without explicit overrides should be absent from
        // the ColorRegisters payload — apps will fall back to their own
        // defaults rather than reading a wrong value we made up.
        let mut theme = WebClientTheme::default();
        theme.red = Some("rgb(255, 0, 0)".to_string());
        let config = config_with_web_theme(theme);
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        // Only `red` (index 1) was set.
        assert_eq!(find_register(&msgs, 0), None);
        assert_eq!(find_register(&msgs, 2), None);
        assert!(find_register(&msgs, 1).is_some());
    }

    #[test]
    fn seed_always_populates_extended_palette_16_to_255() {
        // The 16-255 range is the canonical xterm-256 palette, which is
        // independent of theme config. Verify both the count (240
        // entries) and a couple of spot-check values.
        let config = Config::default();
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        // Default config has no per-index overrides, so the ColorRegisters
        // payload contains exactly 240 entries (16..=255).
        assert_eq!(registers_msg(&msgs), Some(240));

        // Index 196 is canonical pure red in the 6x6x6 cube.
        assert_eq!(
            find_register(&msgs, 196).as_deref(),
            Some("rgb:ffff/0000/0000"),
        );
        // Index 232 is the first greyscale step (rgb 8/8/8).
        assert_eq!(
            find_register(&msgs, 232).as_deref(),
            Some("rgb:0808/0808/0808"),
        );
        // Index 255 is the last greyscale step (rgb 238/238/238).
        assert_eq!(
            find_register(&msgs, 255).as_deref(),
            Some("rgb:eeee/eeee/eeee"),
        );
    }

    #[test]
    fn seed_with_overrides_sums_to_240_plus_overrides() {
        // Adding per-index overrides should grow the ColorRegisters
        // payload — 240 for the extended range plus one entry per
        // override in 0..=15.
        let mut theme = WebClientTheme::default();
        theme.red = Some("rgb(255, 0, 0)".to_string());
        theme.green = Some("rgb(0, 255, 0)".to_string());
        theme.blue = Some("rgb(0, 0, 255)".to_string());
        let config = config_with_web_theme(theme);
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        assert_eq!(registers_msg(&msgs), Some(240 + 3));
    }

    #[test]
    fn seed_skips_color_messages_when_no_theme_resolvable() {
        // A Config whose theme resolution yields nothing (no
        // web_client.theme, no main theme matching) should produce no
        // ForegroundColor / BackgroundColor messages — the server's
        // cache keeps its defaults instead of being clobbered.
        let mut config = Config::default();
        // Force theme resolution to fail by clearing the theme name as
        // well. The exact internals depend on Config::theme_config(),
        // so this test just asserts: whatever the resolver decides,
        // the seed builder must not emit "Some(empty string)" garbage.
        config.web_client = WebClientConfig::default();
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        // Whatever fg/bg the resolver picks, the bridge must produce
        // a well-formed xparse string or skip the message entirely.
        for msg in &msgs {
            match msg {
                ClientToServerMsg::ForegroundColor { color }
                | ClientToServerMsg::BackgroundColor { color } => {
                    assert!(
                        color.starts_with("rgb:"),
                        "expected xparse-format string, got: {:?}",
                        color
                    );
                },
                _ => {},
            }
        }
    }

    #[test]
    fn seed_color_registers_always_emit_xparse_format() {
        // Every entry in the ColorRegisters payload must use the same
        // `rgb:RRRR/GGGG/BBBB` shape that synthesize_cached_reply
        // re-emits verbatim — no `rgb(R, G, B)` CSS strings leaking
        // through.
        let mut theme = WebClientTheme::default();
        theme.black = Some("rgb(0, 0, 0)".to_string());
        theme.bright_white = Some("rgb(255, 255, 255)".to_string());
        let config = config_with_web_theme(theme);
        let opts = Options::default();

        let msgs = build_host_query_seed_msgs(&config, &opts);

        let registers = msgs
            .iter()
            .find_map(|m| match m {
                ClientToServerMsg::ColorRegisters { color_registers } => Some(color_registers),
                _ => None,
            })
            .expect("ColorRegisters message missing");
        for reg in registers {
            assert!(
                reg.color.starts_with("rgb:") && reg.color.len() == 4 + 4 + 1 + 4 + 1 + 4,
                "register {} has malformed color string: {:?}",
                reg.index,
                reg.color
            );
        }
    }
}
