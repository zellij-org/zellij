import { installImeBypass } from "./ime-bypass.js";
import {
    installSoftKeyboardCapture,
    suppressSoftKeyboardOnTouch,
} from "./soft-keyboard.js";
import { installCustomKeyHandler } from "./key-handler.js";
import { installMouseHandlers } from "./mouse.js";
import { createPinchController } from "./pinch.js";
import { installTouchGestures } from "./touch.js";

export { setSoftKeyboard } from "./soft-keyboard.js";

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
