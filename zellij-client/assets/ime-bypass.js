/**
 * Desktop IME-bypass input handling.
 *
 * Works around xterm.js's composition-based character drop under active IMEs
 * (ibus/fcitx on Linux, etc.). When an IME is attached, every keystroke comes
 * through with `ev.key === "Process"` wrapped in a phantom composition, and
 * xterm.js sometimes fails to forward the character under fast typing. The
 * textarea's standardized `input` event is used as the authoritative source of
 * the character, and the textarea is cleared so xterm.js's composition handler
 * sees no diff and cannot double-emit.
 *
 *   - Non-IME typing (ev.key !== "Process") is untouched.
 *   - Real IME composition (Japanese/Chinese) emits `insertCompositionText`,
 *     paste emits `insertFromPaste` — both ignored here, handled by xterm.js.
 */

/**
 * Install the IME-bypass input listener exactly once per page load. The
 * send-function reference is refreshed on every call so the real WebSocket
 * sender replaces the initial placeholder (setupInputHandlers runs twice).
 */
export function installImeBypass(term, sendFunction) {
    if (typeof window.__zjImeBypass === "undefined") {
        window.__zjImeBypass = {
            installed: false,
            sendFn: sendFunction,
            lastKeyWasProcess: false,
        };
    }
    window.__zjImeBypass.sendFn = sendFunction;

    if (window.__zjImeBypass.installed) {
        return;
    }
    window.__zjImeBypass.installed = true;
    const state = window.__zjImeBypass;

    // Non-IME keys report their actual key value and go through xterm.js's
    // normal keyboard path; only "Process" keystrokes are intercepted below.
    document.addEventListener(
        "keydown",
        (ev) => {
            state.lastKeyWasProcess = ev.key === "Process";
        },
        true
    );

    // The xterm.js textarea is created asynchronously by term.open(); poll
    // briefly until it's available, then attach the capture-phase listener.
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
                    // Clear so xterm.js's composition diff finds no text to
                    // emit at compositionend — prevents double-send.
                    ev.target.value = "";
                    state.lastKeyWasProcess = false;
                }
            },
            true
        );
    };
    attach();
}
