import { encode_kitty_key } from "./keyboard.js";
import { isMac } from "./utils.js";

export function installCustomKeyHandler(term, sendFunction) {
    term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown") {
            if (ev.key == "V" && ev.ctrlKey && ev.shiftKey) {
                return;
            }
            if (isMac() && ev.key == "v" && ev.metaKey) {
                return;
            }
            if (hasModifiersToHandle(ev)) {
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
            // xterm.js mishandles Alt+Arrow; send Alt-modified SGR sequences directly:
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
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
        }
        return true;
    });
}

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
