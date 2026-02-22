pub mod actions;
pub mod cli_assets;
pub mod command;
pub mod config;
pub mod keybinds;
pub mod layout;
pub mod mouse;
pub mod options;
pub mod permission;
pub mod plugins;
pub mod theme;
pub mod web_client;

#[cfg(not(target_family = "wasm"))]
pub use not_wasm::*;

#[cfg(not(target_family = "wasm"))]
mod not_wasm {
    use crate::{
        data::{BareKey, InputMode, KeyModifier, KeyWithModifier, ModeInfo, PluginCapabilities},
        envs,
        ipc::ClientAttributes,
    };
    use termwiz::input::{InputEvent, InputParser, KeyCode, KeyEvent, Modifiers};

    use super::keybinds::Keybinds;
    use std::collections::BTreeSet;

    /// Creates a [`ModeInfo`] struct indicating the current [`InputMode`] and its keybinds
    /// (as pairs of [`String`]s).
    pub fn get_mode_info(
        mode: InputMode,
        attributes: &ClientAttributes,
        capabilities: PluginCapabilities,
        keybinds: &Keybinds,
        base_mode: Option<InputMode>,
    ) -> ModeInfo {
        let keybinds = keybinds.to_keybinds_vec();
        let session_name = envs::get_session_name().ok();

        ModeInfo {
            mode,
            base_mode,
            keybinds,
            style: attributes.style,
            capabilities,
            session_name,
            editor: None,
            shell: None,
            web_clients_allowed: None,
            web_sharing: None,
            currently_marking_pane_group: None,
            is_web_client: None,
            web_server_ip: None,
            web_server_port: None,
            web_server_capability: None,
        }
    }

    // used for parsing keys to plugins
    pub fn parse_keys(input_bytes: &[u8]) -> Vec<KeyWithModifier> {
        let mut ret = vec![];
        let mut input_parser = InputParser::new(); // this is the termwiz InputParser
        let maybe_more = false;
        let parse_input_event = |input_event: InputEvent| {
            if let InputEvent::Key(key_event) = input_event {
                ret.push(cast_termwiz_key(key_event, input_bytes, None));
            }
        };
        input_parser.parse(input_bytes, parse_input_event, maybe_more);
        ret
    }

    fn key_is_bound(key: &KeyWithModifier, keybinds: &Keybinds, mode: &InputMode) -> bool {
        keybinds
            .get_actions_for_key_in_mode(mode, key)
            .map_or(false, |actions| !actions.is_empty())
    }

    // FIXME: This is an absolutely cursed function that should be destroyed as soon
    // as an alternative that doesn't touch zellij-tile can be developed...
    pub fn cast_termwiz_key(
        event: KeyEvent,
        raw_bytes: &[u8],
        keybinds_mode: Option<(&Keybinds, &InputMode)>,
    ) -> KeyWithModifier {
        let termwiz_modifiers = event.modifiers;

        // *** THIS IS WHERE WE SHOULD WORK AROUND ISSUES WITH TERMWIZ ***
        if raw_bytes == [8] {
            return KeyWithModifier::new(BareKey::Char('h')).with_ctrl_modifier();
        };

        if raw_bytes == [10] {
            if let Some((keybinds, mode)) = keybinds_mode {
                let ctrl_j = KeyWithModifier::new(BareKey::Char('j')).with_ctrl_modifier();
                if key_is_bound(&ctrl_j, keybinds, mode) {
                    return ctrl_j;
                }
            }
        }
        let mut modifiers = BTreeSet::new();
        if termwiz_modifiers.contains(Modifiers::CTRL) {
            modifiers.insert(KeyModifier::Ctrl);
        }
        if termwiz_modifiers.contains(Modifiers::ALT) {
            modifiers.insert(KeyModifier::Alt);
        }
        if termwiz_modifiers.contains(Modifiers::SHIFT) {
            modifiers.insert(KeyModifier::Shift);
        }

        match event.key {
            KeyCode::Char(c) => {
                if c == '\0' {
                    // NUL character, probably ctrl-space
                    KeyWithModifier::new(BareKey::Char(' ')).with_ctrl_modifier()
                } else {
                    KeyWithModifier::new_with_modifiers(BareKey::Char(c), modifiers)
                }
            },
            KeyCode::Backspace => {
                KeyWithModifier::new_with_modifiers(BareKey::Backspace, modifiers)
            },
            KeyCode::LeftArrow | KeyCode::ApplicationLeftArrow => {
                KeyWithModifier::new_with_modifiers(BareKey::Left, modifiers)
            },
            KeyCode::RightArrow | KeyCode::ApplicationRightArrow => {
                KeyWithModifier::new_with_modifiers(BareKey::Right, modifiers)
            },
            KeyCode::UpArrow | KeyCode::ApplicationUpArrow => {
                KeyWithModifier::new_with_modifiers(BareKey::Up, modifiers)
            },
            KeyCode::DownArrow | KeyCode::ApplicationDownArrow => {
                KeyWithModifier::new_with_modifiers(BareKey::Down, modifiers)
            },
            KeyCode::Home => KeyWithModifier::new_with_modifiers(BareKey::Home, modifiers),
            KeyCode::End => KeyWithModifier::new_with_modifiers(BareKey::End, modifiers),
            KeyCode::PageUp => KeyWithModifier::new_with_modifiers(BareKey::PageUp, modifiers),
            KeyCode::PageDown => KeyWithModifier::new_with_modifiers(BareKey::PageDown, modifiers),
            KeyCode::Tab => KeyWithModifier::new_with_modifiers(BareKey::Tab, modifiers),
            KeyCode::Delete => KeyWithModifier::new_with_modifiers(BareKey::Delete, modifiers),
            KeyCode::Insert => KeyWithModifier::new_with_modifiers(BareKey::Insert, modifiers),
            KeyCode::Function(n) => KeyWithModifier::new_with_modifiers(BareKey::F(n), modifiers),
            KeyCode::Escape => KeyWithModifier::new_with_modifiers(BareKey::Esc, modifiers),
            KeyCode::Enter => KeyWithModifier::new_with_modifiers(BareKey::Enter, modifiers),
            _ => KeyWithModifier::new(BareKey::Esc),
        }
    }

    /// Convert a crossterm `MouseEvent` into a zellij `MouseEvent`.
    ///
    /// Crossterm's mouse events are richer than termwiz's (they distinguish
    /// Down/Up/Drag/Moved directly), so no state tracking is needed.
    #[cfg(windows)]
    pub fn from_crossterm_mouse(event: crossterm::event::MouseEvent) -> super::mouse::MouseEvent {
        use super::mouse;
        use crossterm::event::{KeyModifiers, MouseButton as CButton, MouseEventKind};

        let position = crate::position::Position::new(event.row as i32, event.column);
        let modifiers = event.modifiers;
        let shift = modifiers.contains(KeyModifiers::SHIFT);
        let alt = modifiers.contains(KeyModifiers::ALT);
        let ctrl = modifiers.contains(KeyModifiers::CONTROL);

        let (event_type, left, right, middle, wheel_up, wheel_down) = match event.kind {
            MouseEventKind::Down(CButton::Left) => (
                mouse::MouseEventType::Press,
                true,
                false,
                false,
                false,
                false,
            ),
            MouseEventKind::Down(CButton::Right) => (
                mouse::MouseEventType::Press,
                false,
                true,
                false,
                false,
                false,
            ),
            MouseEventKind::Down(CButton::Middle) => (
                mouse::MouseEventType::Press,
                false,
                false,
                true,
                false,
                false,
            ),
            MouseEventKind::Up(CButton::Left) => (
                mouse::MouseEventType::Release,
                true,
                false,
                false,
                false,
                false,
            ),
            MouseEventKind::Up(CButton::Right) => (
                mouse::MouseEventType::Release,
                false,
                true,
                false,
                false,
                false,
            ),
            MouseEventKind::Up(CButton::Middle) => (
                mouse::MouseEventType::Release,
                false,
                false,
                true,
                false,
                false,
            ),
            MouseEventKind::Drag(CButton::Left) => (
                mouse::MouseEventType::Motion,
                true,
                false,
                false,
                false,
                false,
            ),
            MouseEventKind::Drag(CButton::Right) => (
                mouse::MouseEventType::Motion,
                false,
                true,
                false,
                false,
                false,
            ),
            MouseEventKind::Drag(CButton::Middle) => (
                mouse::MouseEventType::Motion,
                false,
                false,
                true,
                false,
                false,
            ),
            MouseEventKind::Moved => (
                mouse::MouseEventType::Motion,
                false,
                false,
                false,
                false,
                false,
            ),
            MouseEventKind::ScrollUp => (
                mouse::MouseEventType::Press,
                false,
                false,
                false,
                true,
                false,
            ),
            MouseEventKind::ScrollDown => (
                mouse::MouseEventType::Press,
                false,
                false,
                false,
                false,
                true,
            ),
            MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => (
                mouse::MouseEventType::Motion,
                false,
                false,
                false,
                false,
                false,
            ),
        };

        mouse::MouseEvent {
            event_type,
            left,
            right,
            middle,
            wheel_up,
            wheel_down,
            shift,
            alt,
            ctrl,
            position,
        }
    }

    /// Convert a crossterm `KeyEvent` into a zellij `KeyWithModifier` plus synthesized raw VT
    /// bytes suitable for PTY pass-through. Returns `None` for key codes we don't handle
    /// (e.g. media keys, bare modifier presses).
    #[cfg(windows)]
    pub fn cast_crossterm_key(
        event: crossterm::event::KeyEvent,
    ) -> Option<(KeyWithModifier, Vec<u8>)> {
        use crossterm::event::{KeyCode as CKeyCode, KeyModifiers};

        let ct_mods = event.modifiers;
        let mut modifiers = BTreeSet::new();
        if ct_mods.contains(KeyModifiers::CONTROL) {
            modifiers.insert(KeyModifier::Ctrl);
        }
        if ct_mods.contains(KeyModifiers::ALT) {
            modifiers.insert(KeyModifier::Alt);
        }
        if ct_mods.contains(KeyModifiers::SHIFT) {
            modifiers.insert(KeyModifier::Shift);
        }
        if ct_mods.contains(KeyModifiers::SUPER) {
            modifiers.insert(KeyModifier::Super);
        }

        let has_ctrl = ct_mods.contains(KeyModifiers::CONTROL);
        let has_alt = ct_mods.contains(KeyModifiers::ALT);

        let (bare_key, raw_bytes) = match event.code {
            CKeyCode::Char(c) => {
                let bytes = if has_ctrl && has_alt && c.is_ascii_alphabetic() {
                    vec![0x1b, (c.to_ascii_lowercase() as u8) & 0x1f]
                } else if has_ctrl && c.is_ascii_alphabetic() {
                    vec![(c.to_ascii_lowercase() as u8) & 0x1f]
                } else if has_alt {
                    let mut b = vec![0x1b];
                    let mut buf = [0u8; 4];
                    b.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    b
                } else {
                    let mut buf = [0u8; 4];
                    c.encode_utf8(&mut buf).as_bytes().to_vec()
                };
                (BareKey::Char(c), bytes)
            },
            CKeyCode::Enter => (BareKey::Enter, vec![0x0d]),
            CKeyCode::Tab => (BareKey::Tab, vec![0x09]),
            CKeyCode::BackTab => {
                modifiers.insert(KeyModifier::Shift);
                (BareKey::Tab, vec![0x1b, b'[', b'Z'])
            },
            CKeyCode::Backspace => (BareKey::Backspace, vec![0x7f]),
            CKeyCode::Esc => (BareKey::Esc, vec![0x1b]),
            CKeyCode::Left => (BareKey::Left, vec![0x1b, b'[', b'D']),
            CKeyCode::Right => (BareKey::Right, vec![0x1b, b'[', b'C']),
            CKeyCode::Up => (BareKey::Up, vec![0x1b, b'[', b'A']),
            CKeyCode::Down => (BareKey::Down, vec![0x1b, b'[', b'B']),
            CKeyCode::Home => (BareKey::Home, vec![0x1b, b'[', b'H']),
            CKeyCode::End => (BareKey::End, vec![0x1b, b'[', b'F']),
            CKeyCode::PageUp => (BareKey::PageUp, b"\x1b[5~".to_vec()),
            CKeyCode::PageDown => (BareKey::PageDown, b"\x1b[6~".to_vec()),
            CKeyCode::Delete => (BareKey::Delete, b"\x1b[3~".to_vec()),
            CKeyCode::Insert => (BareKey::Insert, b"\x1b[2~".to_vec()),
            CKeyCode::F(n) => {
                let bytes = match n {
                    1 => b"\x1bOP".to_vec(),
                    2 => b"\x1bOQ".to_vec(),
                    3 => b"\x1bOR".to_vec(),
                    4 => b"\x1bOS".to_vec(),
                    5 => b"\x1b[15~".to_vec(),
                    6 => b"\x1b[17~".to_vec(),
                    7 => b"\x1b[18~".to_vec(),
                    8 => b"\x1b[19~".to_vec(),
                    9 => b"\x1b[20~".to_vec(),
                    10 => b"\x1b[21~".to_vec(),
                    11 => b"\x1b[23~".to_vec(),
                    12 => b"\x1b[24~".to_vec(),
                    _ => vec![],
                };
                (BareKey::F(n), bytes)
            },
            CKeyCode::CapsLock => (BareKey::CapsLock, vec![]),
            CKeyCode::ScrollLock => (BareKey::ScrollLock, vec![]),
            CKeyCode::NumLock => (BareKey::NumLock, vec![]),
            CKeyCode::PrintScreen => (BareKey::PrintScreen, vec![]),
            CKeyCode::Pause => (BareKey::Pause, vec![]),
            CKeyCode::Menu => (BareKey::Menu, vec![]),
            CKeyCode::Null => {
                // ctrl-space
                return Some((
                    KeyWithModifier::new(BareKey::Char(' ')).with_ctrl_modifier(),
                    vec![0x00],
                ));
            },
            // Media keys, bare modifier presses, KeypadBegin — skip
            _ => return None,
        };

        Some((
            KeyWithModifier::new_with_modifiers(bare_key, modifiers),
            raw_bytes,
        ))
    }
}
