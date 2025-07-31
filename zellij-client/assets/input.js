/**
 * Input handling for terminal events
 */

import { encode_kitty_key } from "./keyboard.js";
import { isMac } from "./utils.js";

/**
 * Set up all input handlers for the terminal
 * @param {Terminal} term - The terminal instance
 * @param {function} sendFunction - Function to send data through WebSocket
 */
export function setupInputHandlers(term, sendFunction) {
    // Mouse tracking state
    let prev_col = 0;
    let prev_row = 0;

    // Custom key event handler
    term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown") {
            if (ev.key == "V" && ev.ctrlKey && ev.shiftKey) {
                // pass ctrl-shift-v onwards so that paste is interpreted by xterm.js
                return;
            }
            if (isMac() && ev.key == "v" && ev.metaKey) {
                // pass cmd-v onwards so that paste is interpreted by xterm.js
                return;
            }

            let modifiers_count = 0;
            let shift_keycode = 16;
            let alt_keycode = 17;
            let ctrl_keycode = 18;
            if (ev.altKey) {
                modifiers_count += 1;
            }
            if (ev.ctrlKey) {
                modifiers_count += 1;
            }
            if (ev.shiftKey) {
                modifiers_count += 1;
            }
            if (ev.metaKey) {
                modifiers_count += 1;
            }
            if (
                (modifiers_count > 1 || ev.metaKey) &&
                ev.keyCode != shift_keycode &&
                ev.keyCode != alt_keycode &&
                ev.keyCode != ctrl_keycode
            ) {
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
            // workarounds for https://github.com/xtermjs/xterm.js/blob/41e8ae395937011d6bf6c7cb618b851791aed395/src/common/input/Keyboard.ts#L158
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
                // these are not properly handled by xterm.js, so we bypass it and encode them as kitty to make things easier
                ev.preventDefault();
                encode_kitty_key(ev, sendFunction);
                return false;
            }
        }
        return true;
    });

    // Mouse movement handler
    let terminal_element = document.getElementById("terminal");
    terminal_element.addEventListener("mousemove", function (event) {
        // this is a hack around: https://github.com/xtermjs/xterm.js/issues/1062
        // in short, xterm.js doesn't listen to mousemove at all and so even though
        // we send it a request for AnyEvent mouse handling, we don't get motion events in return
        // here we use some internal functions in a hopefully non-destructive way to calculate the
        // columns/rows to send from the x/y coordinates - it's safe to always send these because Zellij
        // always requests mouse AnyEvent handling
        if (event.buttons == 0) {
            // this means no mouse buttons are pressed and this is just a mouse movement
            let { col, row } = term._core._mouseService.getMouseReportCoords(
                event,
                terminal_element
            );
            if (prev_col != col || prev_row != row) {
                sendFunction(`\x1b[<35;${col + 1};${row + 1}M`);
            }
            prev_col = col;
            prev_row = row;
        }
    });

    // Context menu handler
    document.addEventListener("contextmenu", function (event) {
        if (event.altKey) {
            // this is so that when the user does an alt-right-click to ungroup panes, the context menu will not appear
            event.preventDefault();
        }
    });

    // Terminal data handlers
    term.onData((data) => {
        sendFunction(data);
    });

    term.onBinary((data) => {
        const buffer = new Uint8Array(data.length);
        for (let i = 0; i < data.length; ++i) {
            buffer[i] = data.charCodeAt(i) & 255;
        }
        sendFunction(buffer);
    });
}
