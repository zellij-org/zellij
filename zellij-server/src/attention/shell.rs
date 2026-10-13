use std::sync::OnceLock;

use zellij_utils::attention::{CommandText, CommandTextSource, CwdSource, ReportedCwd};

pub const MAX_COMMAND_TEXT_CHARS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellProtocol {
    Osc133,
    Osc633,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Initial,
    Continuation,
    Secondary,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MarkOptions {
    pub aid: Option<String>,
    pub k: Option<String>,
    pub cl: Option<String>,
    pub err: Option<String>,
    pub cmdline: Option<String>,
    pub cmdline_url: Option<String>,
    pub click_events: Option<String>,
    pub redraw: Option<String>,
    pub special_key: Option<String>,
    pub history_id: Option<String>,
}

impl MarkOptions {
    fn set(&mut self, key: &str, value: &str) {
        let value = Some(value.to_owned());
        match key {
            "aid" => self.aid = value,
            "k" => self.k = value,
            "cl" => self.cl = value,
            "err" => self.err = value,
            "cmdline" => self.cmdline = value,
            "cmdline_url" => self.cmdline_url = value,
            "click_events" => self.click_events = value,
            "redraw" => self.redraw = value,
            "special_key" => self.special_key = value,
            "history_id" => self.history_id = value,
            _ => {},
        }
    }
    pub fn prompt_kind(&self) -> Option<PromptKind> {
        match self.k.as_deref() {
            Some("i") => Some(PromptKind::Initial),
            Some("c") => Some(PromptKind::Continuation),
            Some("s") => Some(PromptKind::Secondary),
            Some("r") => Some(PromptKind::Right),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkKind {
    PromptStart { main: bool },
    FreshLine,
    InputStart,
    CommandStart { command: Option<CommandText> },
    CommandEnd { exit_code: Option<i32>, err: Option<String> },
    CommandText(CommandText),
    Cwd(ReportedCwd),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellMark {
    pub protocol: ShellProtocol,
    pub kind: MarkKind,
    pub options: MarkOptions,
}

impl ShellMark {
    pub fn same_event_as(&self, other: &ShellMark) -> bool {
        if self.protocol == other.protocol {
            return false;
        }
        match (&self.kind, &other.kind) {
            (MarkKind::PromptStart { .. }, MarkKind::PromptStart { .. })
            | (MarkKind::InputStart, MarkKind::InputStart)
            | (MarkKind::CommandStart { .. }, MarkKind::CommandStart { .. })
            | (MarkKind::CommandEnd { .. }, MarkKind::CommandEnd { .. }) => true,
            _ => false,
        }
    }
}

fn utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

fn parse_options(fields: &[&[u8]], options: &mut MarkOptions) {
    for field in fields {
        let Some(field) = utf8(field) else {
            continue;
        };
        if let Some((key, value)) = field.split_once('=') {
            options.set(key.trim(), value);
        }
    }
}

pub fn clean_command_text(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| !zellij_utils::attention::is_invisible_formatting(*c))
        .take(MAX_COMMAND_TEXT_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn shell_text(text: String) -> Option<CommandText> {
    clean_command_text(&text).map(|text| CommandText {
        text,
        source: CommandTextSource::Shell,
    })
}

fn main_prompt(command: &[u8], options: &MarkOptions) -> Option<MarkKind> {
    let kind = options.prompt_kind();
    match command {
        b"A" | b"N" => Some(MarkKind::PromptStart {
            main: !matches!(
                kind,
                Some(PromptKind::Secondary) | Some(PromptKind::Continuation) | Some(PromptKind::Right)
            ),
        }),
        b"P" => Some(MarkKind::PromptStart {
            main: matches!(kind, None | Some(PromptKind::Initial)),
        }),
        _ => None,
    }
}

pub fn parse_exit_code(field: Option<&[u8]>) -> Option<i32> {
    field
        .and_then(utf8)
        .map(|f| f.trim())
        .filter(|f| !f.contains('='))
        .and_then(|f| f.parse::<i32>().ok())
}

pub fn parse_osc133(params: &[&[u8]]) -> Option<ShellMark> {
    let command = *params.get(1)?;
    let fields = params.get(2..).unwrap_or_default();
    let mut options = MarkOptions::default();
    let kind = match command {
        b"A" | b"N" | b"P" => {
            parse_options(fields, &mut options);
            main_prompt(command, &options)?
        },
        b"L" => {
            parse_options(fields, &mut options);
            MarkKind::FreshLine
        },
        b"B" | b"I" => {
            parse_options(fields, &mut options);
            MarkKind::InputStart
        },
        b"C" => {
            let mut text = None;
            for (index, field) in fields.iter().enumerate() {
                let Some(field_text) = utf8(field) else {
                    continue;
                };
                if let Some(value) = field_text.strip_prefix("cmdline=") {
                    let mut raw = value.to_owned();
                    for rest in &fields[index + 1..] {
                        raw.push(';');
                        raw.push_str(&String::from_utf8_lossy(rest));
                    }
                    options.cmdline = Some(raw.clone());
                    text = Some(decode_printf_q(&raw));
                    break;
                }
                if let Some((key, value)) = field_text.split_once('=') {
                    options.set(key.trim(), value);
                }
            }
            if text.is_none() {
                if let Some(url) = options.cmdline_url.as_deref() {
                    text = Some(decode_percent(url));
                }
            }
            MarkKind::CommandStart {
                command: text.and_then(shell_text),
            }
        },
        b"D" => {
            let exit_code = parse_exit_code(fields.first().copied());
            parse_options(fields, &mut options);
            MarkKind::CommandEnd {
                exit_code,
                err: options.err.clone(),
            }
        },
        _ => return None,
    };
    Some(ShellMark {
        protocol: ShellProtocol::Osc133,
        kind,
        options,
    })
}

pub fn parse_osc633(params: &[&[u8]]) -> Option<ShellMark> {
    let command = *params.get(1)?;
    let fields = params.get(2..).unwrap_or_default();
    let options = MarkOptions::default();
    let kind = match command {
        b"A" => MarkKind::PromptStart { main: true },
        b"B" => MarkKind::InputStart,
        b"C" => MarkKind::CommandStart { command: None },
        b"D" => MarkKind::CommandEnd {
            exit_code: parse_exit_code(fields.first().copied()),
            err: None,
        },
        b"E" => {
            let text = fields.first().map(|f| decode_633(&String::from_utf8_lossy(f)))?;
            MarkKind::CommandText(shell_text(text)?)
        },
        b"P" => {
            let property = utf8(fields.first()?)?;
            let value = property.strip_prefix("Cwd=")?;
            let path = decode_633(value);
            if path.is_empty() {
                return None;
            }
            MarkKind::Cwd(ReportedCwd {
                path,
                source: CwdSource::Vscode,
                host: None,
                is_local: true,
            })
        },
        _ => return None,
    };
    Some(ShellMark {
        protocol: ShellProtocol::Osc633,
        kind,
        options,
    })
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn decode_percent(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn decode_633(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            if bytes.get(i + 1) == Some(&b'\\') {
                out.push(b'\\');
                i += 2;
                continue;
            }
            if bytes.get(i + 1) == Some(&b'x') {
                if let (Some(h), Some(l)) = (
                    bytes.get(i + 2).and_then(|b| hex_value(*b)),
                    bytes.get(i + 3).and_then(|b| hex_value(*b)),
                ) {
                    out.push(h * 16 + l);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn push_char(out: &mut Vec<u8>, c: char) {
    let mut buffer = [0u8; 4];
    out.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
}

fn decode_ansi_c(bytes: &[u8], mut i: usize, out: &mut Vec<u8>) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => return i + 1,
            b'\\' if i + 1 < bytes.len() => {
                let escape = bytes[i + 1];
                i += 2;
                match escape {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'a' => out.push(0x07),
                    b'b' => out.push(0x08),
                    b'e' | b'E' => out.push(0x1b),
                    b'f' => out.push(0x0c),
                    b'v' => out.push(0x0b),
                    b'\\' => out.push(b'\\'),
                    b'\'' => out.push(b'\''),
                    b'"' => out.push(b'"'),
                    b'?' => out.push(b'?'),
                    b'c' => {
                        if let Some(next) = bytes.get(i) {
                            out.push(next & 0x1f);
                            i += 1;
                        }
                    },
                    b'x' => {
                        let mut value: u32 = 0;
                        let mut digits = 0;
                        while digits < 2 {
                            match bytes.get(i).and_then(|b| hex_value(*b)) {
                                Some(v) => {
                                    value = value * 16 + v as u32;
                                    i += 1;
                                    digits += 1;
                                },
                                None => break,
                            }
                        }
                        if digits == 0 {
                            out.extend_from_slice(b"\\x");
                        } else {
                            out.push(value as u8);
                        }
                    },
                    b'u' | b'U' => {
                        let max_digits = if escape == b'u' { 4 } else { 8 };
                        let mut value: u32 = 0;
                        let mut digits = 0;
                        while digits < max_digits {
                            match bytes.get(i).and_then(|b| hex_value(*b)) {
                                Some(v) => {
                                    value = value * 16 + v as u32;
                                    i += 1;
                                    digits += 1;
                                },
                                None => break,
                            }
                        }
                        match char::from_u32(value).filter(|_| digits > 0) {
                            Some(c) => push_char(out, c),
                            None => {
                                out.push(b'\\');
                                out.push(escape);
                            },
                        }
                    },
                    b'0'..=b'7' => {
                        let mut value: u32 = (escape - b'0') as u32;
                        let mut digits = 1;
                        while digits < 3 {
                            match bytes.get(i) {
                                Some(b) if (b'0'..=b'7').contains(b) => {
                                    value = value * 8 + (b - b'0') as u32;
                                    i += 1;
                                    digits += 1;
                                },
                                _ => break,
                            }
                        }
                        out.push((value & 0xff) as u8);
                    },
                    other => {
                        out.push(b'\\');
                        out.push(other);
                    },
                }
            },
            b => {
                out.push(b);
                i += 1;
            },
        }
    }
    i
}

pub fn decode_printf_q(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if let Some(next) = bytes.get(i + 1) {
                    if *next != b'\n' {
                        out.push(*next);
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            },
            b'$' if bytes.get(i + 1) == Some(&b'\'') => {
                i = decode_ansi_c(bytes, i + 2, &mut out);
            },
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    out.push(bytes[i]);
                    i += 1;
                }
                i += 1;
            },
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\'
                        && matches!(bytes.get(i + 1), Some(b'"' | b'\\' | b'$' | b'`'))
                    {
                        out.push(bytes[i + 1]);
                        i += 2;
                    } else {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
                i += 1;
            },
            b => {
                out.push(b);
                i += 1;
            },
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn local_hostname() -> &'static Option<String> {
    static HOSTNAME: OnceLock<Option<String>> = OnceLock::new();
    HOSTNAME.get_or_init(zellij_utils::session_index::hostname)
}

pub fn is_local_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() || host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return true;
    }
    match local_hostname() {
        Some(local) => {
            let local = local.to_ascii_lowercase();
            let short = |name: &str| name.split('.').next().unwrap_or_default().to_owned();
            host == local || short(&host) == short(&local)
        },
        None => false,
    }
}

fn split_authority(rest: &str) -> (String, &str) {
    match rest.find('/') {
        Some(index) => {
            let authority = &rest[..index];
            let host = authority
                .rsplit_once('@')
                .map(|(_, host)| host)
                .unwrap_or(authority);
            (host.to_owned(), &rest[index..])
        },
        None => (rest.to_owned(), ""),
    }
}

pub fn parse_osc7_uri(uri: &str) -> Option<ReportedCwd> {
    if let Some(rest) = uri.strip_prefix("file://") {
        let (host, path) = split_authority(rest);
        let path = path.split(['?', '#']).next().unwrap_or_default();
        if path.is_empty() {
            return None;
        }
        let path = decode_percent(path);
        let is_local = is_local_host(&host);
        return Some(ReportedCwd {
            path,
            source: CwdSource::Osc7,
            host: (!host.is_empty()).then_some(host),
            is_local,
        });
    }
    if let Some(rest) = uri.strip_prefix("kitty-shell-cwd://") {
        let (host, path) = split_authority(rest);
        if path.is_empty() {
            return None;
        }
        let is_local = is_local_host(&host);
        return Some(ReportedCwd {
            path: path.to_owned(),
            source: CwdSource::KittyShellCwd,
            host: (!host.is_empty()).then_some(host),
            is_local,
        });
    }
    None
}

pub fn parse_conemu_cwd(raw: &str) -> Option<ReportedCwd> {
    let trimmed = raw.trim();
    let path = trimmed
        .strip_prefix('"')
        .and_then(|p| p.strip_suffix('"'))
        .unwrap_or(trimmed);
    if path.is_empty() {
        return None;
    }
    Some(ReportedCwd {
        path: path.to_owned(),
        source: CwdSource::ConEmu,
        host: None,
        is_local: true,
    })
}

pub fn parse_iterm2_cwd(raw: &str, remote_host: Option<&str>) -> Option<ReportedCwd> {
    let path = raw.strip_prefix("CurrentDir=")?;
    if path.is_empty() {
        return None;
    }
    let host = remote_host.map(|h| {
        h.rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(h)
            .to_owned()
    });
    let is_local = host.as_deref().map(is_local_host).unwrap_or(true);
    Some(ReportedCwd {
        path: path.to_owned(),
        source: CwdSource::ITerm2,
        host,
        is_local,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(body: &str) -> Vec<Vec<u8>> {
        body.split(';').map(|p| p.as_bytes().to_vec()).collect()
    }

    fn mark133(body: &str) -> Option<ShellMark> {
        let owned = params(body);
        let refs: Vec<&[u8]> = owned.iter().map(|p| p.as_slice()).collect();
        parse_osc133(&refs)
    }

    fn mark633(body: &str) -> Option<ShellMark> {
        let owned = params(body);
        let refs: Vec<&[u8]> = owned.iter().map(|p| p.as_slice()).collect();
        parse_osc633(&refs)
    }

    fn main_of(body: &str) -> bool {
        match mark133(body).unwrap().kind {
            MarkKind::PromptStart { main } => main,
            other => panic!("not a prompt: {:?}", other),
        }
    }

    fn end_of(body: &str) -> (Option<i32>, Option<String>, Option<String>) {
        let mark = mark133(body).unwrap();
        match mark.kind {
            MarkKind::CommandEnd { exit_code, err } => (exit_code, err, mark.options.aid),
            other => panic!("not an end: {:?}", other),
        }
    }

    fn command_of(mark: Option<ShellMark>) -> Option<String> {
        match mark.unwrap().kind {
            MarkKind::CommandStart { command } => command.map(|c| c.text),
            other => panic!("not a start: {:?}", other),
        }
    }

    #[test]
    fn main_prompt_rule() {
        assert!(main_of("133;A"));
        assert!(main_of("133;A;click_events=1"));
        assert!(main_of("133;A;k=i;click_events=1"));
        assert!(main_of("133;A;cl=m;aid=42"));
        assert!(main_of("133;A;redraw=last;cl=line;aid=9"));
        assert!(main_of("133;N"));
        assert!(main_of("133;P"));
        assert!(main_of("133;P;k=i"));
        assert!(!main_of("133;A;k=s"));
        assert!(!main_of("133;A;k=s;click_events=1"));
        assert!(!main_of("133;N;k=s"));
        assert!(!main_of("133;P;k=s"));
        assert!(!main_of("133;P;k=c"));
        assert!(!main_of("133;P;k=r"));
    }

    #[test]
    fn options_are_parsed_and_unknown_ignored() {
        let mark = mark133("133;A;aid=7;cl=m;special_key=1;redraw=last;history_id=x;foo=bar").unwrap();
        assert_eq!(mark.options.aid.as_deref(), Some("7"));
        assert_eq!(mark.options.cl.as_deref(), Some("m"));
        assert_eq!(mark.options.special_key.as_deref(), Some("1"));
        assert_eq!(mark.options.redraw.as_deref(), Some("last"));
        assert_eq!(mark.options.history_id.as_deref(), Some("x"));
    }

    #[test]
    fn d_variants() {
        assert_eq!(end_of("133;D;0"), (Some(0), None, None));
        assert_eq!(end_of("133;D"), (None, None, None));
        assert_eq!(end_of("133;D;1;aid=123"), (Some(1), None, Some("123".to_owned())));
        assert_eq!(end_of("133;D;aid=123"), (None, None, Some("123".to_owned())));
        assert_eq!(end_of("133;D;0;history_id=x"), (Some(0), None, None));
        assert_eq!(
            end_of("133;D;0;err=CANCEL"),
            (Some(0), Some("CANCEL".to_owned()), None)
        );
        assert_eq!(end_of("133;D;abc"), (None, None, None));
        assert_eq!(end_of("133;D;-1"), (Some(-1), None, None));
    }

    #[test]
    fn other_commands() {
        assert_eq!(mark133("133;B").unwrap().kind, MarkKind::InputStart);
        assert_eq!(mark133("133;I").unwrap().kind, MarkKind::InputStart);
        assert_eq!(mark133("133;L").unwrap().kind, MarkKind::FreshLine);
        assert_eq!(mark133("133;Z"), None);
        assert_eq!(command_of(mark133("133;C")), None);
        assert_eq!(command_of(mark133("133;C;")), None);
    }

    #[test]
    fn cmdline_is_decoded_like_printf_q() {
        assert_eq!(
            command_of(mark133(r"133;C;cmdline=ls\ -la\ /tmp")),
            Some("ls -la /tmp".to_owned())
        );
        assert_eq!(
            command_of(mark133(r"133;C;cmdline=echo\ a\;\ echo\ b")),
            Some("echo a; echo b".to_owned())
        );
        assert_eq!(
            command_of(mark133(r"133;C;cmdline=printf\ $'a\tb\x41\101\u00e9'")),
            Some("printf a b\u{41}A\u{e9}".to_owned())
        );
        assert_eq!(
            command_of(mark133(r"133;C;aid=1;cmdline=echo\ 'x y'")),
            Some("echo x y".to_owned())
        );
    }

    #[test]
    fn cmdline_url_is_percent_decoded() {
        assert_eq!(
            command_of(mark133("133;C;cmdline_url=cargo%20build%20--release")),
            Some("cargo build --release".to_owned())
        );
        assert_eq!(
            command_of(mark133("133;C;cmdline_url=echo%20%C3%A9%3B%20x")),
            Some("echo é; x".to_owned())
        );
        assert_eq!(
            command_of(mark133("133;C;cmdline_url=100%")),
            Some("100%".to_owned())
        );
    }

    #[test]
    fn vscode_sequences() {
        assert_eq!(
            mark633("633;A").unwrap().kind,
            MarkKind::PromptStart { main: true }
        );
        assert_eq!(
            mark633("633;D;2").unwrap().kind,
            MarkKind::CommandEnd {
                exit_code: Some(2),
                err: None
            }
        );
        assert_eq!(
            mark633(r"633;E;echo a\x3b b\\c;nonce123").unwrap().kind,
            MarkKind::CommandText(CommandText {
                text: r"echo a; b\c".to_owned(),
                source: CommandTextSource::Shell
            })
        );
        match mark633(r"633;P;Cwd=/home/u/my\x3bdir;nonce").unwrap().kind {
            MarkKind::Cwd(cwd) => {
                assert_eq!(cwd.path, "/home/u/my;dir");
                assert_eq!(cwd.source, CwdSource::Vscode);
            },
            other => panic!("unexpected {:?}", other),
        }
        assert_eq!(mark633("633;P;IsWindows=True"), None);
        assert_eq!(mark633("633;F"), None);
        assert_eq!(mark633("633;SetMark"), None);
    }

    #[test]
    fn duplicate_events_across_protocols() {
        let a = mark133("133;C").unwrap();
        let b = mark633("633;C").unwrap();
        assert!(a.same_event_as(&b));
        assert!(!a.same_event_as(&a));
        let d = mark633("633;D;0").unwrap();
        assert!(!a.same_event_as(&d));
    }

    #[test]
    fn osc7_uris() {
        let cwd = parse_osc7_uri("file://localhost/home/u/a%20b").unwrap();
        assert_eq!(cwd.path, "/home/u/a b");
        assert!(cwd.is_local);
        let cwd = parse_osc7_uri("file:///tmp").unwrap();
        assert_eq!(cwd.path, "/tmp");
        assert_eq!(cwd.host, None);
        let cwd = parse_osc7_uri("file://user@remote.example.org/srv/app?machineID=abc").unwrap();
        assert_eq!(cwd.path, "/srv/app");
        assert_eq!(cwd.host.as_deref(), Some("remote.example.org"));
        assert!(!cwd.is_local);
        let cwd = parse_osc7_uri("kitty-shell-cwd://localhost/a%20b").unwrap();
        assert_eq!(cwd.path, "/a%20b");
        assert_eq!(cwd.source, CwdSource::KittyShellCwd);
        assert_eq!(parse_osc7_uri("http://x/y"), None);
        assert_eq!(parse_osc7_uri("file://host"), None);
    }

    #[test]
    fn other_cwd_sources() {
        assert_eq!(parse_conemu_cwd("\"C:\\Users\\me\"").unwrap().path, "C:\\Users\\me");
        assert_eq!(parse_conemu_cwd("/tmp").unwrap().path, "/tmp");
        assert_eq!(parse_conemu_cwd("\"\""), None);
        let cwd = parse_iterm2_cwd("CurrentDir=/a b", None).unwrap();
        assert_eq!(cwd.path, "/a b");
        assert!(cwd.is_local);
        let cwd = parse_iterm2_cwd("CurrentDir=/srv", Some("me@far.example.org")).unwrap();
        assert!(!cwd.is_local);
        assert_eq!(parse_iterm2_cwd("SetUserVar=x", None), None);
    }

    #[test]
    fn command_text_is_cleaned() {
        assert_eq!(clean_command_text("  a\tb\u{202e}c  "), Some("a bc".to_owned()));
        assert_eq!(clean_command_text(" \n "), None);
    }
}
