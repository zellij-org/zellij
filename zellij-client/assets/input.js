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

    // On coarse-pointer (touch) devices, suppress the soft keyboard
    // auto-popup. xterm.js's textarea normally summons the on-screen
    // keyboard whenever it gains focus, and a tap inside #terminal
    // implicitly focuses it — that obscures the mobile-mode chrome
    // the user is trying to tap. Setting `inputmode="none"` keeps the
    // textarea focusable for hardware keyboards but prevents the soft
    // keyboard from sliding in. Hardware-keyboard typing (e.g. an
    // attached Bluetooth keyboard) still flows through normally.
    suppressSoftKeyboardOnTouch(term);

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

    // Touch handling: swipe-scroll, tap-to-click, and long-press → right
    // click. All three coexist on the same touch stream by tracking
    // motion and timing as the gesture unfolds:
    //
    //   - touchstart  : remember origin (x, y, t), arm a 500 ms long-press
    //                   timer, reset scroll bookkeeping.
    //   - touchmove   : if total motion exceeds the click threshold the
    //                   gesture is no longer a click — long-press timer
    //                   is cancelled. Vertical motion past the scroll
    //                   threshold (24 px) emits wheel events.
    //   - touchend    : if movement stayed under the click threshold and
    //                   the long-press did not already fire, emit an SGR
    //                   left click at the touch cell. There is no upper
    //                   bound on tap duration — a slow tap (300–500 ms)
    //                   still counts. The long-press itself is the only
    //                   gesture that suppresses the tap on release.
    //
    // SGR mouse coordinates are 1-based, hence the `+ 1` on col/row.
    let last_touch_y = null;
    let pending_scroll = 0;
    let touch_origin = null;      // { x, y, t } captured at touchstart
    let touch_moved = false;      // movement exceeded click_move_threshold
    let long_press_fired = false; // right-click already emitted for this gesture
    let long_press_timer = null;  // pending setTimeout id, cleared on cancel
    let two_finger_gesture_t = null; // performance.now() at 2-finger touchstart
    const touch_scroll_threshold = 24; // px before a swipe step counts
    // 16 px is roughly the touch slop Material/iOS use; a finger pressed
    // on a 1× display reliably stays inside that bound, while higher-DPI
    // devices forgive the 1–2 px the OS reports during a "still" press.
    const click_move_threshold = 16;
    const long_press_duration_ms = 500;
    // A 2-finger tap shorter than this counts as the keyboard toggle.
    // Above the threshold we assume the user changed their mind and
    // discard the gesture without flipping state.
    const two_finger_tap_max_ms = 600;

    const reportCoords = (clientX, clientY) =>
        term._core._mouseService.getMouseReportCoords(
            { clientX, clientY },
            terminal_element
        );

    const cancelLongPress = () => {
        if (long_press_timer !== null) {
            clearTimeout(long_press_timer);
            long_press_timer = null;
        }
    };

    const sendSgrButton = (button, col, row) => {
        // Press + release in a single send so any program bound to
        // mouse clicks sees a complete event regardless of network
        // batching at the WebSocket layer.
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}m`);
    };

    const sendWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const button = direction < 0 ? 65 : 64; // inverted: swipe up => wheel down
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
    };

    terminal_element.addEventListener(
        "touchstart",
        (event) => {
            // Two-finger tap is the keyboard toggle. We arm the
            // gesture timer here and confirm/dismiss on touchend
            // (when both fingers have lifted). While a 2-finger
            // gesture is in flight, we deliberately do not capture
            // a single-finger origin — the single-finger tap path
            // would otherwise also fire when the user lifts only
            // one finger.
            if (event.touches.length === 2 && two_finger_gesture_t === null) {
                event.preventDefault();
                two_finger_gesture_t = performance.now();
                cancelLongPress();
                touch_origin = null;
                touch_moved = false;
                long_press_fired = false;
                return;
            }
            if (two_finger_gesture_t !== null) {
                // Already mid-gesture (a third finger landed, or
                // the user lifted and re-placed). Don't treat as
                // a fresh single-finger tap.
                event.preventDefault();
                return;
            }
            if (event.touches.length > 0) {
                // Suppress the synthetic mouse events (mousedown,
                // mouseup, click) that iOS Safari and Chrome
                // mobile fire ~300 ms after a touch for legacy
                // mouse-compatibility. Without this they reach
                // xterm.js's own mouse handler, which then sends a
                // SECOND SGR click for the same gesture — a tap
                // that opens a menu would be immediately closed
                // by the synthesized click on the now-row-spanning
                // CollapseSelector region. Per the touch-events
                // spec, preventDefault on touchstart cancels the
                // emulated mouse cascade for that touch.
                event.preventDefault();
                const touch = event.touches[0];
                last_touch_y = touch.clientY;
                pending_scroll = 0;
                // Capture the cell coords NOW, while the terminal
                // layout still reflects what the user is looking at.
                // On iOS the soft keyboard sliding up between
                // touchstart and touchend triggers a visualViewport
                // resize → fitAddon re-fit → the cell at the same
                // screen coord shifts. Recomputing at touchend would
                // send a click for a different cell than the user
                // tapped, so we lock in the cell here and reuse it
                // for both the tap and the long-press path.
                const { col, row } = reportCoords(
                    touch.clientX,
                    touch.clientY
                );
                touch_origin = {
                    x: touch.clientX,
                    y: touch.clientY,
                    col,
                    row,
                    t: performance.now(),
                };
                touch_moved = false;
                long_press_fired = false;
                cancelLongPress();
                long_press_timer = setTimeout(() => {
                    // Still stationary and not yet released — emit a
                    // right-click at the captured cell. The flag
                    // suppresses the touchend tap so the user does not
                    // see both events for the same gesture.
                    long_press_timer = null;
                    if (touch_origin === null || touch_moved) {
                        return;
                    }
                    long_press_fired = true;
                    sendSgrButton(2, touch_origin.col, touch_origin.row);
                    if (typeof navigator.vibrate === "function") {
                        navigator.vibrate(10);
                    }
                }, long_press_duration_ms);
            }
        },
        { passive: false }
    );

    terminal_element.addEventListener(
        "touchmove",
        (event) => {
            if (event.touches.length === 0 || last_touch_y === null) {
                return;
            }
            event.preventDefault();
            const touch = event.touches[0];

            // Track total displacement from the origin. Any motion past
            // the click threshold disqualifies the gesture from being a
            // tap or long-press; only swipe-scroll remains active.
            if (touch_origin !== null && !touch_moved) {
                const dx = touch.clientX - touch_origin.x;
                const dy = touch.clientY - touch_origin.y;
                if (Math.hypot(dx, dy) > click_move_threshold) {
                    touch_moved = true;
                    cancelLongPress();
                }
            }

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
        (event) => {
            // Resolve a pending 2-finger gesture once both fingers
            // have lifted. Within the timeout this counts as a
            // keyboard toggle; otherwise discarded.
            if (
                two_finger_gesture_t !== null &&
                event.touches.length === 0
            ) {
                const elapsed = performance.now() - two_finger_gesture_t;
                two_finger_gesture_t = null;
                if (elapsed < two_finger_tap_max_ms) {
                    toggleSoftKeyboard(term);
                }
                return;
            }
            cancelLongPress();
            // Tap path: stationary release, not preempted by long-press.
            // Tap duration is unbounded — the user's finger can rest on
            // the bar for as long as they like; only motion or a fired
            // long-press disqualifies the gesture. Use the cell coords
            // captured at touchstart so a layout shift mid-tap (e.g.
            // soft keyboard sliding up) cannot redirect the click.
            if (
                touch_origin !== null &&
                !touch_moved &&
                !long_press_fired
            ) {
                sendSgrButton(0, touch_origin.col, touch_origin.row);
            }
            last_touch_y = null;
            pending_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
        },
        { passive: true }
    );

    terminal_element.addEventListener(
        "touchcancel",
        () => {
            // Browser yanked the gesture (e.g. a system-level swipe) —
            // discard it without firing tap or long-press so the user
            // doesn't see a stray click after a cancelled gesture.
            cancelLongPress();
            last_touch_y = null;
            pending_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
            two_finger_gesture_t = null;
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
 * Mark xterm.js's textarea `inputmode="none"` on touch devices so the
 * soft keyboard does not auto-pop on every tap inside the terminal.
 * Idempotent across the dual `setupInputHandlers` invocations from
 * `index.js`. Skipped on `pointer: fine` devices (desktops) so they
 * see no behavioral change.
 *
 * The textarea still receives focus and still processes hardware
 * keypresses; only the on-screen keyboard popup is suppressed. Users
 * who want the soft keyboard back perform a 2-finger tap, handled by
 * `toggleSoftKeyboard` below.
 */
function suppressSoftKeyboardOnTouch(term) {
    if (window.__zjSoftKbdSuppressed) {
        return;
    }
    if (!isCoarsePointerDevice()) {
        return;
    }
    window.__zjSoftKbdSuppressed = true;
    window.__zjSoftKbdEnabled = false;
    // xterm.js creates the textarea asynchronously inside term.open();
    // poll briefly until it is attached, then mark it.
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
 * Heuristic match for "this device probably has only a soft keyboard
 * available". `pointer: coarse` covers touchscreens; the UA fallback
 * handles browsers/devices with quirky media query support.
 */
function isCoarsePointerDevice() {
    if (
        window.matchMedia &&
        window.matchMedia("(pointer: coarse)").matches
    ) {
        return true;
    }
    return /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent);
}

/**
 * Set the soft-keyboard visibility explicitly on touch devices. Called
 * from two places: the 2-finger tap gesture handler (via
 * `toggleSoftKeyboard`) and the control-channel `SetSoftKeyboard`
 * message dispatched by the mobile plugin when the user taps its ⌨
 * button. On desktops this is a no-op because the suppression was
 * never installed.
 *
 * Mechanics: removing `inputmode="none"` lets the browser open the
 * soft keyboard the next time the textarea has focus, so we follow
 * up with `focus()`. The reverse path applies `inputmode="none"` and
 * `blur()` so the keyboard slides away. Idempotent — if the requested
 * state already matches the current state, the call returns without
 * touching the DOM (avoiding a no-op `focus()`/`blur()` flicker).
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
    if (on) {
        ta.removeAttribute("inputmode");
        ta.focus();
    } else {
        ta.setAttribute("inputmode", "none");
        ta.blur();
    }
}

/**
 * Flip the soft-keyboard state on touch devices. Called from the
 * 2-finger tap gesture handler. Thin wrapper around `setSoftKeyboard`.
 */
function toggleSoftKeyboard(term) {
    setSoftKeyboard(term, !window.__zjSoftKbdEnabled);
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
