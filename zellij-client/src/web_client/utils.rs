use axum::http::header::{HOST, ORIGIN};
use axum::http::{HeaderMap, Request};
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

const SESSION_TOKEN_COOKIE: &str = "session_token";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicateSessionCookie;

pub fn parse_cookies<T>(
    request: &Request<T>,
) -> Result<HashMap<String, String>, DuplicateSessionCookie> {
    let mut cookies = HashMap::new();

    for cookie_header in request.headers().get_all("cookie") {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie_part in cookie_str.split(';') {
                if let Ok(cookie) = Cookie::parse(cookie_part.trim()) {
                    let name = cookie.name();
                    // A sibling subdomain can add a second, domain-scoped
                    // session cookie. Never let one silently displace the
                    // other; the request is ambiguous.
                    if name == SESSION_TOKEN_COOKIE && cookies.contains_key(name) {
                        return Err(DuplicateSessionCookie);
                    }
                    cookies.insert(name.to_string(), cookie.value().to_string());
                }
            }
        }
    }

    Ok(cookies)
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

/// Whether a websocket upgrade comes from a page served by this server.
///
/// Browsers attach an `Origin` header to every websocket handshake, and the
/// session cookie travels with it even from another origin on the same site
/// (another port, a sibling subdomain), so the cookie alone does not prove the
/// page is ours. If an `Origin` is present its host (and port) must be the one
/// the request was addressed to, and its scheme must be what we serve (https
/// when we terminate TLS ourselves; either one otherwise, since a proxy in
/// front of a plain http server is what provides the https). A proxy that
/// rewrites `Host` is accounted for through `X-Forwarded-Host`.
///
/// Without an `Origin` the request is not from a browser page (a script, the
/// cli), which is not the cross-site scenario this guards against, so it is
/// let through; it still has to carry a valid session cookie.
pub fn websocket_origin_is_allowed(headers: &HeaderMap, is_https: bool) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Some((scheme, origin_authority)) = origin.to_str().ok().and_then(split_origin) else {
        return false;
    };
    if is_https && scheme != "https" {
        return false;
    }
    let origin_authority = strip_default_port(&origin_authority, scheme);

    let forwarded_host = headers
        .get("x-forwarded-host")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim);
    [
        headers.get(HOST).and_then(|v| v.to_str().ok()),
        forwarded_host,
    ]
    .into_iter()
    .flatten()
    .any(|host| strip_default_port(&host.to_ascii_lowercase(), scheme) == origin_authority)
}

fn split_origin(origin: &str) -> Option<(&'static str, String)> {
    let (scheme, rest) = origin.split_once("://")?;
    let scheme = if scheme.eq_ignore_ascii_case("https") {
        "https"
    } else if scheme.eq_ignore_ascii_case("http") {
        "http"
    } else {
        return None;
    };
    // an origin is just scheme://host[:port]
    if rest.is_empty() || rest.contains(['/', '?', '#', '@']) {
        return None;
    }
    Some((scheme, rest.to_ascii_lowercase()))
}

fn strip_default_port(authority: &str, scheme: &str) -> String {
    let default_port = if scheme == "https" { ":443" } else { ":80" };
    authority
        .strip_suffix(default_port)
        .unwrap_or(authority)
        .to_string()
}

#[cfg(test)]
mod websocket_origin_tests {
    use super::websocket_origin_is_allowed;
    use axum::http::HeaderMap;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(*name, value.parse().unwrap());
        }
        headers
    }

    #[test]
    fn a_missing_origin_is_allowed() {
        assert!(websocket_origin_is_allowed(
            &headers(&[("host", "zellij.example:8082")]),
            false
        ));
        assert!(websocket_origin_is_allowed(
            &headers(&[("host", "zellij.example")]),
            true
        ));
    }

    #[test]
    fn the_servers_own_origin_is_allowed() {
        let own = headers(&[
            ("host", "zellij.example:8082"),
            ("origin", "http://zellij.example:8082"),
        ]);
        assert!(websocket_origin_is_allowed(&own, false));
        let own_tls = headers(&[
            ("host", "Zellij.Example:8082"),
            ("origin", "https://zellij.example:8082"),
        ]);
        assert!(websocket_origin_is_allowed(&own_tls, true));
    }

    #[test]
    fn default_ports_are_equivalent_to_no_port() {
        let h = headers(&[
            ("host", "zellij.example"),
            ("origin", "https://zellij.example:443"),
        ]);
        assert!(websocket_origin_is_allowed(&h, true));
        let h = headers(&[
            ("host", "zellij.example:443"),
            ("origin", "https://zellij.example"),
        ]);
        assert!(websocket_origin_is_allowed(&h, true));
    }

    #[test]
    fn another_site_is_refused() {
        let h = headers(&[
            ("host", "zellij.example:8082"),
            ("origin", "https://evil.example"),
        ]);
        assert!(!websocket_origin_is_allowed(&h, false));
    }

    #[test]
    fn another_port_or_subdomain_on_the_same_host_is_refused() {
        let other_port = headers(&[
            ("host", "zellij.example:8082"),
            ("origin", "http://zellij.example:9999"),
        ]);
        assert!(!websocket_origin_is_allowed(&other_port, false));
        let sibling = headers(&[
            ("host", "zellij.example:8082"),
            ("origin", "http://blog.zellij.example:8082"),
        ]);
        assert!(!websocket_origin_is_allowed(&sibling, false));
        let suffix_trick = headers(&[
            ("host", "zellij.example"),
            ("origin", "http://zellij.example.evil.example"),
        ]);
        assert!(!websocket_origin_is_allowed(&suffix_trick, false));
    }

    #[test]
    fn opaque_and_malformed_origins_are_refused() {
        for origin in [
            "null",
            "",
            "zellij.example",
            "ftp://zellij.example",
            "http://zellij.example/path",
            "http://user@zellij.example",
        ] {
            let h = headers(&[("host", "zellij.example"), ("origin", origin)]);
            assert!(
                !websocket_origin_is_allowed(&h, false),
                "origin {:?} must be refused",
                origin
            );
        }
    }

    #[test]
    fn a_plain_http_origin_is_refused_by_a_tls_server() {
        let h = headers(&[
            ("host", "zellij.example"),
            ("origin", "http://zellij.example"),
        ]);
        assert!(!websocket_origin_is_allowed(&h, true));
    }

    #[test]
    fn a_proxy_that_terminates_tls_and_rewrites_host_is_supported() {
        let h = headers(&[
            ("host", "127.0.0.1:8082"),
            ("x-forwarded-host", "zellij.example"),
            ("origin", "https://zellij.example"),
        ]);
        assert!(websocket_origin_is_allowed(&h, false));
    }

    #[test]
    fn a_request_without_a_host_cannot_vouch_for_an_origin() {
        let h = headers(&[("origin", "http://zellij.example")]);
        assert!(!websocket_origin_is_allowed(&h, false));
    }
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

#[cfg(test)]
mod cookie_tests {
    use super::{parse_cookies, DuplicateSessionCookie};
    use axum::http::Request;

    fn request_with_cookie_headers(headers: &[&'static str]) -> Request<()> {
        let mut builder = Request::builder();
        for value in headers {
            builder = builder.header("cookie", *value);
        }
        builder.body(()).unwrap()
    }

    #[test]
    fn cookies_from_several_headers_are_merged() {
        let request =
            request_with_cookie_headers(&["session_token=abc; theme=dark", "other=value"]);
        let cookies = parse_cookies(&request).unwrap();
        assert_eq!(
            cookies.get("session_token").map(String::as_str),
            Some("abc")
        );
        assert_eq!(cookies.get("theme").map(String::as_str), Some("dark"));
        assert_eq!(cookies.get("other").map(String::as_str), Some("value"));
    }

    #[test]
    fn duplicate_session_cookies_in_one_header_are_rejected() {
        let request = request_with_cookie_headers(&["session_token=first; session_token=second"]);
        assert_eq!(parse_cookies(&request), Err(DuplicateSessionCookie));
    }

    #[test]
    fn duplicate_session_cookies_across_headers_are_rejected() {
        let request = request_with_cookie_headers(&[
            "session_token=first; theme=dark",
            "session_token=second",
        ]);
        assert_eq!(parse_cookies(&request), Err(DuplicateSessionCookie));
    }
}
