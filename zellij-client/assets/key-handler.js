/**
 * Hardware keyboard handling via xterm.js's custom key-event handler.
 */

import { encode_kitty_key } from "./keyboard.js";
import { isMac } from "./utils.js";

/**
 * Install the custom keydown handler. Lets paste shortcuts through to xterm.js,
 * routes multi-modifier combos through the kitty encoder, and works around
 * xterm.js keys it mishandles (alt-arrows, alt +/-/=). Calling this again
 * replaces the single handler (rebinding the sender), which is how the second
 * setupInputHandlers invocation swaps in the real WebSocket sender.
 */
export function installCustomKeyHandler(term, sendFunction) {
    term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown") {
            if (ev.key == "V" && ev.ctrlKey && ev.shiftKey) {
                // pass ctrl-shift-v onwards so xterm.js interprets the paste
                return;
            }
            if (isMac() && ev.key == "v" && ev.metaKey) {
                // pass cmd-v onwards so xterm.js interprets the paste
                return;
            }
            if (hasModifiersToHandle(ev)) {
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
            // Workarounds for keys xterm.js mishandles:
            // https://github.com/xtermjs/xterm.js/blob/41e8ae395937011d6bf6c7cb618b851791aed395/src/common/input/Keyboard.ts#L158
            if (ev.key == "ArrowLeft" && ev.altKey) {
                ev.preventDefault();
                sendFunction("\x1b[1;3D");
                return false;
            }
            if (ev.key == "ArrowRight" && ev.altKey) {
                ev.preventDefault();
                sendFunction("\x1b[1;3C");
                return false;
            }
            if (ev.key == "ArrowUp" && ev.altKey) {
                ev.preventDefault();
                sendFunction("\x1b[1;3A");
                return false;
            }
            if (ev.key == "ArrowDown" && ev.altKey) {
                ev.preventDefault();
                sendFunction("\x1b[1;3B");
                return false;
            }
            if (
                (ev.key == "=" && ev.altKey) ||
                (ev.key == "+" && ev.altKey) ||
                (ev.key == "-" && ev.altKey)
            ) {
                // not properly handled by xterm.js, so encode as kitty
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
        }
        return true;
    });
}

/**
 * True if the event carries modifiers that need special handling and is not a
 * modifier key itself.
 */
function hasModifiersToHandle(ev) {
    const MODIFIER_KEYS = ["Shift", "Control", "Alt", "Meta"];
    const modifiers_count = [
        ev.altKey,
        ev.ctrlKey,
        ev.shiftKey,
        ev.metaKey,
    ].filter(Boolean).length;
    const isModifierKey = MODIFIER_KEYS.includes(ev.key);
    return (modifiers_count > 1 || ev.metaKey) && !isModifierKey;
}
