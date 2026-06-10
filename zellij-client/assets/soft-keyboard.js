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

export function installSoftKeyboardCapture(term, sendFunction) {
    if (!isCoarsePointerDevice()) {
        return;
    }
    if (typeof window.__zjSoftKbdCapture === "undefined") {
        window.__zjSoftKbdCapture = {
            installed: false,
            sendFn: sendFunction,
            element: null,
            // document.activeElement returns the shadow host (not the input) for a
            // closed shadow root, so focus state must be mirrored manually.
            isFocused: false,
        };
    }
    window.__zjSoftKbdCapture.sendFn = sendFunction;

    if (window.__zjSoftKbdCapture.installed) {
        return;
    }
    window.__zjSoftKbdCapture.installed = true;
    const state = window.__zjSoftKbdCapture;

    // The capture input steals focus on every tap, so xterm.js renders the cursor
    // with cursorInactiveStyle; mirror the active style so it stays visible.
    const syncInactiveCursorStyle = () => {
        const active = term.options.cursorStyle || "block";
        term.options.cursorInactiveStyle = active;
    };
    syncInactiveCursorStyle();
    window.__zjSyncInactiveCursorStyle = syncInactiveCursorStyle;

    // Firefox Android / GBoard no-op backspace on truly empty content, so keep the
    // input padded with the caret in the middle to give backspace something to delete.
    // Must NOT be a normal space: diff() strips PADDING_CHAR out of the inserted text,
    // which would swallow a genuinely typed space (U+0020). U+00A0 is non-typeable.
    const PADDING_CHAR = "\u00a0";
    const PADDING_LEN = 8;
    const CARET_OFFSET = PADDING_LEN / 2;
    const BASELINE = PADDING_CHAR.repeat(PADDING_LEN);

    const captureHost = document.createElement("div");
    captureHost.id = "zj-mobile-capture-host";
    captureHost.style.cssText =
        "position:fixed;top:0;left:0;" +
        "width:1px;height:1px;" +
        "opacity:0;pointer-events:none;" +
        "overflow:hidden;";
    captureHost.setAttribute("aria-hidden", "true");
    document.body.appendChild(captureHost);
    // Closed shadow root hides the input from password managers, whose content
    // scripts query input[type=password] and cannot pierce it.
    const captureShadow = captureHost.attachShadow({ mode: "closed" });

    const div = document.createElement("input");
    div.id = "zj-mobile-capture";
    // type="password" disables prediction/autocorrect/composition on every mobile
    // keyboard vendor, turning each tap into one immediate character.
    div.type = "password";
    div.setAttribute("autocomplete", "new-password");
    div.setAttribute("autocorrect", "off");
    div.setAttribute("autocapitalize", "off");
    div.setAttribute("spellcheck", "false");
    div.setAttribute("inputmode", "text");
    div.setAttribute("aria-hidden", "true");
    div.setAttribute("data-1p-ignore", "true");
    div.setAttribute("data-lpignore", "true");
    div.setAttribute("data-bwignore", "true");
    div.setAttribute("data-form-type", "other");
    div.setAttribute("data-dashlane-ignore", "true");
    div.tabIndex = -1;
    // Kept 1x1 transparent rather than hidden: display:none / visibility:hidden
    // would dismiss the OS keyboard.
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
        }
    };

    const resetBaseline = () => {
        div.value = BASELINE;
        setCaretToMiddle();
    };

    resetBaseline();

    // A single backspace tap fires two deleteContentBackward events ~5 ms apart;
    // this dedupe window merges them into one \x7f.
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

        if (ev.inputType === "deleteContentBackward") {
            div.value = BASELINE;
            lastText = BASELINE;
            setCaretToMiddle();
        }
    });

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

    // Mobile browsers honor programmatic focus() only inside a user gesture, so
    // re-focus the capture on every gesture to keep the OS keyboard summoned.
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

// inputmode="none" stops the OS keyboard auto-popping on tap while the textarea
// still processes hardware keypresses.
export function suppressSoftKeyboardOnTouch(term) {
    if (window.__zjSoftKbdSuppressed) {
        return;
    }
    if (!isCoarsePointerDevice()) {
        return;
    }
    window.__zjSoftKbdSuppressed = true;
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
            ta.removeAttribute("inputmode");
            ta.focus();
        }
    } else {
        if (capture) {
            capture.blur();
        }
        ta.setAttribute("inputmode", "none");
        ta.focus();
    }
}

export function toggleSoftKeyboard(term) {
    setSoftKeyboard(term, !window.__zjSoftKbdEnabled);
}
