/**
 * Keyboard handling functions
 */

/**
 * Map from `KeyboardEvent.key` (the JS-side name of a non-printable key)
 * to the codepoint the Kitty keyboard protocol expects in its
 * functional-key CSI u encoding.
 *
 * Without this table the previous implementation did
 * `ev.key.charCodeAt(0)` for every key — which works for printable keys
 * like "a" (97) but produces garbage for multi-character names:
 *   - "Backspace".charCodeAt(0) → 66 ('B')
 *   - "Delete".charCodeAt(0)    → 68 ('D')
 *   - "Enter".charCodeAt(0)     → 69 ('E')
 *
 * The host then receives e.g. `\x1b[66;9u` for Cmd+Backspace and the
 * shell ignores it. Visible symptom: Cmd+Backspace, Cmd+Delete, etc.
 * appear to do nothing in the web client.
 *
 * Reference: https://sw.kovidgoyal.net/kitty/keyboard-protocol/#functional-key-definitions
 */
const FUNCTIONAL_KEY_CODES = {
    // Whitespace / control
    Escape: 27,
    Enter: 13,
    Tab: 9,
    Backspace: 127,
    // Editing
    Insert: 0xe001,
    Delete: 0xe002,
    // Cursor movement
    ArrowLeft: 0xe004,
    ArrowRight: 0xe005,
    ArrowUp: 0xe006,
    ArrowDown: 0xe007,
    PageUp: 0xe008,
    PageDown: 0xe009,
    Home: 0xe00a,
    End: 0xe00b,
    // Mode keys
    CapsLock: 0xe00c,
    ScrollLock: 0xe00d,
    NumLock: 0xe00e,
    PrintScreen: 0xe00f,
    Pause: 0xe010,
    ContextMenu: 0xe011, // Menu key
    // Function keys (Kitty PUA assignments)
    F1: 0xe012,
    F2: 0xe013,
    F3: 0xe014,
    F4: 0xe015,
    F5: 0xe016,
    F6: 0xe017,
    F7: 0xe018,
    F8: 0xe019,
    F9: 0xe01a,
    F10: 0xe01b,
    F11: 0xe01c,
    F12: 0xe01d,
};

/**
 * Encode a keyboard event into kitty protocol ANSI escape sequence
 * @param {KeyboardEvent} ev - The keyboard event to encode
 * @param {function} send_ansi_key - Function to send the ANSI key sequence
 */
export function encode_kitty_key(ev, send_ansi_key) {
    let shift_value = 1;
    let alt_value = 2;
    let ctrl_value = 4;
    let super_value = 8;
    let modifier_string = 1;
    if (ev.shiftKey) {
        modifier_string += shift_value;
    }
    if (ev.altKey) {
        modifier_string += alt_value;
    }
    if (ev.ctrlKey) {
        modifier_string += ctrl_value;
    }
    if (ev.metaKey) {
        modifier_string += super_value;
    }
    // For named functional keys, use the Kitty PUA codepoint; otherwise
    // (printable keys), take the unicode codepoint of the character.
    // `String.fromCodePoint(...)` round-trip is not needed since we just
    // want the integer code.
    let key_code;
    if (Object.prototype.hasOwnProperty.call(FUNCTIONAL_KEY_CODES, ev.key)) {
        key_code = FUNCTIONAL_KEY_CODES[ev.key];
    } else if (ev.key.length === 1) {
        // Printable key (single Unicode unit). `codePointAt(0)` handles
        // BMP and surrogate pairs equivalently for length-1 strings.
        key_code = ev.key.codePointAt(0);
    } else {
        // Unknown multi-character key name. Don't emit a garbage
        // sequence — fall through silently. The browser may still emit
        // the key via xterm.js's normal path; we just don't add a
        // Kitty wrapping on top.
        return;
    }
    send_ansi_key(`\x1b[${key_code};${modifier_string}u`);
}
