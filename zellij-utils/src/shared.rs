//! Some general utility functions.

use std::net::{IpAddr, Ipv4Addr};
use std::{iter, str::from_utf8};

use crate::data::{Palette, PaletteColor, PaletteSource, ThemeHue};
use crate::envs::get_session_name;
use crate::errors::prelude::*;
use crate::input::options::Options;
use colorsys::{Ansi256, Rgb};
use strip_ansi_escapes::strip;
use unicode_width::UnicodeWidthStr;

#[cfg(unix)]
pub use unix_only::*;

#[cfg(unix)]
mod unix_only {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::{fs, io};

    pub fn set_permissions(path: &Path, mode: u32) -> io::Result<()> {
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(mode);
        fs::set_permissions(path, permissions)
    }
}

#[cfg(not(unix))]
pub fn set_permissions(_path: &std::path::Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

pub fn ansi_len(s: &str) -> usize {
    from_utf8(&strip(s)).unwrap().width()
}

pub fn clean_string_from_control_and_linebreak(input: &str) -> String {
    input
        .chars()
        .filter(|c| {
            !c.is_control() &&
            *c != '\n' &&      // line feed
            *c != '\r' &&      // carriage return
            *c != '\u{2028}' && // line separator
            *c != '\u{2029}' // paragraph separator
        })
        .collect()
}

pub fn adjust_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| {
            let actual_len = ansi_len(l);
            if actual_len > columns {
                truncate_ansi_line_to_width(l, columns)
            } else {
                [l, &str::repeat(" ", columns - actual_len)].concat()
            }
        })
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}

fn escape_sequence_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return bytes.len();
    }
    match bytes[1] {
        b'[' => {
            let mut i = 2;
            while i < bytes.len() {
                if (0x40..=0x7e).contains(&bytes[i]) {
                    return i + 1;
                }
                i += 1;
            }
            bytes.len()
        },
        b']' | b'P' | b'_' | b'^' | b'X' => {
            let mut i = 2;
            while i < bytes.len() {
                if bytes[i] == 0x07 {
                    return i + 1;
                }
                if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'\\') {
                    return i + 2;
                }
                i += 1;
            }
            bytes.len()
        },
        _ => 1 + s[1..].chars().next().map(|c| c.len_utf8()).unwrap_or(0),
    }
}

pub fn truncate_ansi_line_to_width(line: &str, columns: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut used = 0;
    let mut rest = line;
    while !rest.is_empty() {
        if rest.starts_with('\u{1b}') {
            let len = escape_sequence_len(rest);
            out.push_str(&rest[..len]);
            rest = &rest[len..];
            continue;
        }
        let text_end = rest.find('\u{1b}').unwrap_or(rest.len());
        let text = &rest[..text_end];
        for (cluster, width) in crate::graphemes::clusters(text) {
            if used + width > columns {
                used = columns + 1;
                break;
            }
            used += width;
            out.push_str(cluster);
        }
        rest = &rest[text_end..];
        if used > columns {
            while let Some(escape_start) = rest.find('\u{1b}') {
                rest = &rest[escape_start..];
                let len = escape_sequence_len(rest);
                out.push_str(&rest[..len]);
                rest = &rest[len..];
            }
            break;
        }
    }
    out
}

pub fn make_terminal_title(pane_title: &str) -> String {
    let pane_title = crate::graphemes::strip_control_characters(pane_title);
    format!(
        "\u{1b}]0;{}{}\u{07}",
        get_session_name()
            .map(|n| {
                let n = crate::graphemes::strip_control_characters(&n).into_owned();
                if pane_title.is_empty() {
                    format!("{}", n)
                } else {
                    format!("{} | ", n)
                }
            })
            .unwrap_or_default(),
        pane_title
    )
}

// Colors
pub mod colors {
    pub const WHITE: u8 = 255;
    pub const GREEN: u8 = 154;
    pub const GRAY: u8 = 238;
    pub const BRIGHT_GRAY: u8 = 245;
    pub const RED: u8 = 124;
    pub const ORANGE: u8 = 166;
    pub const BLACK: u8 = 16;
    pub const MAGENTA: u8 = 201;
    pub const CYAN: u8 = 51;
    pub const YELLOW: u8 = 226;
    pub const BLUE: u8 = 45;
    pub const PURPLE: u8 = 99;
    pub const GOLD: u8 = 136;
    pub const SILVER: u8 = 245;
    pub const PINK: u8 = 207;
    pub const BROWN: u8 = 215;
}

pub mod default_theme_colors {
    pub const BACKGROUND: (u8, u8, u8) = (11, 11, 11);
    pub const SURFACE: (u8, u8, u8) = (56, 56, 56);
    pub const SHADOW: (u8, u8, u8) = (32, 32, 32);
    pub const RIBBON: (u8, u8, u8) = (152, 152, 152);
    pub const RIBBON_HOVER: (u8, u8, u8) = (220, 220, 220);
    pub const FOREGROUND: (u8, u8, u8) = (215, 215, 215);
    pub const MUTED: (u8, u8, u8) = (137, 137, 137);
    pub const DIM: (u8, u8, u8) = (105, 105, 105);
    pub const GREEN: (u8, u8, u8) = (150, 220, 90);
    pub const GOLD: (u8, u8, u8) = (255, 205, 80);
    pub const BLUE: (u8, u8, u8) = (70, 175, 255);
    pub const ROSE: (u8, u8, u8) = (255, 95, 110);
    pub const VIOLET: (u8, u8, u8) = (217, 140, 255);
    pub const TEAL: (u8, u8, u8) = (60, 215, 200);
    pub const PINK: (u8, u8, u8) = (255, 130, 200);
    pub const PURPLE: (u8, u8, u8) = (160, 150, 255);
    pub const ORANGE: (u8, u8, u8) = (255, 145, 70);
    pub const SAND: (u8, u8, u8) = (240, 190, 140);
    pub const WINE: (u8, u8, u8) = (79, 26, 34);
    pub const NAVY: (u8, u8, u8) = (20, 49, 79);
    pub const BELL: (u8, u8, u8) = (99, 11, 20);
}

pub fn _hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    Rgb::from_hex_str(hex)
        .expect("The passed argument must be a valid hex color")
        .into()
}

pub fn eightbit_to_rgb(c: u8) -> (u8, u8, u8) {
    Ansi256::new(c).as_rgb().into()
}

pub fn default_palette() -> Palette {
    Palette {
        source: PaletteSource::Default,
        theme_hue: ThemeHue::Dark,
        fg: PaletteColor::Rgb(default_theme_colors::FOREGROUND),
        bg: PaletteColor::Rgb(default_theme_colors::BACKGROUND),
        black: PaletteColor::Rgb(default_theme_colors::BACKGROUND),
        red: PaletteColor::Rgb(default_theme_colors::ROSE),
        green: PaletteColor::Rgb(default_theme_colors::GREEN),
        yellow: PaletteColor::Rgb(default_theme_colors::GOLD),
        blue: PaletteColor::Rgb(default_theme_colors::BLUE),
        magenta: PaletteColor::Rgb(default_theme_colors::PINK),
        cyan: PaletteColor::Rgb(default_theme_colors::TEAL),
        white: PaletteColor::Rgb(default_theme_colors::FOREGROUND),
        orange: PaletteColor::Rgb(default_theme_colors::ORANGE),
        gray: PaletteColor::Rgb(default_theme_colors::SURFACE),
        purple: PaletteColor::Rgb(default_theme_colors::PURPLE),
        gold: PaletteColor::Rgb(default_theme_colors::GOLD),
        silver: PaletteColor::Rgb(default_theme_colors::MUTED),
        pink: PaletteColor::Rgb(default_theme_colors::PINK),
        brown: PaletteColor::Rgb(default_theme_colors::SAND),
    }
}

// Dark magic
pub fn detect_theme_hue(bg: PaletteColor) -> ThemeHue {
    match bg {
        PaletteColor::Rgb((r, g, b)) => {
            // HSP, P stands for perceived brightness
            let hsp: f64 = (0.299 * (r as f64 * r as f64)
                + 0.587 * (g as f64 * g as f64)
                + 0.114 * (b as f64 * b as f64))
                .sqrt();
            match hsp > 127.5 {
                true => ThemeHue::Light,
                false => ThemeHue::Dark,
            }
        },
        _ => ThemeHue::Dark,
    }
}

// (this was shamelessly copied from alacritty)
//
// This returns the current terminal version as a unique number based on the
// semver version. The different versions are padded to ensure that a higher semver version will
// always report a higher version number.
pub fn version_number(mut version: &str) -> usize {
    if let Some(separator) = version.rfind('-') {
        version = &version[..separator];
    }

    let mut version_number = 0;

    let semver_versions = version.split('.');
    for (i, semver_version) in semver_versions.rev().enumerate() {
        let semver_number = semver_version.parse::<usize>().unwrap_or(0);
        version_number += usize::pow(100, i as u32) * semver_number;
    }

    version_number
}

pub fn web_server_base_url(
    web_server_ip: IpAddr,
    web_server_port: u16,
    has_certificate: bool,
    enforce_https_for_localhost: bool,
    dangerously_allow_web_serving_without_a_certificate: bool,
) -> String {
    let is_loopback = match web_server_ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    };

    let serves_plain_http = !has_certificate
        && if is_loopback {
            !enforce_https_for_localhost
        } else {
            dangerously_allow_web_serving_without_a_certificate
        };
    let url_prefix = if serves_plain_http { "http" } else { "https" };
    format!("{}://{}:{}", url_prefix, web_server_ip, web_server_port)
}

pub fn web_server_base_url_from_config(config_options: Options) -> String {
    let web_server_ip = config_options
        .web_server_ip
        .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let web_server_port = config_options.web_server_port.unwrap_or_else(|| 8082);
    let has_certificate =
        config_options.web_server_cert.is_some() && config_options.web_server_key.is_some();
    let enforce_https_for_localhost = config_options.enforce_https_for_localhost.unwrap_or(false);
    let dangerously_allow_web_serving_without_a_certificate = config_options
        .dangerously_allow_web_serving_without_a_certificate
        .unwrap_or(false);
    web_server_base_url(
        web_server_ip,
        web_server_port,
        has_certificate,
        enforce_https_for_localhost,
        dangerously_allow_web_serving_without_a_certificate,
    )
}

pub struct ServerAddress {
    pub ip: String,
    pub port: u16,
}

pub fn parse_base_url(url: &str) -> Result<ServerAddress> {
    let url = url::Url::parse(url)?;
    let ip = url
        .host_str()
        .ok_or_else(|| anyhow!("No host in URL"))?
        .to_string();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow!("No port in URL"))?;

    Ok(ServerAddress { ip, port })
}

#[cfg(test)]
mod web_server_base_url_tests {
    use super::*;

    fn remote() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
    }

    fn localhost() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    }

    #[test]
    fn non_loopback_without_certificate_uses_https_by_default() {
        assert_eq!(
            web_server_base_url(remote(), 8082, false, false, false),
            "https://0.0.0.0:8082"
        );
    }

    #[test]
    fn non_loopback_without_certificate_uses_http_when_dangerously_allowed() {
        assert_eq!(
            web_server_base_url(remote(), 8082, false, false, true),
            "http://0.0.0.0:8082"
        );
    }

    #[test]
    fn non_loopback_with_certificate_uses_https_when_dangerously_allowed() {
        assert_eq!(
            web_server_base_url(remote(), 8082, true, false, true),
            "https://0.0.0.0:8082"
        );
    }

    #[test]
    fn loopback_urls_are_unchanged_by_dangerous_allowance() {
        assert_eq!(
            web_server_base_url(localhost(), 8082, false, false, true),
            "http://127.0.0.1:8082"
        );
        assert_eq!(
            web_server_base_url(localhost(), 8082, false, true, true),
            "https://127.0.0.1:8082"
        );
        assert_eq!(
            web_server_base_url(localhost(), 8082, true, false, true),
            "https://127.0.0.1:8082"
        );
    }

    #[test]
    fn base_url_from_config_reads_dangerous_allowance() {
        let options = Options {
            web_server_ip: Some(remote()),
            dangerously_allow_web_serving_without_a_certificate: Some(true),
            ..Default::default()
        };
        assert_eq!(
            web_server_base_url_from_config(options),
            "http://0.0.0.0:8082"
        );
        let options = Options {
            web_server_ip: Some(remote()),
            ..Default::default()
        };
        assert_eq!(
            web_server_base_url_from_config(options),
            "https://0.0.0.0:8082"
        );
    }
}

#[cfg(test)]
mod adjust_to_size_tests {
    use super::*;

    #[test]
    fn adjust_to_size_cuts_non_ascii_lines_by_width_on_character_boundaries() {
        let adjusted = adjust_to_size("héllo wörld", 1, 4);
        assert_eq!(adjusted, "héll");
        let adjusted = adjust_to_size("你好世界", 1, 5);
        assert_eq!(adjusted, "你好");
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let adjusted = adjust_to_size(&format!("ab{}cd", family), 1, 3);
        assert_eq!(adjusted, "ab");
    }

    #[test]
    fn adjust_to_size_keeps_styles_when_cutting() {
        let adjusted = adjust_to_size("\u{1b}[31mrédé\u{1b}[0m", 1, 2);
        assert_eq!(adjusted, "\u{1b}[31mré\u{1b}[0m");
    }

    #[test]
    fn terminal_title_has_no_control_characters() {
        let title = make_terminal_title("a\u{7}b\u{1b}c");
        assert!(title.ends_with("abc\u{7}"));
        assert_eq!(title.matches('\u{1b}').count(), 1);
    }
}
