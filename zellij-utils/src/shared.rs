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
                let mut line = String::from(l);
                line.truncate(columns);
                line
            } else {
                [l, &str::repeat(" ", columns - ansi_len(l))].concat()
            }
        })
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}

pub fn make_terminal_title(pane_title: &str) -> String {
    format!(
        "\u{1b}]0;{}{}\u{07}",
        get_session_name()
            .map(|n| if pane_title.is_empty() {
                format!("{}", n)
            } else {
                format!("{} | ", n)
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
        fg: PaletteColor::EightBit(colors::BRIGHT_GRAY),
        bg: PaletteColor::EightBit(colors::GRAY),
        black: PaletteColor::EightBit(colors::BLACK),
        red: PaletteColor::EightBit(colors::RED),
        green: PaletteColor::EightBit(colors::GREEN),
        yellow: PaletteColor::EightBit(colors::YELLOW),
        blue: PaletteColor::EightBit(colors::BLUE),
        magenta: PaletteColor::EightBit(colors::MAGENTA),
        cyan: PaletteColor::EightBit(colors::CYAN),
        white: PaletteColor::EightBit(colors::WHITE),
        orange: PaletteColor::EightBit(colors::ORANGE),
        gray: PaletteColor::EightBit(colors::GRAY),
        purple: PaletteColor::EightBit(colors::PURPLE),
        gold: PaletteColor::EightBit(colors::GOLD),
        silver: PaletteColor::EightBit(colors::SILVER),
        pink: PaletteColor::EightBit(colors::PINK),
        brown: PaletteColor::EightBit(colors::BROWN),
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
