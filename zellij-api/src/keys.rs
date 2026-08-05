//! Turns human-readable key names from the API (`"ctrl-c"`, `"enter"`, `"f5"`)
//! into the bytes a terminal application expects on its stdin.
//!
//! We deliberately emit raw bytes rather than `KeyWithModifier`, because the
//! bytes are what a pane's PTY consumes and they survive being addressed at a
//! specific pane via `Action::WriteToPaneId`.

/// Parse one key name into the byte sequence to write to a pane.
///
/// Accepted forms:
/// - named keys: `enter`, `tab`, `esc`, `space`, `backspace`, `delete`,
///   `insert`, `home`, `end`, `pageup`, `pagedown`, `up`/`down`/`left`/`right`
/// - function keys: `f1` .. `f12`
/// - a single character: `a`, `Z`, `/`
/// - modifiers, combinable and separated by `-` or `+`:
///   `ctrl-c`, `alt-f`, `shift-tab`, `ctrl-alt-delete`
pub fn key_to_bytes(name: &str) -> Result<Vec<u8>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("empty key name".to_string());
    }

    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;

    // Split modifiers off the front, walking the original string. The final
    // segment is the key itself, so a literal "-" or "+" as the key still
    // works.
    //
    // This deliberately does not walk a lowercased copy in parallel: case
    // conversion can change a string's byte length (`İ` lowercases to two
    // chars), and offsets taken from one string then applied to the other
    // would slice through a character. `-` and `+` are ASCII, so splitting the
    // original is always on a character boundary.
    let mut rest = trimmed;
    loop {
        let split = rest
            .find('-')
            .or_else(|| rest.find('+'))
            .filter(|idx| *idx > 0 && *idx + 1 < rest.len());
        let Some(idx) = split else { break };
        let (prefix, remainder) = rest.split_at(idx);
        let remainder = &remainder[1..];
        match prefix.to_lowercase().as_str() {
            "ctrl" | "control" | "c" => ctrl = true,
            "alt" | "meta" | "opt" | "option" | "m" => alt = true,
            "shift" | "s" => shift = true,
            _ => break,
        }
        rest = remainder;
    }

    let base_original = rest;
    let base_lowered = rest.to_lowercase();
    let base = base_lowered.as_str();
    let mut bytes = match base {
        "enter" | "return" | "cr" => vec![b'\r'],
        "newline" | "lf" => vec![b'\n'],
        "tab" if !shift => vec![b'\t'],
        "tab" => b"\x1b[Z".to_vec(), // shift-tab is back-tab
        "esc" | "escape" => vec![0x1b],
        "space" => vec![b' '],
        "backspace" | "bs" => vec![0x7f],
        "delete" | "del" => b"\x1b[3~".to_vec(),
        "insert" | "ins" => b"\x1b[2~".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" | "pgup" => b"\x1b[5~".to_vec(),
        "pagedown" | "pgdn" | "pgdown" => b"\x1b[6~".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => b"\x1b[15~".to_vec(),
        "f6" => b"\x1b[17~".to_vec(),
        "f7" => b"\x1b[18~".to_vec(),
        "f8" => b"\x1b[19~".to_vec(),
        "f9" => b"\x1b[20~".to_vec(),
        "f10" => b"\x1b[21~".to_vec(),
        "f11" => b"\x1b[23~".to_vec(),
        "f12" => b"\x1b[24~".to_vec(),
        other => {
            let mut chars = other.chars();
            let (Some(ch), None) = (chars.next(), chars.next()) else {
                return Err(format!("unknown key name: '{}'", name));
            };
            // Use the caller's original casing for plain characters, so "A"
            // stays uppercase even though we lowercased for lookup.
            let ch = base_original.chars().next().unwrap_or(ch);
            let ch = if shift {
                ch.to_uppercase().next().unwrap_or(ch)
            } else {
                ch
            };
            if ctrl {
                // Control codes: ctrl-a == 0x01 ... ctrl-z == 0x1a, plus the
                // handful of punctuation control codes.
                let upper = ch.to_ascii_uppercase();
                let code = match upper {
                    'A'..='Z' => (upper as u8) - b'A' + 1,
                    '@' => 0,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    '^' => 0x1e,
                    '_' => 0x1f,
                    ' ' => 0,
                    _ => return Err(format!("no control code for key '{}'", name)),
                };
                ctrl = false; // consumed
                vec![code]
            } else {
                let mut buf = [0u8; 4];
                ch.encode_utf8(&mut buf).as_bytes().to_vec()
            }
        },
    };

    if ctrl {
        // ctrl applied to a named key (e.g. ctrl-left) — emit the xterm
        // modified-key form where we know it, otherwise report it rather than
        // silently dropping the modifier.
        bytes = match base {
            "up" => b"\x1b[1;5A".to_vec(),
            "down" => b"\x1b[1;5B".to_vec(),
            "right" => b"\x1b[1;5C".to_vec(),
            "left" => b"\x1b[1;5D".to_vec(),
            "home" => b"\x1b[1;5H".to_vec(),
            "end" => b"\x1b[1;5F".to_vec(),
            "delete" | "del" => b"\x1b[3;5~".to_vec(),
            _ => return Err(format!("ctrl modifier not supported for key '{}'", name)),
        };
    }

    if alt {
        // Alt is transmitted as ESC followed by the key.
        let mut prefixed = vec![0x1b];
        prefixed.extend(bytes);
        bytes = prefixed;
    }

    Ok(bytes)
}

/// Encode a whole sequence of key names into one byte stream.
pub fn keys_to_bytes(keys: &[String]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for key in keys {
        out.extend(key_to_bytes(key)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_characters_pass_through() {
        assert_eq!(key_to_bytes("a").unwrap(), b"a");
        assert_eq!(key_to_bytes("Z").unwrap(), b"Z");
        assert_eq!(key_to_bytes("/").unwrap(), b"/");
    }

    #[test]
    fn named_keys_use_their_terminal_sequences() {
        assert_eq!(key_to_bytes("enter").unwrap(), b"\r");
        assert_eq!(key_to_bytes("tab").unwrap(), b"\t");
        assert_eq!(key_to_bytes("esc").unwrap(), vec![0x1b]);
        assert_eq!(key_to_bytes("backspace").unwrap(), vec![0x7f]);
        assert_eq!(key_to_bytes("up").unwrap(), b"\x1b[A");
        assert_eq!(key_to_bytes("f5").unwrap(), b"\x1b[15~");
    }

    #[test]
    fn ctrl_maps_to_control_codes() {
        assert_eq!(key_to_bytes("ctrl-c").unwrap(), vec![0x03]);
        assert_eq!(key_to_bytes("ctrl-a").unwrap(), vec![0x01]);
        assert_eq!(key_to_bytes("ctrl-z").unwrap(), vec![0x1a]);
        // Capitalised and alternate spellings agree.
        assert_eq!(key_to_bytes("CTRL-C").unwrap(), vec![0x03]);
        assert_eq!(key_to_bytes("control+c").unwrap(), vec![0x03]);
    }

    #[test]
    fn alt_prefixes_with_escape() {
        assert_eq!(key_to_bytes("alt-f").unwrap(), vec![0x1b, b'f']);
        assert_eq!(key_to_bytes("alt-enter").unwrap(), vec![0x1b, b'\r']);
    }

    #[test]
    fn shift_tab_is_back_tab() {
        assert_eq!(key_to_bytes("shift-tab").unwrap(), b"\x1b[Z");
    }

    #[test]
    fn shift_uppercases_characters() {
        assert_eq!(key_to_bytes("shift-a").unwrap(), b"A");
    }

    #[test]
    fn combined_modifiers_stack() {
        // ESC prefix, then the control code.
        assert_eq!(key_to_bytes("ctrl-alt-c").unwrap(), vec![0x1b, 0x03]);
        assert_eq!(key_to_bytes("ctrl-left").unwrap(), b"\x1b[1;5D");
    }

    #[test]
    fn unknown_keys_are_reported_rather_than_dropped() {
        assert!(key_to_bytes("nonsense").is_err());
        assert!(key_to_bytes("").is_err());
        // A modifier we cannot faithfully encode must fail loudly.
        assert!(key_to_bytes("ctrl-f5").is_err());
    }

    #[test]
    fn hostile_key_names_are_rejected_not_fatal() {
        // Key names arrive from the network, and the parser splits modifiers by
        // byte offset across a lowercase conversion — which can change a
        // string's length (`İ` lowercases to two chars). Anything unparseable
        // must come back as an error, never a panic.
        let nasty = [
            "İ",
            "ctrl-İ",
            "c-İ",
            "ctrl-alt-İ",
            "ß",
            "ctrl-ß",
            "-",
            "+",
            "--",
            "ctrl-",
            "-a",
            "ctrl+",
            "ctrl-alt-",
            "🎉",
            "ctrl-🎉",
            "shift-🎉",
            "\u{0130}\u{0131}",
            "ctrl-\u{0130}\u{0131}",
            " ",
            "\t",
            "\u{feff}",
            "a\u{0300}",
            "ctrl-a\u{0300}",
        ];
        for name in nasty {
            // The assertion is simply that this returns rather than unwinds.
            let _ = key_to_bytes(name);
        }
    }

    #[test]
    fn sequences_concatenate() {
        let keys: Vec<String> = ["s", "u", "d", "o", "enter"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(keys_to_bytes(&keys).unwrap(), b"sudo\r");
    }
}
