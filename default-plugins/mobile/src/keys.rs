//! Key serialization for the mobile plugin's typing-mode.
//!
//! Plugins run in wasm, so the `serialize_kitty` / `serialize_non_kitty`
//! helpers on `KeyWithModifier` (gated to `not(target_family = "wasm")`
//! because they pull in the termwiz vendored encoder) are not
//! available. This module provides a small, hand-rolled subset for
//! typing-mode passthrough: enough to forward plain characters,
//! Ctrl-letter combos, Alt-letter combos, and the common navigation
//! keys an interactive program (vim, less, REPLs) cares about.
//!
//! The encoding follows the legacy xterm convention (no kitty
//! disambiguation, no modifyOtherKeys), which is what the receiving
//! pty's terminal emulator already speaks by default.
//!
//! When a key cannot be sensibly serialized (e.g. a pure modifier
//! release, an F-key with an unsupported index), an empty `Vec` is
//! returned and the caller swallows it.

use zellij_tile::prelude::*;

/// Translate a `KeyWithModifier` arriving from the Zellij key event
/// stream into the bytes that should be written to the pane's pty.
pub fn serialize_key(key: &KeyWithModifier) -> Vec<u8> {
    let ctrl = key.key_modifiers.contains(&KeyModifier::Ctrl);
    let alt = key.key_modifiers.contains(&KeyModifier::Alt);
    let shift = key.key_modifiers.contains(&KeyModifier::Shift);

    match key.bare_key {
        BareKey::Char(c) => serialize_char(c, ctrl, alt, shift),
        BareKey::Enter => prefix_alt(alt, b"\r"),
        BareKey::Tab => {
            if shift {
                // CSI Z is the standard back-tab.
                prefix_alt(alt, b"\x1b[Z")
            } else {
                prefix_alt(alt, b"\t")
            }
        },
        BareKey::Backspace => {
            if ctrl {
                prefix_alt(alt, b"\x08")
            } else {
                prefix_alt(alt, b"\x7f")
            }
        },
        BareKey::Esc => prefix_alt(alt, b"\x1b"),
        BareKey::Up => arrow_or_modified(b'A', ctrl, alt, shift),
        BareKey::Down => arrow_or_modified(b'B', ctrl, alt, shift),
        BareKey::Right => arrow_or_modified(b'C', ctrl, alt, shift),
        BareKey::Left => arrow_or_modified(b'D', ctrl, alt, shift),
        BareKey::Home => arrow_or_modified(b'H', ctrl, alt, shift),
        BareKey::End => arrow_or_modified(b'F', ctrl, alt, shift),
        BareKey::Insert => tilde_or_modified(2, ctrl, alt, shift),
        BareKey::Delete => tilde_or_modified(3, ctrl, alt, shift),
        BareKey::PageUp => tilde_or_modified(5, ctrl, alt, shift),
        BareKey::PageDown => tilde_or_modified(6, ctrl, alt, shift),
        BareKey::F(n) => serialize_function_key(n, ctrl, alt, shift),
        BareKey::CapsLock
        | BareKey::ScrollLock
        | BareKey::NumLock
        | BareKey::PrintScreen
        | BareKey::Pause
        | BareKey::Menu => Vec::new(),
    }
}

fn serialize_char(c: char, ctrl: bool, alt: bool, _shift: bool) -> Vec<u8> {
    if ctrl {
        if let Some(byte) = ctrl_byte(c) {
            return prefix_alt(alt, &[byte]);
        }
        // Fall through to plain encoding for chars that have no Ctrl
        // mapping (e.g. punctuation outside the small Ctrl table).
    }
    let mut buf = [0u8; 4];
    let encoded = c.encode_utf8(&mut buf).as_bytes();
    prefix_alt(alt, encoded)
}

/// Standard xterm Ctrl-letter mapping. `Ctrl+a` → 0x01 … `Ctrl+z` →
/// 0x1a. A handful of punctuation keys also map (Ctrl+@, Ctrl+[, …)
/// per the legacy encoding.
fn ctrl_byte(c: char) -> Option<u8> {
    let lower = c.to_ascii_lowercase();
    match lower {
        '@' | ' ' => Some(0x00),
        c if ('a'..='z').contains(&c) => Some((c as u8) - b'`'),
        '[' => Some(0x1b),
        '\\' => Some(0x1c),
        ']' => Some(0x1d),
        '^' => Some(0x1e),
        '_' | '?' => Some(0x1f),
        _ => None,
    }
}

/// CSI A/B/C/D/H/F arrow/home/end serialization. With any modifier
/// active, emit the modified `CSI 1 ; <mods> <ch>` form.
fn arrow_or_modified(letter: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    if ctrl || shift {
        let modifier = encode_modifiers(ctrl, alt, shift);
        format!("\x1b[1;{}{}", modifier, letter as char).into_bytes()
    } else {
        // Alt as the "ESC-prefix" convention works for the bare form;
        // for modified arrows the modifier byte already carries Alt.
        prefix_alt(alt, &[0x1b, b'[', letter])
    }
}

/// `CSI <n> ~` form for Insert/Delete/PageUp/PageDown. With modifiers
/// emit `CSI <n> ; <mods> ~`.
fn tilde_or_modified(n: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    if ctrl || shift {
        let modifier = encode_modifiers(ctrl, alt, shift);
        format!("\x1b[{};{}~", n, modifier).into_bytes()
    } else {
        prefix_alt(alt, format!("\x1b[{}~", n).as_bytes())
    }
}

/// Function keys. F1..F4 use the SS3 (`ESC O P/Q/R/S`) form; F5+ use
/// `CSI <n> ~`. Modifier-encoded forms follow xterm.
fn serialize_function_key(n: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    let any_mod = ctrl || shift;
    match n {
        1..=4 => {
            let letter = match n {
                1 => 'P',
                2 => 'Q',
                3 => 'R',
                4 => 'S',
                _ => unreachable!(),
            };
            if any_mod {
                let modifier = encode_modifiers(ctrl, alt, shift);
                format!("\x1b[1;{}{}", modifier, letter).into_bytes()
            } else {
                prefix_alt(alt, format!("\x1bO{}", letter).as_bytes())
            }
        },
        5 => format_tilde_fkey(15, ctrl, alt, shift),
        6 => format_tilde_fkey(17, ctrl, alt, shift),
        7 => format_tilde_fkey(18, ctrl, alt, shift),
        8 => format_tilde_fkey(19, ctrl, alt, shift),
        9 => format_tilde_fkey(20, ctrl, alt, shift),
        10 => format_tilde_fkey(21, ctrl, alt, shift),
        11 => format_tilde_fkey(23, ctrl, alt, shift),
        12 => format_tilde_fkey(24, ctrl, alt, shift),
        _ => Vec::new(),
    }
}

fn format_tilde_fkey(n: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    if ctrl || shift {
        let modifier = encode_modifiers(ctrl, alt, shift);
        format!("\x1b[{};{}~", n, modifier).into_bytes()
    } else {
        prefix_alt(alt, format!("\x1b[{}~", n).as_bytes())
    }
}

/// xterm modifier byte: `1 + (shift) + 2*(alt) + 4*(ctrl)`. Only used
/// in the modified CSI forms.
fn encode_modifiers(ctrl: bool, alt: bool, shift: bool) -> u8 {
    let mut m = 1u8;
    if shift {
        m += 1;
    }
    if alt {
        m += 2;
    }
    if ctrl {
        m += 4;
    }
    m
}

/// Prepend ESC for Alt-prefixed sequences. Most terminals interpret
/// `ESC <ch>` as Alt+<ch> regardless of whether `<ch>` is itself an
/// escape sequence, so this is safe to apply uniformly.
fn prefix_alt(alt: bool, body: &[u8]) -> Vec<u8> {
    if alt {
        let mut out = Vec::with_capacity(body.len() + 1);
        out.push(0x1b);
        out.extend_from_slice(body);
        out
    } else {
        body.to_vec()
    }
}

#[cfg(test)]
mod tests {
    //! Sanity checks confirming `serialize_key` covers everything the
    //! in-plugin keyboard emits — F-keys and Ctrl/Alt-letter combos.
    use super::*;
    use std::collections::BTreeSet;

    fn key_with(bare: BareKey, mods: &[KeyModifier]) -> KeyWithModifier {
        let mut set = BTreeSet::new();
        for m in mods {
            set.insert(*m);
        }
        KeyWithModifier {
            bare_key: bare,
            key_modifiers: set,
        }
    }

    #[test]
    fn f1_unmodified() {
        assert_eq!(serialize_key(&key_with(BareKey::F(1), &[])), b"\x1bOP");
    }

    #[test]
    fn f12_unmodified() {
        assert_eq!(serialize_key(&key_with(BareKey::F(12), &[])), b"\x1b[24~");
    }

    #[test]
    fn ctrl_c() {
        assert_eq!(
            serialize_key(&key_with(BareKey::Char('c'), &[KeyModifier::Ctrl])),
            vec![0x03]
        );
    }

    #[test]
    fn alt_x() {
        assert_eq!(
            serialize_key(&key_with(BareKey::Char('x'), &[KeyModifier::Alt])),
            vec![0x1b, b'x']
        );
    }
}
