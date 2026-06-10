use zellij_tile::prelude::*;

pub fn serialize_key(key: &KeyWithModifier) -> Vec<u8> {
    let ctrl = key.key_modifiers.contains(&KeyModifier::Ctrl);
    let alt = key.key_modifiers.contains(&KeyModifier::Alt);
    let shift = key.key_modifiers.contains(&KeyModifier::Shift);

    match key.bare_key {
        BareKey::Char(c) => serialize_char(c, ctrl, alt, shift),
        BareKey::Enter => prefix_alt(alt, b"\r"),
        BareKey::Tab => {
            if shift {
                let back_tab = b"\x1b[Z";
                prefix_alt(alt, back_tab)
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
    }
    let mut buf = [0u8; 4];
    let encoded = c.encode_utf8(&mut buf).as_bytes();
    prefix_alt(alt, encoded)
}

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

fn arrow_or_modified(letter: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    if ctrl || shift {
        let modifier = encode_modifiers(ctrl, alt, shift);
        format!("\x1b[1;{}{}", modifier, letter as char).into_bytes()
    } else {
        prefix_alt(alt, &[0x1b, b'[', letter])
    }
}

fn tilde_or_modified(n: u8, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    if ctrl || shift {
        let modifier = encode_modifiers(ctrl, alt, shift);
        format!("\x1b[{};{}~", n, modifier).into_bytes()
    } else {
        prefix_alt(alt, format!("\x1b[{}~", n).as_bytes())
    }
}

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
