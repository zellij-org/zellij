use axum::http::Request;
use axum_extra::extract::cookie::Cookie;
use std::collections::HashMap;
use std::net::IpAddr;

pub fn get_mime_type(ext: Option<&str>) -> &str {
    match ext {
        None => "text/plain",
        Some(ext) => match ext {
            "html" => "text/html",
            "css" => "text/css",
            "js" => "application/javascript",
            "wasm" => "application/wasm",
            "png" => "image/png",
            "ico" => "image/x-icon",
            "svg" => "image/svg+xml",
            "webmanifest" => "application/manifest+json",
            _ => "text/plain",
        },
    }
}

pub fn should_use_https(
    ip: IpAddr,
    has_certificate: bool,
    enforce_https_for_localhost: bool,
    dangerously_allow_web_serving_without_a_certificate: bool,
) -> Result<bool, String> {
    let is_loopback = match ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    };

    if has_certificate {
        Ok(true)
    } else if is_loopback && enforce_https_for_localhost {
        Err(format!("Cannot bind without an SSL certificate."))
    } else if is_loopback || dangerously_allow_web_serving_without_a_certificate {
        Ok(false)
    } else {
        Err(format!(
            "Cannot bind to non-loopback IP: {} without an SSL certificate.",
            ip
        ))
    }
}

pub fn parse_cookies<T>(request: &Request<T>) -> HashMap<String, String> {
    let mut cookies = HashMap::new();

    for cookie_header in request.headers().get_all("cookie") {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie_part in cookie_str.split(';') {
                if let Ok(cookie) = Cookie::parse(cookie_part.trim()) {
                    cookies.insert(cookie.name().to_string(), cookie.value().to_string());
                }
            }
        }
    }

    cookies
}

pub fn terminal_init_messages() -> Vec<&'static str> {
    let clear_client_terminal_attributes = "\u{1b}[?1l\u{1b}=\u{1b}[r\u{1b}[?1000l\u{1b}[?1002l\u{1b}[?1003l\u{1b}[?1005l\u{1b}[?1006l\u{1b}[?12l";
    let enter_alternate_screen = "\u{1b}[?1049h";
    let bracketed_paste = "\u{1b}[?2004h";
    // Flag 17 = DISAMBIGUATE_ESCAPE_CODES (1) | REPORT_ASSOCIATED_TEXT (16);
    // see the matching constant in zellij-client/src/lib.rs for rationale.
    let enter_kitty_keyboard_mode = "\u{1b}[>17u";
    let enable_mouse_mode = "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1015h\u{1b}[?1006h";
    vec![
        clear_client_terminal_attributes,
        enter_alternate_screen,
        bracketed_paste,
        enter_kitty_keyboard_mode,
        enable_mouse_mode,
    ]
}

#[cfg(test)]
mod should_use_https_tests {
    use super::should_use_https;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    fn localhost() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    }

    fn remote_v4() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))
    }

    fn remote_v6() -> IpAddr {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    }

    #[test]
    fn non_loopback_ipv4_without_certificate_is_refused_by_default() {
        assert!(should_use_https(remote_v4(), false, false, false).is_err());
        assert!(should_use_https(remote_v4(), false, true, false).is_err());
    }

    #[test]
    fn non_loopback_ipv6_without_certificate_is_refused_by_default() {
        assert!(should_use_https(remote_v6(), false, false, false).is_err());
    }

    #[test]
    fn non_loopback_with_certificate_uses_https() {
        assert_eq!(should_use_https(remote_v4(), true, false, false), Ok(true));
        assert_eq!(should_use_https(remote_v6(), true, false, false), Ok(true));
    }

    #[test]
    fn loopback_without_certificate_uses_http() {
        assert_eq!(
            should_use_https(localhost(), false, false, false),
            Ok(false)
        );
        assert_eq!(
            should_use_https(IpAddr::V6(Ipv6Addr::LOCALHOST), false, false, false),
            Ok(false)
        );
    }

    #[test]
    fn loopback_with_certificate_uses_https() {
        assert_eq!(should_use_https(localhost(), true, false, false), Ok(true));
    }

    #[test]
    fn loopback_with_enforced_https_and_certificate_uses_https() {
        assert_eq!(should_use_https(localhost(), true, true, false), Ok(true));
    }

    #[test]
    fn loopback_with_enforced_https_without_certificate_is_refused() {
        assert!(should_use_https(localhost(), false, true, false).is_err());
    }

    #[test]
    fn non_loopback_without_certificate_uses_http_when_dangerously_allowed() {
        assert_eq!(should_use_https(remote_v4(), false, false, true), Ok(false));
        assert_eq!(should_use_https(remote_v6(), false, false, true), Ok(false));
    }

    #[test]
    fn non_loopback_with_certificate_still_uses_https_when_dangerously_allowed() {
        assert_eq!(should_use_https(remote_v4(), true, false, true), Ok(true));
    }

    #[test]
    fn enforced_https_for_localhost_wins_over_dangerous_allowance() {
        assert!(should_use_https(localhost(), false, true, true).is_err());
    }

    #[test]
    fn dangerous_allowance_does_not_change_loopback_behavior() {
        assert_eq!(should_use_https(localhost(), false, false, true), Ok(false));
        assert_eq!(should_use_https(localhost(), true, false, true), Ok(true));
    }
}
