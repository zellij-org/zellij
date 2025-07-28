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
            _ => "text/plain",
        },
    }
}

pub fn should_use_https(
    ip: IpAddr,
    has_certificate: bool,
    enforce_https_for_localhost: bool,
) -> Result<bool, String> {
    let is_loopback = match ip {
        IpAddr::V4(ipv4) => ipv4.is_loopback(),
        IpAddr::V6(ipv6) => ipv6.is_loopback(),
    };

    if is_loopback && !enforce_https_for_localhost {
        Ok(has_certificate)
    } else if is_loopback {
        Err(format!("Cannot bind without an SSL certificate."))
    } else if has_certificate {
        Ok(true)
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
    let enter_kitty_keyboard_mode = "\u{1b}[>1u";
    let enable_mouse_mode = "\u{1b}[?1000h\u{1b}[?1002h\u{1b}[?1015h\u{1b}[?1006h";
    vec![
        clear_client_terminal_attributes,
        enter_alternate_screen,
        bracketed_paste,
        enter_kitty_keyboard_mode,
        enable_mouse_mode,
    ]
}
