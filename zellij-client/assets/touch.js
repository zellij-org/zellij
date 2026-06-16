import { clampFontSize } from "./terminal.js";
import { toggleSoftKeyboard } from "./soft-keyboard.js";

const SGR_COORD_BASE = 1;
export function installTouchGestures({ term, terminalElement, sendFunction, pinch }) {
    let last_touch_y = null;
    let last_touch_x = null;
    let pending_scroll = 0;
    let pending_h_scroll = 0;
    let touch_origin = null;
    let touch_moved = false;
    let long_press_fired = false;
    let long_press_timer = null;
    let two_finger_gesture_t = null;
    let pinch_initial_distance = null;
    let pinch_initial_font_size = null;
    let pinch_active = false;

    const touch_scroll_threshold = 24;
    const click_move_threshold = 16;
    const long_press_duration_ms = 500;
    const two_finger_tap_max_ms = 600;
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
        const sgrCol = col + SGR_COORD_BASE;
        const sgrRow = row + SGR_COORD_BASE;
        sendFunction(`\x1b[<${button};${sgrCol};${sgrRow}M`);
        sendFunction(`\x1b[<${button};${sgrCol};${sgrRow}m`);
    };

    const sendWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const swipedUp = direction < 0;
        const button = swipedUp ? 65 : 64;
        sendFunction(`\x1b[<${button};${col + SGR_COORD_BASE};${row + SGR_COORD_BASE}M`);
    };

    // Scroll direction follows the content, not the finger: swiping the content
    // leftward reveals the right edge, so finger-left maps to wheel-right.
    const sendHorizontalWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const button = direction < 0 ? 66 : 67;
        sendFunction(`\x1b[<${button};${col + SGR_COORD_BASE};${row + SGR_COORD_BASE}M`);
    };

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
                event.preventDefault();
                return;
            }
            if (event.touches.length > 0) {
                // iOS/Android honor focus() as a keyboard summon only inside a
                // still-active, not-yet-prevented gesture, so this must precede
                // the preventDefault below.
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
                // preventDefault on touchstart cancels the synthetic mouse cascade
                // browsers fire ~300 ms later, which would otherwise make xterm.js
                // send a second SGR click for the same gesture.
                event.preventDefault();
                const touch = event.touches[0];
                last_touch_y = touch.clientY;
                last_touch_x = touch.clientX;
                pending_scroll = 0;
                pending_h_scroll = 0;
                // Capture cell coords now: on iOS the soft keyboard sliding up
                // between touchstart and touchend re-fits the grid.
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

            if (touch_origin !== null && !touch_moved) {
                const dx = touch.clientX - touch_origin.x;
                const dy = touch.clientY - touch_origin.y;
                if (Math.hypot(dx, dy) > click_move_threshold) {
                    touch_moved = true;
                    cancelLongPress();
                }
            }

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
            if (two_finger_gesture_t !== null && event.touches.length === 0) {
                const elapsed = performance.now() - two_finger_gesture_t;
                const wasPinch = pinch_active;
                two_finger_gesture_t = null;
                pinch_active = false;
                pinch_initial_distance = null;
                pinch_initial_font_size = null;
                if (wasPinch) {
                    pinch.armRemoval();
                    return;
                }
                if (elapsed < two_finger_tap_max_ms) {
                    toggleSoftKeyboard(term);
                }
                return;
            }
            cancelLongPress();
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
            cancelLongPress();
            last_touch_y = null;
            last_touch_x = null;
            pending_scroll = 0;
            pending_h_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
            two_finger_gesture_t = null;
            pinch_initial_distance = null;
            pinch_initial_font_size = null;
            pinch_active = false;
            pinch.destroy();
        },
        { passive: true }
    );
}
