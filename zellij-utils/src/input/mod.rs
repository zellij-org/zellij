pub mod actions;
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
}
