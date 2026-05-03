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
    const isAndroidClient = /Android/i.test(navigator.userAgent);

    // Mouse tracking state
    let prev_col = 0;
    let prev_row = 0;

    // Work around xterm.js's composition-based character drop under active IMEs
    // (ibus/fcitx on Linux, etc.). When an IME is attached, every keystroke comes
    // through with ev.key === "Process" and a phantom compositionstart/end wraps
    // the keystroke. xterm.js sometimes fails to forward the character through
    // its composition path under fast typing. We bypass that path only for this
    // case: use the textarea's standardized `input` event as the authoritative
    // source of the character, and clear the textarea so xterm.js's composition
    // handler sees no diff and cannot double-emit.
    //
    // - Non-IME typing (ev.key !== "Process") is untouched: xterm.js's keyboard
    //   handler still emits onData directly, our listener does not trigger.
    // - Real IME composition (Japanese/Chinese) emits `insertCompositionText`,
    //   not `insertText` — our filter ignores it; xterm.js handles it as today.
    // - Paste fires `insertFromPaste` — also ignored here.
    installImeBypass(term, sendFunction);

    // Custom key event handler
    term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown") {
            if (
                isAndroidClient &&
                ev.key === "Enter" &&
                !ev.ctrlKey &&
                !ev.altKey &&
                !ev.metaKey
            ) {
                // Android physical keyboards often commit pending composition text on Enter.
                // Clear the hidden textarea and send a single carriage return directly.
                ev.preventDefault();
                if (term.textarea) {
                    term.textarea.value = "";
                }
                sendFunction("\r");
                return false;
            }
            if (ev.key == "V" && ev.ctrlKey && ev.shiftKey) {
                // pass ctrl-shift-v onwards so that paste is interpreted by xterm.js
                return;
            }
            if (isMac() && ev.key == "v" && ev.metaKey) {
                // pass cmd-v onwards so that paste is interpreted by xterm.js
                return;
            }
            if (hasModifiersToHandle(ev)) {
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

    // Touch scroll handler (convert vertical swipes to mouse wheel events)
    let last_touch_y = null;
    let pending_scroll = 0;
    const touch_scroll_threshold = 24;
    const sendWheelEvent = (direction, touch) => {
        let { col, row } = term._core._mouseService.getMouseReportCoords(
            { clientX: touch.clientX, clientY: touch.clientY },
            terminal_element
        );
        const button = direction < 0 ? 65 : 64; // inverted: swipe up => wheel down
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
    };

    terminal_element.addEventListener(
        "touchstart",
        (event) => {
            if (event.touches.length > 0) {
                last_touch_y = event.touches[0].clientY;
                pending_scroll = 0;
            }
        },
        { passive: true }
    );

    terminal_element.addEventListener(
        "touchmove",
        (event) => {
            if (event.touches.length === 0 || last_touch_y === null) {
                return;
            }
            event.preventDefault();
            const touch = event.touches[0];
            const delta = touch.clientY - last_touch_y;
            last_touch_y = touch.clientY;
            pending_scroll += delta;
            while (pending_scroll <= -touch_scroll_threshold) {
                sendWheelEvent(-1, touch);
                pending_scroll += touch_scroll_threshold;
            }
            while (pending_scroll >= touch_scroll_threshold) {
                sendWheelEvent(1, touch);
                pending_scroll -= touch_scroll_threshold;
            }
        },
        { passive: false }
    );

    terminal_element.addEventListener(
        "touchend",
        () => {
            last_touch_y = null;
            pending_scroll = 0;
        },
        { passive: true }
    );

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

/**
 * Install the IME-bypass input listener exactly once per page load.
 * The send-function reference is refreshed on every call so the real
 * WebSocket sender (installed after initWebSockets) replaces the initial
 * placeholder — see index.js where setupInputHandlers is called twice.
 */
function installImeBypass(term, sendFunction) {
    if (typeof window.__zjImeBypass === "undefined") {
        window.__zjImeBypass = {
            installed: false,
            sendFn: sendFunction,
            lastKeyWasProcess: false,
        };
    }
    // Always point at the most-recently-provided sender.
    window.__zjImeBypass.sendFn = sendFunction;

    if (window.__zjImeBypass.installed) {
        return;
    }
    window.__zjImeBypass.installed = true;
    const state = window.__zjImeBypass;

    // Track whether the most recent keydown was IME-intercepted. Non-IME keys
    // report their actual key value ("a", "Enter", "ArrowLeft", ...) and go
    // through xterm.js's normal keyboard path; we do not want to interfere.
    document.addEventListener(
        "keydown",
        (ev) => {
            state.lastKeyWasProcess = ev.key === "Process";
        },
        true
    );

    // The xterm.js textarea is created asynchronously by term.open(). Poll
    // briefly until it's available, then attach our capture-phase input
    // listener.
    const attach = () => {
        const ta = term && term._core && term._core.textarea;
        if (!ta) {
            setTimeout(attach, 50);
            return;
        }
        ta.addEventListener(
            "input",
            (ev) => {
                if (
                    state.lastKeyWasProcess &&
                    ev.inputType === "insertText" &&
                    !ev.isComposing &&
                    ev.data
                ) {
                    state.sendFn(ev.data);
                    // Clear so xterm.js's composition diff at compositionend
                    // finds no text to emit — prevents double-send.
                    ev.target.value = "";
                    state.lastKeyWasProcess = false;
                }
            },
            true
        );
    };
    attach();
}

/**
 * Check if a key event has modifiers and is not a modifier key itself
 * @param {KeyboardEvent} ev - The keyboard event
 * @returns {boolean} - True if the key has modifiers that need special handling
 */
function hasModifiersToHandle(ev) {
    // Use key property for simpler modifier key detection
    const MODIFIER_KEYS = ["Shift", "Control", "Alt", "Meta"];

    // Count active modifiers
    const modifiers_count = [
        ev.altKey,
        ev.ctrlKey,
        ev.shiftKey,
        ev.metaKey,
    ].filter(Boolean).length;

    // Check if we have multiple modifiers or meta key, and it's not a modifier key itself
    const isModifierKey = MODIFIER_KEYS.includes(ev.key);
    return (modifiers_count > 1 || ev.metaKey) && !isModifierKey;
}
