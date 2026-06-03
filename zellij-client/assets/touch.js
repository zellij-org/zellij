/**
 * Touch gestures: swipe-scroll, tap-to-click, long-press → right-click,
 * two-finger-tap → keyboard toggle, and pinch-to-zoom.
 *
 * All single-finger gestures coexist on one touch stream by tracking motion and
 * timing as it unfolds:
 *   - touchstart: remember origin (x,y,t,col,row), arm a 500 ms long-press timer.
 *   - touchmove : motion past the click threshold disqualifies tap/long-press;
 *                 vertical/horizontal motion past the scroll threshold emits wheel.
 *   - touchend  : if movement stayed under threshold and long-press didn't fire,
 *                 emit a left click at the cell captured on touchstart.
 * SGR coordinates are 1-based (hence the +1 on col/row).
 */

import { clampFontSize } from "./terminal.js";
import { toggleSoftKeyboard } from "./soft-keyboard.js";

export function installTouchGestures({ term, terminalElement, sendFunction, pinch }) {
    let last_touch_y = null;
    let last_touch_x = null;
    let pending_scroll = 0;
    let pending_h_scroll = 0;
    let touch_origin = null;      // { x, y, col, row, t } captured at touchstart
    let touch_moved = false;      // movement exceeded click_move_threshold
    let long_press_fired = false; // right-click already emitted for this gesture
    let long_press_timer = null;
    let two_finger_gesture_t = null; // performance.now() at 2-finger touchstart
    // Pinch bookkeeping: finger spread and font size captured at 2-finger
    // touchstart drive a ratio-based new font size on move. `pinch_active`
    // flips once the spread changes past pinch_activation_threshold, which
    // disqualifies the gesture from the 2-finger-tap keyboard toggle.
    let pinch_initial_distance = null;
    let pinch_initial_font_size = null;
    let pinch_active = false;

    const touch_scroll_threshold = 24; // px before a swipe step counts
    // 16 px ≈ the touch slop Material/iOS use; a still press stays inside it.
    const click_move_threshold = 16;
    const long_press_duration_ms = 500;
    // A 2-finger tap shorter than this counts as the keyboard toggle.
    const two_finger_tap_max_ms = 600;
    // Pinch needs a deliberate spread to engage so accidental drift during a
    // 2-finger tap doesn't zoom. 18 px exceeds the OS slop (~10 px).
    const pinch_activation_threshold = 18;

    const reportCoords = (clientX, clientY) =>
        term._core._mouseService.getMouseReportCoords(
            { clientX, clientY },
            terminalElement
        );

    const cancelLongPress = () => {
        if (long_press_timer !== null) {
            clearTimeout(long_press_timer);
            long_press_timer = null;
        }
    };

    const sendSgrButton = (button, col, row) => {
        // Press + release in one send so a program bound to mouse clicks sees a
        // complete event regardless of WebSocket batching.
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}m`);
    };

    const sendWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const button = direction < 0 ? 65 : 64; // inverted: swipe up => wheel down
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
    };

    // Horizontal wheel (buttons 66/67 mirror 64/65). Direction is where the
    // *content* moves, not the finger: finger left drags content left and
    // reveals the right edge (ScrollRight), so finger left => wheel right.
    const sendHorizontalWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const button = direction < 0 ? 66 : 67;
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
    };

    // Euclidean distance between the first two touches; 0 if fewer than two.
    const touchPairDistance = (touches) => {
        if (touches.length < 2) {
            return 0;
        }
        const dx = touches[0].clientX - touches[1].clientX;
        const dy = touches[0].clientY - touches[1].clientY;
        return Math.hypot(dx, dy);
    };

    terminalElement.addEventListener(
        "touchstart",
        (event) => {
            // Two-finger tap = keyboard toggle. Arm the timer here, resolve on
            // touchend. While a 2-finger gesture is in flight, don't capture a
            // single-finger origin (it would also fire on lifting one finger).
            if (event.touches.length === 2 && two_finger_gesture_t === null) {
                event.preventDefault();
                two_finger_gesture_t = performance.now();
                cancelLongPress();
                touch_origin = null;
                touch_moved = false;
                long_press_fired = false;
                pinch_initial_distance = touchPairDistance(event.touches);
                pinch_initial_font_size = clampFontSize(
                    term.options.fontSize || 16
                );
                pinch_active = false;
                return;
            }
            if (two_finger_gesture_t !== null) {
                // Already mid-gesture (third finger / re-place) — not a fresh tap.
                event.preventDefault();
                return;
            }
            if (event.touches.length > 0) {
                // Summon the OS keyboard BEFORE preventDefault: iOS/Android honor
                // focus() as a keyboard summon only inside a still-active,
                // not-yet-prevented gesture. The preventDefault below suppresses
                // the synthetic mouse cascade (and the focus it carries), so this
                // focus call must precede it to give a valid summon signal.
                if (
                    window.__zjSoftKbdEnabled &&
                    window.__zjSoftKbdCapture &&
                    window.__zjSoftKbdCapture.element &&
                    !window.__zjSoftKbdCapture.isFocused
                ) {
                    try {
                        window.__zjSoftKbdCapture.element.focus({
                            preventScroll: true,
                        });
                    } catch (_) {
                        window.__zjSoftKbdCapture.element.focus();
                    }
                }
                // Suppress the synthetic mouse cascade (mousedown/up/click) that
                // mobile browsers fire ~300 ms after a touch — otherwise xterm.js
                // sends a SECOND SGR click for the same gesture. Per the
                // touch-events spec, preventDefault on touchstart cancels it.
                event.preventDefault();
                const touch = event.touches[0];
                last_touch_y = touch.clientY;
                last_touch_x = touch.clientX;
                pending_scroll = 0;
                pending_h_scroll = 0;
                // Capture cell coords NOW: on iOS the soft keyboard sliding up
                // between touchstart and touchend re-fits the grid, so the cell
                // at the same screen coord shifts. Lock it in here and reuse for
                // both tap and long-press.
                const { col, row } = reportCoords(touch.clientX, touch.clientY);
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
                    long_press_timer = null;
                    if (touch_origin === null || touch_moved) {
                        return;
                    }
                    // Still stationary and unreleased — right-click at the
                    // captured cell. The flag suppresses the touchend tap.
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

    terminalElement.addEventListener(
        "touchmove",
        (event) => {
            // Pinch path runs before the swipe logic so a pinch never emits
            // scroll wheel events.
            if (
                event.touches.length === 2 &&
                pinch_initial_distance !== null &&
                pinch_initial_distance > 0
            ) {
                event.preventDefault();
                const dist = touchPairDistance(event.touches);
                if (
                    !pinch_active &&
                    Math.abs(dist - pinch_initial_distance) >
                        pinch_activation_threshold
                ) {
                    pinch_active = true;
                    // Snapshot now: from here the user sees the overlay rather
                    // than the xterm canvas, so it can clear/repaint without a
                    // visible flash (the overlay is refreshed on every render).
                    pinch.snapshot();
                }
                if (pinch_active) {
                    const ratio = dist / pinch_initial_distance;
                    pinch.applyFontSize(pinch_initial_font_size * ratio);
                }
                return;
            }

            if (event.touches.length === 0 || last_touch_y === null) {
                return;
            }
            event.preventDefault();
            const touch = event.touches[0];

            // Any motion past the click threshold disqualifies tap/long-press;
            // only swipe-scroll remains.
            if (touch_origin !== null && !touch_moved) {
                const dx = touch.clientX - touch_origin.x;
                const dy = touch.clientY - touch_origin.y;
                if (Math.hypot(dx, dy) > click_move_threshold) {
                    touch_moved = true;
                    cancelLongPress();
                }
            }

            // Per-axis accumulators, no axis lock — a diagonal swipe fires both
            // vertical and horizontal ticks. The mobile plugin clamps pan
            // offsets, so an off-axis tick at the current max is a harmless no-op.
            const delta_y = touch.clientY - last_touch_y;
            const delta_x =
                last_touch_x === null ? 0 : touch.clientX - last_touch_x;
            last_touch_y = touch.clientY;
            last_touch_x = touch.clientX;
            pending_scroll += delta_y;
            pending_h_scroll += delta_x;
            while (pending_scroll <= -touch_scroll_threshold) {
                sendWheelEvent(-1, touch);
                pending_scroll += touch_scroll_threshold;
            }
            while (pending_scroll >= touch_scroll_threshold) {
                sendWheelEvent(1, touch);
                pending_scroll -= touch_scroll_threshold;
            }
            while (pending_h_scroll <= -touch_scroll_threshold) {
                sendHorizontalWheelEvent(-1, touch);
                pending_h_scroll += touch_scroll_threshold;
            }
            while (pending_h_scroll >= touch_scroll_threshold) {
                sendHorizontalWheelEvent(1, touch);
                pending_h_scroll -= touch_scroll_threshold;
            }
        },
        { passive: false }
    );

    terminalElement.addEventListener(
        "touchend",
        (event) => {
            // Resolve a pending 2-finger gesture once both fingers have lifted.
            if (two_finger_gesture_t !== null && event.touches.length === 0) {
                const elapsed = performance.now() - two_finger_gesture_t;
                const wasPinch = pinch_active;
                two_finger_gesture_t = null;
                pinch_active = false;
                pinch_initial_distance = null;
                pinch_initial_font_size = null;
                if (wasPinch) {
                    // The pinched font size is session-only (a reload resets to
                    // the server/browser default — deliberately ephemeral so a
                    // stale zoom can't bias fresh-attach mobile-mode routing).
                    // Arm overlay removal: the next render refreshes it once more
                    // and drops it after a rAF, so the final canvas-clear is
                    // invisible. The keyboard toggle is suppressed (pinch ≠ tap).
                    pinch.armRemoval();
                    return;
                }
                if (elapsed < two_finger_tap_max_ms) {
                    toggleSoftKeyboard(term);
                }
                return;
            }
            cancelLongPress();
            // Tap path: stationary release not preempted by long-press. Tap
            // duration is unbounded; only motion or a fired long-press
            // disqualifies it. Use the cell captured at touchstart so a layout
            // shift mid-tap can't redirect the click.
            if (touch_origin !== null && !touch_moved && !long_press_fired) {
                sendSgrButton(0, touch_origin.col, touch_origin.row);
            }
            last_touch_y = null;
            last_touch_x = null;
            pending_scroll = 0;
            pending_h_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
        },
        { passive: true }
    );

    terminalElement.addEventListener(
        "touchcancel",
        () => {
            // Browser yanked the gesture (e.g. a system swipe) — discard it
            // without firing tap or long-press.
            cancelLongPress();
            last_touch_y = null;
            last_touch_x = null;
            pending_scroll = 0;
            pending_h_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
            two_finger_gesture_t = null;
            // Keep whatever font size the gesture applied; reset only the
            // bookkeeping. A cancelled gesture is ambiguous intent.
            pinch_initial_distance = null;
            pinch_initial_font_size = null;
            pinch_active = false;
            // Drop the overlay immediately — there's no touchend to schedule an
            // orderly removal. The canvas behind may briefly show in whatever
            // state xterm.js left it; tolerable for a cancelled gesture.
            pinch.destroy();
        },
        { passive: true }
    );
}
