/**
 * Mobile soft-keyboard capture and toggling.
 *
 * Owns the `window.__zjSoftKbd*` globals and `window.__zjSyncInactiveCursorStyle`
 * (the latter is also re-invoked by websockets.js after config updates).
 */

/**
 * Heuristic match for "this device probably has only a soft keyboard". Order
 * matters — the cheap matchMedia hit fires first on real mobile browsers.
 *   - `pointer: coarse`   — iOS/Android Chrome/Safari, DevTools mobile emulation.
 *   - `maxTouchPoints>0`  — WebViews / in-app browsers lacking pointer:coarse.
 *   - `ontouchstart`      — older WebViews lacking maxTouchPoints.
 *   - UA regex            — final fallback for quirky media-query support.
 */
export function isCoarsePointerDevice() {
    if (window.matchMedia && window.matchMedia("(pointer: coarse)").matches) {
        return true;
    }
    if (typeof navigator !== "undefined" && navigator.maxTouchPoints > 0) {
        return true;
    }
    if ("ontouchstart" in window) {
        return true;
    }
    return /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent);
}

/**
 * Capture mobile soft-keyboard keystrokes for the terminal.
 *
 * The OS soft keyboard only sends keystrokes into a text-input it owns, and
 * then runs prediction/autocorrect/composition on top — none of which a
 * terminal wants. Three stacked tricks keep the keyboard happy while
 * neutralizing its smart layer:
 *
 *   1. A hidden 1×1 transparent text-input gives the OS keyboard a target
 *      (display:none / visibility:hidden would close the keyboard).
 *   2. `type="password"` disables prediction/autocorrect/composition on every
 *      mobile keyboard vendor by policy — each tap becomes one character,
 *      dispatched immediately, never revisited. This eliminates the hidden
 *      composition buffer behind the original Tab+Enter doubling bug.
 *   3. A closed shadow root hides the field from password managers, whose
 *      content scripts query `input[type=password]` and cannot pierce it.
 *
 * Keystroke pipeline (the "RestoreDOM" pattern): mobile keyboards don't
 * reliably fire `keydown` for soft keys, so characters are read by diffing the
 * input's `.value` on every `input` event against the previous value
 * (longest-common-prefix/suffix → N deletes + M inserts → pty bytes). The field
 * is seeded with NBSP padding around a mid-string caret so a single typed char
 * yields a "delete 0, insert 1" diff, and Backspace on an "empty" line has
 * padding to delete (Firefox Android / GBoard no-op backspace on truly empty
 * content). Value is reset to BASELINE on Enter to bound growth and refill fodder.
 *
 * Side-effect: with a closed shadow root, `document.activeElement` returns the
 * shadow *host*, never the inner input — so focus checks read the mirrored
 * `state.isFocused` flag kept in sync by focus/blur listeners.
 *
 * Idempotent across the dual setupInputHandlers invocations; subsequent calls
 * just refresh `state.sendFn`.
 */
export function installSoftKeyboardCapture(term, sendFunction) {
    if (!isCoarsePointerDevice()) {
        return;
    }
    if (typeof window.__zjSoftKbdCapture === "undefined") {
        window.__zjSoftKbdCapture = {
            installed: false,
            sendFn: sendFunction,
            element: null,
            // Mirrors the input's focus state — `document.activeElement` returns
            // the shadow host (not the input) for a closed shadow root, so
            // identity checks against `element` can't work. Kept in sync by the
            // focus/blur listeners below.
            isFocused: false,
        };
    }
    window.__zjSoftKbdCapture.sendFn = sendFunction;

    if (window.__zjSoftKbdCapture.installed) {
        return;
    }
    window.__zjSoftKbdCapture.installed = true;
    const state = window.__zjSoftKbdCapture;

    // The capture input steals focus on every tap, so xterm.js would render the
    // cursor with `cursorInactiveStyle` ("outline") — unreadable on a small
    // viewport. Mirror the active style into the inactive slot so the cursor
    // stays visible while typing. Exposed on `window` so websockets.js can
    // re-mirror after a SetConfig changes `cursorStyle`.
    const syncInactiveCursorStyle = () => {
        const active = term.options.cursorStyle || "block";
        term.options.cursorInactiveStyle = active;
    };
    syncInactiveCursorStyle();
    window.__zjSyncInactiveCursorStyle = syncInactiveCursorStyle;

    // NBSP padding with the caret in the middle: a single typed char lands
    // between padding chars ("delete 0, insert 1" diff), and backspace on an
    // "empty" line has padding to delete. 8 chars is comfortable headroom.
    const PADDING_CHAR = " ";
    const PADDING_LEN = 8;
    const CARET_OFFSET = PADDING_LEN / 2;
    const BASELINE = PADDING_CHAR.repeat(PADDING_LEN);

    // Construction order: host → closed shadow → input. The host carries the
    // 1×1 transparent positioning so no popover has anything visible to anchor
    // to. `mode:"closed"` means `host.shadowRoot` is null from outside; the only
    // reference is `captureShadow`, held in this closure.
    const captureHost = document.createElement("div");
    captureHost.id = "zj-mobile-capture-host";
    captureHost.style.cssText =
        "position:fixed;top:0;left:0;" +
        "width:1px;height:1px;" +
        "opacity:0;pointer-events:none;" +
        "overflow:hidden;";
    captureHost.setAttribute("aria-hidden", "true");
    document.body.appendChild(captureHost);
    const captureShadow = captureHost.attachShadow({ mode: "closed" });

    const div = document.createElement("input");
    div.id = "zj-mobile-capture";
    div.type = "password";
    div.setAttribute("autocomplete", "new-password");
    div.setAttribute("autocorrect", "off");
    div.setAttribute("autocapitalize", "off");
    div.setAttribute("spellcheck", "false");
    div.setAttribute("inputmode", "text");
    div.setAttribute("aria-hidden", "true");
    // Belt-and-braces opt-outs for managers that pierce shadow roots.
    div.setAttribute("data-1p-ignore", "true");
    div.setAttribute("data-lpignore", "true");
    div.setAttribute("data-bwignore", "true");
    div.setAttribute("data-form-type", "other");
    div.setAttribute("data-dashlane-ignore", "true");
    div.tabIndex = -1;
    // 1×1 transparent, pointer-events disabled so touches pass through to the
    // terminal. Stays focusable (display:none / visibility:hidden would close
    // the keyboard).
    div.style.cssText =
        "position:fixed;top:0;left:0;" +
        "width:1px;height:1px;" +
        "opacity:0;pointer-events:none;" +
        "border:0;padding:0;margin:0;" +
        "background:transparent;color:transparent;" +
        "caret-color:transparent;outline:none;" +
        "white-space:pre;overflow:hidden;" +
        "user-select:text;-webkit-user-select:text;";
    captureShadow.appendChild(div);
    state.element = div;

    div.addEventListener("focus", () => { state.isFocused = true; });
    div.addEventListener("blur", () => { state.isFocused = false; });

    const setCaretToMiddle = () => {
        try {
            const pos = Math.min(CARET_OFFSET, div.value.length);
            div.setSelectionRange(pos, pos);
        } catch (_) {
            // setSelectionRange throws if not yet focusable; best-effort.
        }
    };

    const resetBaseline = () => {
        div.value = BASELINE;
        setCaretToMiddle();
    };

    resetBaseline();

    // Tight per-character dedupe (8 ms, well under any key-autorepeat). Also
    // collapses the IME's composition-active backspace pair: a single backspace
    // tap fires TWO `deleteContentBackward` events ~5 ms apart, which the dedupe
    // merges into one `\x7f`. The matching value-refill below keeps the padding
    // topped up so the value never drains to empty (which would stop input
    // events firing and strand the deletion).
    let lastCh = null;
    let lastChAt = 0;
    const DEDUPE_MS = 8;
    const dispatchCh = (ch) => {
        const now = performance.now();
        if (ch === lastCh && now - lastChAt < DEDUPE_MS) {
            return;
        }
        lastCh = ch;
        lastChAt = now;
        state.sendFn(ch);
    };

    // Diff previous value `a` vs post-mutation `b` by walking the longest common
    // prefix then suffix. Returns chars deleted from the middle of `a` and the
    // substring inserted in the middle of `b`. Padding chars are stripped from
    // the insertion in case the IME swaps padding for itself.
    const diff = (a, b) => {
        const minLen = Math.min(a.length, b.length);
        let prefixLen = 0;
        while (prefixLen < minLen && a[prefixLen] === b[prefixLen]) {
            prefixLen++;
        }
        let suffixLen = 0;
        const maxSuffix = minLen - prefixLen;
        while (
            suffixLen < maxSuffix &&
            a[a.length - 1 - suffixLen] === b[b.length - 1 - suffixLen]
        ) {
            suffixLen++;
        }
        const deletedCount = a.length - prefixLen - suffixLen;
        let inserted = b.slice(prefixLen, b.length - suffixLen);
        if (inserted.indexOf(PADDING_CHAR) !== -1) {
            inserted = inserted.split(PADDING_CHAR).join("");
        }
        return { deletedCount, inserted };
    };

    // With `type="password"` each `input` event is one user-driven mutation
    // (insert, backspace, or paste); the value accumulates until Enter resets it.
    let lastText = BASELINE;

    div.addEventListener("input", (ev) => {
        const current = div.value;
        if (current === lastText) {
            return;
        }
        const { deletedCount, inserted } = diff(lastText, current);
        for (let i = 0; i < deletedCount; i++) {
            dispatchCh("\x7f");
        }
        for (const ch of inserted) {
            dispatchCh(ch);
        }
        lastText = current;

        // Refill backspace fodder after each deletion: the composition-active
        // backspace pair consumes two cells of `div.value` per user tap (while
        // the dedupe emits one `\x7f`), so the padding drains at 2× the plugin's
        // rate. Resetting to BASELINE after every delete keeps it topped up.
        // Pure inserts don't drain padding and are left alone. Programmatic
        // value assignment does not fire `input`, so this doesn't re-enter.
        if (ev.inputType === "deleteContentBackward") {
            div.value = BASELINE;
            lastText = BASELINE;
            setCaretToMiddle();
        }
    });

    // Special keys don't always produce observable value mutations; dispatch
    // their escape sequences directly. Backspace goes through the input diff.
    // Enter also resets to baseline (bounds growth + refills fodder).
    div.addEventListener("keydown", (ev) => {
        switch (ev.key) {
            case "Enter":
                ev.preventDefault();
                state.sendFn("\r");
                div.value = BASELINE;
                lastText = BASELINE;
                setCaretToMiddle();
                return;
            case "Tab":
                ev.preventDefault();
                state.sendFn("\t");
                return;
            case "Escape":
                ev.preventDefault();
                state.sendFn("\x1b");
                return;
            case "ArrowUp":
                ev.preventDefault();
                state.sendFn("\x1b[A");
                return;
            case "ArrowDown":
                ev.preventDefault();
                state.sendFn("\x1b[B");
                return;
            case "ArrowRight":
                ev.preventDefault();
                state.sendFn("\x1b[C");
                return;
            case "ArrowLeft":
                ev.preventDefault();
                state.sendFn("\x1b[D");
                return;
        }
    });

    // Mobile browsers ignore programmatic focus() outside a user gesture, so the
    // plugin's startup set_soft_keyboard(true) can't summon the keyboard before
    // the first touch. Recover by focusing the capture on every gesture:
    //   - `click` bubbles AFTER xterm.js's mousedown textarea-focus, so focus()
    //     here wins the race inside the still-active gesture window.
    //   - `touchend` covers taps that never resolve to a click (long-press, scroll).
    //   - `pointerdown` (capture phase) is a fallback for differing focus synthesis.
    // Gated on `__zjSoftKbdEnabled` so a user who disabled the keyboard isn't
    // re-summoned. A blur+focus pair re-summons after the keyboard was dismissed
    // externally (Android back button hides it without blurring).
    const ensureCaptureFocused = () => {
        if (!window.__zjSoftKbdEnabled) {
            return;
        }
        if (state.isFocused) {
            div.blur();
        }
        try {
            div.focus({ preventScroll: true });
        } catch (_) {
            div.focus();
        }
    };
    window.addEventListener("click", ensureCaptureFocused, { passive: true });
    window.addEventListener("touchend", ensureCaptureFocused, { passive: true });
    window.addEventListener("pointerdown", ensureCaptureFocused, {
        capture: true,
        passive: true,
    });
}

/**
 * Mark xterm.js's textarea `inputmode="none"` on touch devices so the soft
 * keyboard does not auto-pop on every tap. The textarea still processes
 * hardware keypresses; only the on-screen popup is suppressed. Idempotent;
 * skipped on `pointer: fine` devices.
 */
export function suppressSoftKeyboardOnTouch(term) {
    if (window.__zjSoftKbdSuppressed) {
        return;
    }
    if (!isCoarsePointerDevice()) {
        return;
    }
    window.__zjSoftKbdSuppressed = true;
    // Default to "soft keyboard wanted" so the first-touch focus fallback works
    // even if the plugin's startup set_soft_keyboard(true) never arrives (e.g.
    // plain ssh sessions). The 2-finger toggle flips this back to false.
    window.__zjSoftKbdEnabled = true;
    const apply = () => {
        const ta = term && term._core && term._core.textarea;
        if (!ta) {
            setTimeout(apply, 50);
            return;
        }
        ta.setAttribute("inputmode", "none");
    };
    apply();
}

/**
 * Set soft-keyboard visibility on touch devices. Called from the 2-finger tap
 * (via toggleSoftKeyboard) and from websockets.js's SetSoftKeyboard handler.
 * No-op on desktops (suppression was never installed) and when already in the
 * requested state. Summoning is routed through the dedicated capture input so
 * keystrokes flow through the value-diff path; xterm.js's textarea keeps
 * `inputmode="none"` and only takes focus when the soft keyboard is off, so
 * hardware-keyboard typing still uses xterm.js's normal keydown handler.
 */
export function setSoftKeyboard(term, on) {
    if (!isCoarsePointerDevice()) {
        return;
    }
    const ta = term && term._core && term._core.textarea;
    if (!ta) {
        return;
    }
    if (window.__zjSoftKbdEnabled === on) {
        return;
    }
    window.__zjSoftKbdEnabled = on;
    const capture = window.__zjSoftKbdCapture && window.__zjSoftKbdCapture.element;
    if (on) {
        if (capture) {
            capture.focus();
        } else {
            // Fallback if the capture input failed to install: summon via the
            // textarea, accepting the keydown-reliability tradeoff.
            ta.removeAttribute("inputmode");
            ta.focus();
        }
    } else {
        if (capture) {
            capture.blur();
        }
        // Re-focus xterm.js's textarea (inputmode="none" prevents re-summoning)
        // so attached hardware keyboards keep driving its keydown handler.
        ta.setAttribute("inputmode", "none");
        ta.focus();
    }
}

/**
 * Flip the soft-keyboard state. Called from the 2-finger tap gesture handler.
 */
export function toggleSoftKeyboard(term) {
    setSoftKeyboard(term, !window.__zjSoftKbdEnabled);
}
