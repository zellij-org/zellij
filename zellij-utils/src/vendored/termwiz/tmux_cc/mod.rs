use anyhow::{anyhow, Context};
use parser::Rule;
use pest::iterators::{Pair, Pairs};
use pest::Parser as _;

pub type TmuxWindowId = u64;
pub type TmuxPaneId = u64;
pub type TmuxSessionId = u64;

pub mod parser {
    use pest_derive::Parser;
    #[derive(Parser)]
    #[grammar = "vendored/termwiz/tmux_cc/tmux.pest"]
    pub struct TmuxParser;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guarded {
    pub error: bool,
    pub timestamp: i64,
    pub number: u64,
    pub flags: i64,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    // Tmux generic events
    Begin {
        timestamp: i64,
        number: u64,
        flags: i64,
    },
    End {
        timestamp: i64,
        number: u64,
        flags: i64,
    },
    Error {
        timestamp: i64,
        number: u64,
        flags: i64,
    },
    Guarded(Guarded),

    // Tmux specific events
    ClientDetached {
        client_name: String,
    },
    ClientSessionChanged {
        client_name: String,
        session: TmuxSessionId,
        session_name: String,
    },
    ConfigError {
        error: String,
    },
    Continue {
        pane: TmuxPaneId,
    },
    ExtendedOutput {
        pane: TmuxPaneId,
        text: String,
    },
    Exit {
        reason: Option<String>,
    },
    LayoutChange {
        window: TmuxWindowId,
        layout: String,
        visible_layout: Option<String>,
        raw_flags: Option<String>,
    },
    Message {
        message: String,
    },
    Output {
        pane: TmuxPaneId,
        text: String,
    },
    PaneModeChanged {
        pane: TmuxPaneId,
    },
    PasteBufferChanged {
        buffer: String,
    },
    PasteBufferDeleted {
        buffer: String,
    },
    Pause {
        pane: TmuxPaneId,
    },
    SessionChanged {
        session: TmuxSessionId,
        name: String,
    },
    SessionRenamed {
        name: String,
    },
    SessionsChanged,
    SessionWindowChanged {
        session: TmuxSessionId,
        window: TmuxWindowId,
    },
    SubscriptionChanged,
    UnlinkedWindowAdd {
        window: TmuxWindowId,
    },
    UnlinkedWindowClose {
        window: TmuxWindowId,
    },
    UnlinkedWindowRenamed {
        window: TmuxWindowId,
    },
    WindowAdd {
        window: TmuxWindowId,
    },
    WindowClose {
        window: TmuxWindowId,
    },
    WindowPaneChanged {
        window: TmuxWindowId,
        pane: TmuxPaneId,
    },
    WindowRenamed {
        window: TmuxWindowId,
        name: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct PaneLayout {
    pub pane_id: TmuxPaneId,
    pub pane_width: u64,
    pub pane_height: u64,
    pub pane_left: u64,
    pub pane_top: u64,
}

#[derive(Debug)]
pub enum WindowLayout {
    SplitVertical(Vec<PaneLayout>),
    SplitHorizontal(Vec<PaneLayout>),
    SinglePane(PaneLayout),
}

fn parse_pane_id(pair: Pair<Rule>) -> anyhow::Result<TmuxPaneId> {
    match pair.as_rule() {
        Rule::pane_id => {
            let mut pairs = pair.into_inner();
            pairs
                .next()
                .ok_or_else(|| anyhow!("missing pane id"))?
                .as_str()
                .parse()
                .context("pane_id is somehow not digits")
        },
        _ => anyhow::bail!("parse_pane_id can only parse Rule::pane_id, got {:?}", pair),
    }
}

fn parse_window_id(pair: Pair<Rule>) -> anyhow::Result<TmuxWindowId> {
    match pair.as_rule() {
        Rule::window_id => {
            let mut pairs = pair.into_inner();
            pairs
                .next()
                .ok_or_else(|| anyhow!("missing window id"))?
                .as_str()
                .parse()
                .context("window_id is somehow not digits")
        },
        _ => anyhow::bail!(
            "parse_window_id can only parse Rule::window_id, got {:?}",
            pair
        ),
    }
}

fn parse_session_id(pair: Pair<Rule>) -> anyhow::Result<TmuxSessionId> {
    match pair.as_rule() {
        Rule::session_id => {
            let mut pairs = pair.into_inner();
            pairs
                .next()
                .ok_or_else(|| anyhow!("missing session id"))?
                .as_str()
                .parse()
                .context("session_id is somehow not digits")
        },
        _ => anyhow::bail!(
            "parse_session_id can only parse Rule::session_id, got {:?}",
            pair
        ),
    }
}

/// Parses a %begin, %end, %error guard line tuple
fn parse_guard(mut pairs: Pairs<Rule>) -> anyhow::Result<(i64, u64, i64)> {
    let timestamp = pairs
        .next()
        .ok_or_else(|| anyhow!("missing timestamp"))?
        .as_str()
        .parse::<i64>()?;
    let number = pairs
        .next()
        .ok_or_else(|| anyhow!("missing number"))?
        .as_str()
        .parse::<u64>()?;
    let flags = pairs
        .next()
        .ok_or_else(|| anyhow!("missing flags"))?
        .as_str()
        .parse::<i64>()?;
    Ok((timestamp, number, flags))
}

fn parse_line(line: &str) -> anyhow::Result<Event> {
    let mut pairs = parser::TmuxParser::parse(Rule::line_entire, line)?;
    let pair = pairs.next().ok_or_else(|| anyhow::anyhow!("no pairs!?"))?;
    match pair.as_rule() {
        // Tmux generic rules
        Rule::begin => {
            let (timestamp, number, flags) = parse_guard(pair.into_inner())?;
            Ok(Event::Begin {
                timestamp,
                number,
                flags,
            })
        },
        Rule::end => {
            let (timestamp, number, flags) = parse_guard(pair.into_inner())?;
            Ok(Event::End {
                timestamp,
                number,
                flags,
            })
        },
        Rule::error => {
            let (timestamp, number, flags) = parse_guard(pair.into_inner())?;
            Ok(Event::Error {
                timestamp,
                number,
                flags,
            })
        },

        // Tmux specific rules
        Rule::client_detached => {
            let mut pairs = pair.into_inner();
            let client_name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            Ok(Event::ClientDetached { client_name })
        },
        Rule::client_session_changed => {
            let mut pairs = pair.into_inner();
            let client_name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            let session =
                parse_session_id(pairs.next().ok_or_else(|| anyhow!("missing session id"))?)?;
            let session_name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing session name"))?
                    .as_str(),
            )?;
            Ok(Event::ClientSessionChanged {
                client_name,
                session,
                session_name,
            })
        },
        Rule::config_error => {
            let mut pairs = pair.into_inner();
            let error = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            Ok(Event::ConfigError { error })
        },
        Rule::r#continue => {
            let mut pairs = pair.into_inner();
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            Ok(Event::Continue { pane })
        },
        Rule::extended_output => {
            let mut pairs = pair.into_inner();
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            let text = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing text"))?
                    .as_str(),
            )?;
            Ok(Event::ExtendedOutput { pane, text })
        },
        Rule::exit => {
            let mut pairs = pair.into_inner();
            let reason = pairs.next().map(|pair| pair.as_str().to_owned());
            Ok(Event::Exit { reason })
        },
        Rule::layout_change => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            let layout = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing layout"))?
                    .as_str(),
            )?;
            let visible_layout = pairs.next().map(|pair| pair.as_str().to_owned());
            let raw_flags = pairs.next().map(|r| r.as_str().to_owned());
            Ok(Event::LayoutChange {
                window,
                layout,
                visible_layout,
                raw_flags,
            })
        },
        Rule::message => {
            let mut pairs = pair.into_inner();
            let message = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing text"))?
                    .as_str(),
            )?;
            Ok(Event::Message { message })
        },
        Rule::output => {
            let mut pairs = pair.into_inner();
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            let text = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing text"))?
                    .as_str(),
            )?;
            Ok(Event::Output { pane, text })
        },
        Rule::pane_mode_changed => {
            let mut pairs = pair.into_inner();
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            Ok(Event::PaneModeChanged { pane })
        },
        Rule::paste_buffer_changed => {
            let mut pairs = pair.into_inner();
            let buffer = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing text"))?
                    .as_str(),
            )?;
            Ok(Event::PasteBufferChanged { buffer })
        },
        Rule::paste_buffer_deleted => {
            let mut pairs = pair.into_inner();
            let buffer = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing text"))?
                    .as_str(),
            )?;
            Ok(Event::PasteBufferDeleted { buffer })
        },
        Rule::pause => {
            let mut pairs = pair.into_inner();
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            Ok(Event::Pause { pane })
        },
        Rule::session_changed => {
            let mut pairs = pair.into_inner();
            let session =
                parse_session_id(pairs.next().ok_or_else(|| anyhow!("missing session id"))?)?;
            let name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            Ok(Event::SessionChanged { session, name })
        },
        Rule::session_renamed => {
            let mut pairs = pair.into_inner();
            let name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            Ok(Event::SessionRenamed { name })
        },
        Rule::session_window_changed => {
            let mut pairs = pair.into_inner();
            let session =
                parse_session_id(pairs.next().ok_or_else(|| anyhow!("missing session id"))?)?;
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::SessionWindowChanged { session, window })
        },
        Rule::sessions_changed => Ok(Event::SessionsChanged),
        Rule::subscription_changed => Ok(Event::SubscriptionChanged),
        Rule::unlinked_window_add => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::UnlinkedWindowAdd { window })
        },
        Rule::unlinked_window_close => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::UnlinkedWindowClose { window })
        },
        Rule::unlinked_window_renamed => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::UnlinkedWindowRenamed { window })
        },
        Rule::window_add => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::WindowAdd { window })
        },
        Rule::window_close => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            Ok(Event::WindowClose { window })
        },
        Rule::window_pane_changed => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            let pane = parse_pane_id(pairs.next().ok_or_else(|| anyhow!("missing pane id"))?)?;
            Ok(Event::WindowPaneChanged { window, pane })
        },
        Rule::window_renamed => {
            let mut pairs = pair.into_inner();
            let window =
                parse_window_id(pairs.next().ok_or_else(|| anyhow!("missing window id"))?)?;
            let name = unvis(
                pairs
                    .next()
                    .ok_or_else(|| anyhow!("missing name"))?
                    .as_str(),
            )?;
            Ok(Event::WindowRenamed { window, name })
        },
        Rule::EOI
        | Rule::any_text
        | Rule::client_name
        | Rule::layout_pane
        | Rule::layout_split_horizontal
        | Rule::layout_split_pane
        | Rule::layout_split_vertical
        | Rule::layout_window
        | Rule::line
        | Rule::line_entire
        | Rule::number
        | Rule::pane_id
        | Rule::session_id
        | Rule::window_id
        | Rule::window_layout
        | Rule::word => anyhow::bail!("Should not reach here"),
    }
}

/// Decode OpenBSD `vis` encoded strings
/// See: https://github.com/tmux/tmux/blob/486ce9b09855ae30a2bf5e576cb6f7ad37792699/compat/unvis.c
pub fn unvis(s: &str) -> anyhow::Result<String> {
    enum State {
        Ground,
        Start,
        Meta,
        Meta1,
        Ctrl(u8),
        Octal2(u8),
        Octal3(u8),
    }

    let mut state = State::Ground;
    let mut result: Vec<u8> = vec![];
    let mut bytes = s.as_bytes().iter();

    fn is_octal(b: u8) -> bool {
        b >= b'0' && b <= b'7'
    }

    fn unvis_byte(b: u8, state: &mut State, result: &mut Vec<u8>) -> anyhow::Result<bool> {
        match state {
            State::Ground => {
                if b == b'\\' {
                    *state = State::Start;
                } else {
                    result.push(b);
                }
            },

            State::Start => {
                match b {
                    b'\\' => {
                        result.push(b'\\');
                        *state = State::Ground;
                    },
                    b'0' | b'1' | b'2' | b'3' | b'4' | b'5' | b'6' | b'7' => {
                        let value = b - b'0';
                        *state = State::Octal2(value);
                    },
                    b'M' => {
                        *state = State::Meta;
                    },
                    b'^' => {
                        *state = State::Ctrl(0);
                    },
                    b'n' => {
                        result.push(b'\n');
                        *state = State::Ground;
                    },
                    b'r' => {
                        result.push(b'\r');
                        *state = State::Ground;
                    },
                    b'b' => {
                        result.push(b'\x08');
                        *state = State::Ground;
                    },
                    b'a' => {
                        result.push(b'\x07');
                        *state = State::Ground;
                    },
                    b'v' => {
                        result.push(b'\x0b');
                        *state = State::Ground;
                    },
                    b't' => {
                        result.push(b'\t');
                        *state = State::Ground;
                    },
                    b'f' => {
                        result.push(b'\x0c');
                        *state = State::Ground;
                    },
                    b's' => {
                        result.push(b' ');
                        *state = State::Ground;
                    },
                    b'E' => {
                        result.push(b'\x1b');
                        *state = State::Ground;
                    },
                    b'\n' => {
                        // Hidden newline
                        // result.push(b'\n');
                        *state = State::Ground;
                    },
                    b'$' => {
                        // Hidden marker
                        *state = State::Ground;
                    },
                    _ => {
                        // Invalid syntax
                        anyhow::bail!("Invalid \\ escape: {}", b);
                    },
                }
            },

            State::Meta => {
                if b == b'-' {
                    *state = State::Meta1;
                } else if b == b'^' {
                    *state = State::Ctrl(0o200);
                } else {
                    anyhow::bail!("invalid \\M escape: {}", b);
                }
            },

            State::Meta1 => {
                result.push(b | 0o200);
                *state = State::Ground;
            },

            State::Ctrl(c) => {
                if b == b'?' {
                    result.push(*c | 0o177);
                } else {
                    result.push((b & 0o37) | *c);
                }
                *state = State::Ground;
            },

            State::Octal2(prior) => {
                if is_octal(b) {
                    // It's the second in a 2 or 3 byte octal sequence
                    let value = (*prior << 3) + (b - b'0');
                    *state = State::Octal3(value);
                } else {
                    // Prior character was a single octal value
                    result.push(*prior);
                    *state = State::Ground;
                    // re-process the current byte
                    return Ok(true);
                }
            },

            State::Octal3(prior) => {
                if is_octal(b) {
                    // It's the third in a 3 byte octal sequence
                    let value = (*prior << 3) + (b - b'0');
                    result.push(value);
                    *state = State::Ground;
                } else {
                    // Prior was a 2-byte octal sequence
                    result.push(*prior);
                    *state = State::Ground;
                    // re-process the current byte
                    return Ok(true);
                }
            },
        }
        // Don't process this byte again
        Ok(false)
    }

    while let Some(&b) = bytes.next() {
        let again = unvis_byte(b, &mut state, &mut result)?;
        if again {
            unvis_byte(b, &mut state, &mut result)?;
        }
    }

    String::from_utf8(result)
        .map_err(|err| anyhow::anyhow!("Unescaped string is not valid UTF8: {}", err))
}

fn parse_layout_pane(pair: Pair<Rule>) -> anyhow::Result<PaneLayout> {
    let mut pairs = pair.into_inner();

    let pane_width = pairs
        .next()
        .ok_or_else(|| anyhow!("wrong pane layout format"))?
        .as_str()
        .parse()?;
    let pane_height = pairs
        .next()
        .ok_or_else(|| anyhow!("wrong pane layout format"))?
        .as_str()
        .parse()?;
    let pane_left = pairs
        .next()
        .ok_or_else(|| anyhow!("wrong pane layout format"))?
        .as_str()
        .parse()?;
    let pane_top = pairs
        .next()
        .ok_or_else(|| anyhow!("wrong pane layout format"))?
        .as_str()
        .parse()?;

    let pane_id = match pairs.next() {
        Some(x) => x.as_str().parse()?,
        None => 0,
    };

    return Ok(PaneLayout {
        pane_id,
        pane_width,
        pane_height,
        pane_left,
        pane_top,
    });
}

fn parse_layout_inner(
    mut pairs: Pairs<Rule>,
    result: &mut Vec<WindowLayout>,
) -> anyhow::Result<Vec<PaneLayout>> {
    let mut stack = Vec::new();

    while let Some(pair) = pairs.next() {
        let rule = pair.as_rule();
        match rule {
            Rule::layout_split_horizontal | Rule::layout_split_vertical => {
                let mut pairs_inner = pair.into_inner();
                let pair = pairs_inner
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("no pairs!?"))?;
                let mut pane = parse_layout_pane(pair)?;

                if result.is_empty() {
                    // Fake one, to flag it is not a TmuxLayout::SinglePane will pop
                    result.push(WindowLayout::SplitHorizontal(vec![]));
                }

                let mut layout_inner = parse_layout_inner(pairs_inner, result)?;

                let last_item = layout_inner
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("wrong layout format"))?;

                pane.pane_id = last_item.pane_id;

                layout_inner.insert(0, pane.clone());

                if let Rule::layout_split_horizontal = rule {
                    result.insert(0, WindowLayout::SplitHorizontal(layout_inner));
                } else {
                    result.insert(0, WindowLayout::SplitVertical(layout_inner));
                }

                stack.push(pane);
            },
            Rule::layout_pane => {
                let pane = parse_layout_pane(pair)?;

                // SinglePane
                if result.is_empty() {
                    result.insert(0, WindowLayout::SinglePane(pane));
                    return Ok(stack);
                }

                stack.push(pane);
            },
            Rule::EOI
            | Rule::any_text
            | Rule::begin
            | Rule::client_detached
            | Rule::client_name
            | Rule::client_session_changed
            | Rule::config_error
            | Rule::r#continue
            | Rule::end
            | Rule::error
            | Rule::exit
            | Rule::extended_output
            | Rule::layout_change
            | Rule::layout_split_pane
            | Rule::layout_window
            | Rule::line
            | Rule::line_entire
            | Rule::message
            | Rule::number
            | Rule::output
            | Rule::pane_id
            | Rule::pane_mode_changed
            | Rule::paste_buffer_changed
            | Rule::paste_buffer_deleted
            | Rule::pause
            | Rule::session_changed
            | Rule::session_id
            | Rule::session_renamed
            | Rule::session_window_changed
            | Rule::sessions_changed
            | Rule::subscription_changed
            | Rule::unlinked_window_add
            | Rule::unlinked_window_close
            | Rule::unlinked_window_renamed
            | Rule::window_add
            | Rule::window_close
            | Rule::window_id
            | Rule::window_layout
            | Rule::window_pane_changed
            | Rule::window_renamed
            | Rule::word => anyhow::bail!("Should not reach here"),
        }
    }

    Ok(stack)
}

pub fn parse_layout(layout: &str) -> anyhow::Result<Vec<WindowLayout>> {
    let mut result = Vec::new();
    let pairs = parser::TmuxParser::parse(Rule::layout_window, layout)?;

    let _ = parse_layout_inner(pairs, &mut result)?;
    if result.len() > 1 {
        let _ = result.pop();
    }

    Ok(result)
}

pub struct Parser {
    buffer: Vec<u8>,
    begun: Option<Guarded>,
}

impl Parser {
    pub fn new() -> Self {
        Self {
            buffer: vec![],
            begun: None,
        }
    }

    pub fn advance_byte(&mut self, c: u8) -> anyhow::Result<Option<Event>> {
        if c == b'\n' {
            self.process_line()
        } else {
            self.buffer.push(c);
            Ok(None)
        }
    }

    pub fn advance_string(&mut self, s: &str) -> anyhow::Result<Vec<Event>> {
        self.advance_bytes(s.as_bytes())
    }

    pub fn advance_bytes(&mut self, bytes: &[u8]) -> anyhow::Result<Vec<Event>> {
        let mut events = vec![];
        for (i, &b) in bytes.iter().enumerate() {
            match self.advance_byte(b) {
                Ok(option_event) => {
                    if let Some(e) = option_event {
                        events.push(e);
                    }
                },
                Err(err) => {
                    // concat remained bytes after digested bytes
                    return Err(anyhow::anyhow!(format!(
                        "{}{}",
                        err,
                        String::from_utf8_lossy(&bytes[i..])
                    )));
                },
            }
        }
        Ok(events)
    }

    fn process_guarded_line(&mut self, line: String) -> anyhow::Result<Option<Event>> {
        let result = match parse_line(&line) {
            Ok(Event::End {
                timestamp,
                number,
                flags,
            }) => {
                if let Some(begun) = self.begun.take() {
                    if begun.timestamp == timestamp
                        && begun.number == number
                        && begun.flags == flags
                    {
                        Some(Event::Guarded(begun))
                    } else {
                        log::error!("mismatched %end; expected {:?} but got {}", begun, line);
                        None
                    }
                } else {
                    log::error!("unexpected %end with no %begin ({})", line);
                    None
                }
            },
            Ok(Event::Error {
                timestamp,
                number,
                flags,
            }) => {
                if let Some(mut begun) = self.begun.take() {
                    if begun.timestamp == timestamp
                        && begun.number == number
                        && begun.flags == flags
                    {
                        begun.error = true;
                        Some(Event::Guarded(begun))
                    } else {
                        log::error!("mismatched %error; expected {:?} but got {}", begun, line);
                        None
                    }
                } else {
                    log::error!("unexpected %error with no %begin ({})", line);
                    None
                }
            },
            _ => {
                let begun = self
                    .begun
                    .as_mut()
                    .ok_or_else(|| anyhow!("missing begun"))?;
                begun.output.push_str(&line);
                begun.output.push('\n');
                None
            },
        };
        self.buffer.clear();
        return Ok(result);
    }

    fn process_line(&mut self) -> anyhow::Result<Option<Event>> {
        if self.buffer.last() == Some(&b'\r') {
            self.buffer.pop();
        }
        let result = match std::str::from_utf8(&self.buffer) {
            Ok(line) => {
                if self.begun.is_some() {
                    let line = line.to_owned();
                    return self.process_guarded_line(line);
                }
                match parse_line(line) {
                    Ok(Event::Begin {
                        timestamp,
                        number,
                        flags,
                    }) => {
                        if self.begun.is_some() {
                            log::error!("expected %end or %error before %begin ({})", line);
                        }
                        self.begun.replace(Guarded {
                            timestamp,
                            number,
                            flags,
                            error: false,
                            output: String::new(),
                        });
                        None
                    },
                    Ok(event) => Some(event),
                    Err(err) => {
                        log::error!("Unrecognized tmux cc line: {}", err);
                        return Err(anyhow::anyhow!(line.to_owned()));
                    },
                }
            },
            Err(err) => {
                log::error!("Failed to parse line from tmux: {}", err);
                None
            },
        };
        self.buffer.clear();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k9::assert_equal as assert_eq;

    #[test]
    fn test_parse_line() {
        assert_eq!(
            Event::Begin {
                timestamp: 12345,
                number: 321,
                flags: 0,
            },
            parse_line("%begin 12345 321 0").unwrap()
        );

        assert_eq!(
            Event::End {
                timestamp: 12345,
                number: 321,
                flags: 0,
            },
            parse_line("%end 12345 321 0").unwrap()
        );
    }

    #[test]
    fn test_parse_sequence() {
        let input = b"%sessions-changed
%pane-mode-changed %0
%begin 1604279270 310 0
stuff
in
here
%end 1604279270 310 0
%window-add @1
%window-close @38
%unlinked-window-close @39
%sessions-changed
%session-changed $1 1
%client-session-changed /dev/pts/5 $1 home
%client-detached /dev/pts/10
%layout-change @1 b25d,80x24,0,0,0
%layout-change @1 cafd,120x29,0,0,0 cafd,120x29,0,0,0 *
%output %1 \\033[1m\\033[7m%\\033[27m\\033[1m\\033[0m    \\015 \\015
%output %1 \\033kwez@cube-localdomain:~\\033\\134\\033]2;wez@cube-localdomain:~\\033\\134
%output %1 \\033]7;file://cube-localdomain/home/wez\\033\\134
%output %1 \\033[K\\033[?2004h
%exit
%exit I said so
%config-error /home/joe/.tmux.conf:1: unknown command: dadsafafasdf
%continue %2
%extended-output %1 \\033[1m\\033[7m%\\033[27m\\033[1m\\033[0m    \\015 \\015
%message message text
%unlinked-window-add @40
%unlinked-window-renamed @41
%paste-buffer-changed just something
%paste-buffer-deleted just something else
%pause %3
%subscription-changed something we don't handle so far
";

        let mut p = Parser::new();
        let events = p.advance_bytes(input).unwrap();
        assert_eq!(
            vec![
                Event::SessionsChanged,
                Event::PaneModeChanged { pane: 0 },
                Event::Guarded(Guarded {
                    timestamp: 1604279270,
                    number: 310,
                    flags: 0,
                    error: false,
                    output: "stuff\nin\nhere\n".to_owned()
                }),
                Event::WindowAdd { window: 1 },
                Event::WindowClose { window: 38 },
                Event::UnlinkedWindowClose { window: 39 },
                Event::SessionsChanged,
                Event::SessionChanged {
                    session: 1,
                    name: "1".to_owned(),
                },
                Event::ClientSessionChanged {
                    client_name: "/dev/pts/5".to_owned(),
                    session: 1,
                    session_name: "home".to_owned()
                },
                Event::ClientDetached {
                    client_name: "/dev/pts/10".to_owned()
                },
                Event::LayoutChange {
                    window: 1,
                    layout: "b25d,80x24,0,0,0".to_owned(),
                    visible_layout: None,
                    raw_flags: None
                },
                Event::LayoutChange {
                    window: 1,
                    layout: "cafd,120x29,0,0,0".to_owned(),
                    visible_layout: Some("cafd,120x29,0,0,0".to_owned()),
                    raw_flags: Some("*".to_owned())
                },
                Event::Output {
                    pane: 1,
                    text: "\x1b[1m\x1b[7m%\x1b[27m\x1b[1m\x1b[0m    \r \r".to_owned()
                },
                Event::Output {
                    pane: 1,
                    text: "\x1bkwez@cube-localdomain:~\x1b\\\x1b]2;wez@cube-localdomain:~\x1b\\"
                        .to_owned()
                },
                Event::Output {
                    pane: 1,
                    text: "\x1b]7;file://cube-localdomain/home/wez\x1b\\".to_owned(),
                },
                Event::Output {
                    pane: 1,
                    text: "\x1b[K\x1b[?2004h".to_owned(),
                },
                Event::Exit { reason: None },
                Event::Exit {
                    reason: Some("I said so".to_owned())
                },
                Event::ConfigError {
                    error: "/home/joe/.tmux.conf:1: unknown command: dadsafafasdf".to_owned()
                },
                Event::Continue { pane: 2 },
                Event::ExtendedOutput {
                    pane: 1,
                    text: "\x1b[1m\x1b[7m%\x1b[27m\x1b[1m\x1b[0m    \r \r".to_owned()
                },
                Event::Message {
                    message: "message text".to_owned()
                },
                Event::UnlinkedWindowAdd { window: 40 },
                Event::UnlinkedWindowRenamed { window: 41 },
                Event::PasteBufferChanged {
                    buffer: "just something".to_owned()
                },
                Event::PasteBufferDeleted {
                    buffer: "just something else".to_owned()
                },
                Event::Pause { pane: 3 },
                Event::SubscriptionChanged,
            ],
            events
        );
    }

    #[test]
    fn test_parse_layout() {
        let layout_case1 = "158x40,0,0,72".to_string();
        let layout_case2 = "158x40,0,0[158x20,0,0,69,158x19,0,21{79x19,0,21,70,78x19,80,21[78x9,80,21,71,78x9,80,31,73]}]".to_string();
        let layout_case3 = "158x40,0,0{79x40,0,0[79x20,0,0,74,79x19,0,21{39x19,0,21,76,39x19,40,21,77}],78x40,80,0,75}".to_string();

        let mut layout = parse_layout(&layout_case1).unwrap();
        let l = layout.pop().unwrap();
        assert!(if let WindowLayout::SinglePane(p) = l {
            assert_eq!(p.pane_width, 158);
            assert_eq!(p.pane_height, 40);
            assert_eq!(p.pane_left, 0);
            assert_eq!(p.pane_top, 0);
            assert_eq!(p.pane_id, 72);
            true
        } else {
            false
        });

        layout = parse_layout(&layout_case2).unwrap();
        assert!(matches!(&layout[0], WindowLayout::SplitVertical(_x)));
        assert!(matches!(&layout[1], WindowLayout::SplitHorizontal(_x)));
        assert!(matches!(&layout[2], WindowLayout::SplitVertical(_x)));
        layout = parse_layout(&layout_case3).unwrap();
        assert!(matches!(&layout[0], WindowLayout::SplitHorizontal(_x)));
        assert!(matches!(&layout[1], WindowLayout::SplitVertical(_x)));
        assert!(matches!(&layout[2], WindowLayout::SplitHorizontal(_x)));
    }
}
