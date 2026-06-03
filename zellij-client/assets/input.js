import { installImeBypass } from "./ime-bypass.js";
import {
    installSoftKeyboardCapture,
    suppressSoftKeyboardOnTouch,
} from "./soft-keyboard.js";
import { installCustomKeyHandler } from "./key-handler.js";
import { installMouseHandlers } from "./mouse.js";
import { createPinchController } from "./pinch.js";
import { installTouchGestures } from "./touch.js";

// Re-exported so websockets.js can keep importing it from ./input.js.
export { setSoftKeyboard } from "./soft-keyboard.js";

/**
 * Set up all input handlers for the terminal.
 *
 * NOTE: this runs twice (see index.js) — first with a placeholder no-op sender,
 * then with the real WebSocket sender. The soft-keyboard / IME installers are
 * self-guarded and just refresh their sender on the second call; the custom key
 * handler is replaced. The mouse/touch/pinch/data registrations below are NOT
 * guarded, so the second call adds a second set bound to the real sender (the
 * first set is bound to the no-op). This is pre-existing behavior, preserved
 * here intentionally — de-duplicating it would be a separate cleanup.
 *
 * @param {Terminal} term - The terminal instance
 * @param {FitAddon} fitAddon - The xterm.js fit addon (unused here; retained for
 *     call-site compatibility)
 * @param {function} sendFunction - Function to send data through the WebSocket
 */
export function setupInputHandlers(term, fitAddon, sendFunction) {
    installImeBypass(term, sendFunction);
    installSoftKeyboardCapture(term, sendFunction);
    suppressSoftKeyboardOnTouch(term);
    installCustomKeyHandler(term, sendFunction);

    const terminalElement = document.getElementById("terminal");
    installMouseHandlers(term, terminalElement, sendFunction);

    const pinch = createPinchController(term);
    installTouchGestures({ term, terminalElement, sendFunction, pinch });

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
