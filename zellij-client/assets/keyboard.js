/**
 * Keyboard handling functions
 */

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
    let key_code = ev.key.charCodeAt(0);
    send_ansi_key(`\x1b[${key_code};${modifier_string}u`);
}
