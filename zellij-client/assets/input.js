/**
 * Input handling for terminal events
 */

import { encode_kitty_key } from "./keyboard.js";
import { isMac } from "./utils.js";
import { clampFontSize } from "./terminal.js";

/**
 * Set up all input handlers for the terminal
 * @param {Terminal} term - The terminal instance
 * @param {FitAddon} fitAddon - The xterm.js fit addon, used by the
 *     pinch-to-zoom gesture to re-flow the grid when the font size
 *     changes mid-gesture.
 * @param {function} sendFunction - Function to send data through WebSocket
 */
export function setupInputHandlers(term, fitAddon, sendFunction) {
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

    // On mobile, the system soft keyboard's `keydown` events on
    // xterm.js's textarea are unreliable (Android typically reports
    // `keyCode 229` with `key === "Unidentified"`; autocorrect /
    // predictive text batch and rewrite input). Install a dedicated
    // hidden `<input type="password">` (housed inside a closed shadow
    // root) whose `input`-event value-diff drives a sentinel-backed
    // capture path — consistent across iOS Safari, Android Chrome,
    // GBoard, SwiftKey, Samsung Keyboard, and Firefox Android, AND
    // works in in-app browsers / older WebViews. See
    // `installSoftKeyboardCapture`'s docstring for the full rationale
    // behind the password-input + closed-shadow-root design.
    // Scoped to coarse-pointer devices so desktops are untouched.
    // The window-level click / touchend / pointerdown listeners
    // installed inside this function focus the capture on every
    // gesture, so the OS keyboard appears on the first tap.
    installSoftKeyboardCapture(term, sendFunction);

    // On coarse-pointer (touch) devices, mark xterm.js's textarea
    // `inputmode="none"` so a tap on the terminal does not summon
    // the OS keyboard through the textarea — the textarea is the
    // input target for hardware keyboards only. The dedicated
    // capture `<input>` installed above is the OS-keyboard target,
    // and its window-level focus listeners route every touch there.
    // Hardware-keyboard typing (e.g. an attached Bluetooth keyboard)
    // still flows through xterm.js's normal keydown handler.
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
    let last_touch_x = null;
    let pending_scroll = 0;
    let pending_h_scroll = 0;
    let touch_origin = null;      // { x, y, t } captured at touchstart
    let touch_moved = false;      // movement exceeded click_move_threshold
    let long_press_fired = false; // right-click already emitted for this gesture
    let long_press_timer = null;  // pending setTimeout id, cleared on cancel
    let two_finger_gesture_t = null; // performance.now() at 2-finger touchstart
    // Pinch-to-zoom bookkeeping: the finger spread at 2-finger
    // touchstart and the terminal's font size at the same moment are
    // captured so a subsequent touchmove can compute a ratio-based
    // new font size independent of where the gesture started.
    // `pinch_active` flips true once the spread changes by more than
    // `pinch_activation_threshold` px — that disqualifies the gesture
    // from the 2-finger-tap keyboard toggle on release.
    let pinch_initial_distance = null;
    let pinch_initial_font_size = null;
    let pinch_active = false;
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
    // Pinch needs a deliberate spread/contract to engage so that
    // accidental finger drift during a 2-finger tap does not
    // accidentally zoom. 18 px comfortably exceeds the OS touch slop
    // (≈10 px) while still feeling instant on a phone.
    const pinch_activation_threshold = 18;

    // Pinch snapshot overlay. xterm.js's WebGL renderer assigns
    // canvas.width / canvas.height inside the fontSize setter and
    // inside term.resize, and that assignment clears the
    // framebuffer. On mobile the next animation frame is rAF-
    // throttled during a touch gesture, so the cleared canvas
    // gets composited before xterm.js can repaint — the user sees
    // a blank flash. The overlay hides that window:
    //
    //   1. When the gesture first crosses the pinch-activation
    //      threshold, we snapshot the live canvas pixels onto a
    //      <canvas>, position it absolutely on top of the xterm
    //      canvas, and append it to <body>.
    //   2. Underneath, applyPinchFontSize continues to drive
    //      xterm.js / the server resize as before. Every canvas
    //      clear caused by a fontSize or term.resize change is
    //      hidden by the overlay.
    //   3. Every time xterm.js paints (term.onRender) while the
    //      overlay is up, we re-blit the now-fresh canvas content
    //      onto the overlay. That's what lets server data that
    //      lands mid-pinch reach the user — the snapshot tracks
    //      the latest committed render rather than freezing at
    //      the moment the gesture started.
    //   4. On touchend, the overlay's removal is armed: the very
    //      next term.onRender refreshes the overlay one last time
    //      (so the user sees the post-commit content), defers one
    //      extra rAF for the browser to composite, then drops the
    //      overlay. A safety timeout guarantees removal even if
    //      onRender doesn't fire (e.g. server unreachable).
    let pinchOverlay = null;
    let pinchOverlayAwaitingRender = false;
    let pinchOverlaySafetyTimer = null;

    const destroyPinchOverlay = () => {
        if (pinchOverlay) {
            pinchOverlay.remove();
            pinchOverlay = null;
        }
        pinchOverlayAwaitingRender = false;
        if (pinchOverlaySafetyTimer !== null) {
            clearTimeout(pinchOverlaySafetyTimer);
            pinchOverlaySafetyTimer = null;
        }
    };

    const createPinchOverlay = () => {
        destroyPinchOverlay();
        if (!term.element) return;
        const sourceCanvases = term.element.querySelectorAll("canvas");
        if (sourceCanvases.length === 0) return;
        const ref = sourceCanvases[0];
        const rect = ref.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return;

        const overlay = document.createElement("canvas");
        overlay.width = ref.width;
        overlay.height = ref.height;
        const outer = document.getElementById("terminal");
        const bg = outer
            ? window.getComputedStyle(outer).backgroundColor
            : "transparent";
        Object.assign(overlay.style, {
            position: "fixed",
            left: rect.left + "px",
            top: rect.top + "px",
            width: rect.width + "px",
            height: rect.height + "px",
            zIndex: "9999",
            pointerEvents: "none",
            background: bg,
        });

        const ctx = overlay.getContext("2d");
        if (ctx) {
            // Composite every xterm.js canvas (main WebGL render +
            // any auxiliary layers — cursor, selection, link
            // underline) into the overlay so the snapshot matches
            // what the user sees.
            for (const c of sourceCanvases) {
                try {
                    ctx.drawImage(c, 0, 0);
                } catch (e) {
                    // Tainted canvases throw; skip and continue.
                }
            }
        }

        document.body.appendChild(overlay);
        pinchOverlay = overlay;
    };

    // Re-draw the existing overlay element with the current xterm
    // canvas state. Called from the term.onRender hook so server
    // data that lands mid-pinch is exposed to the user without
    // tearing down the overlay element (which would briefly
    // reveal the canvas underneath at the wrong moment).
    //
    // Resizing the overlay's backing buffer is atomic w.r.t. the
    // compositor: the resize-clear and the immediately-following
    // drawImage happen inside the onRender callback (synchronous
    // JS), so the browser never composites the cleared overlay.
    const refreshPinchOverlay = () => {
        if (!pinchOverlay) return;
        if (!term.element) return;
        const sourceCanvases = term.element.querySelectorAll("canvas");
        if (sourceCanvases.length === 0) return;
        const ref = sourceCanvases[0];
        const rect = ref.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) return;

        if (pinchOverlay.width !== ref.width) {
            pinchOverlay.width = ref.width;
        }
        if (pinchOverlay.height !== ref.height) {
            pinchOverlay.height = ref.height;
        }
        pinchOverlay.style.left = rect.left + "px";
        pinchOverlay.style.top = rect.top + "px";
        pinchOverlay.style.width = rect.width + "px";
        pinchOverlay.style.height = rect.height + "px";

        const ctx = pinchOverlay.getContext("2d");
        if (ctx) {
            ctx.clearRect(0, 0, pinchOverlay.width, pinchOverlay.height);
            for (const c of sourceCanvases) {
                try {
                    ctx.drawImage(c, 0, 0);
                } catch (e) {
                    // Tainted canvases throw; skip and continue.
                }
            }
        }
    };

    const armPinchOverlayRemoval = () => {
        if (!pinchOverlay) return;
        pinchOverlayAwaitingRender = true;
        // Safety net: if onRender doesn't fire (e.g. the server
        // didn't actually respond), remove the overlay anyway after
        // a bounded time. 600 ms is generous for a roundtrip + a
        // couple of paint frames.
        if (pinchOverlaySafetyTimer !== null) {
            clearTimeout(pinchOverlaySafetyTimer);
        }
        pinchOverlaySafetyTimer = setTimeout(() => {
            pinchOverlaySafetyTimer = null;
            destroyPinchOverlay();
        }, 600);
    };

    // Hook xterm.js's render callback once. Two roles:
    //   * While the overlay is up AND a pinch is still in
    //     progress (or the touchend-armed removal has not fired
    //     yet), each render is a moment when the canvas
    //     underneath has fresh content. Re-snapshot it so the
    //     overlay tracks the latest committed state — this is
    //     what lets server data delivered mid-pinch reach the
    //     user instead of being hidden until touchend.
    //   * Once removal has been armed (touchend with wasPinch),
    //     the next render is also when the post-commit content
    //     has landed; defer one extra rAF (so the browser has a
    //     chance to composite the refreshed overlay) then drop
    //     it entirely.
    if (term && typeof term.onRender === "function") {
        term.onRender(() => {
            if (!pinchOverlay) return;
            refreshPinchOverlay();
            if (pinchOverlayAwaitingRender) {
                pinchOverlayAwaitingRender = false;
                requestAnimationFrame(() => {
                    destroyPinchOverlay();
                });
            }
        });
    }

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

    /// Horizontal wheel SGR. Buttons 66/67 mirror 64/65 — 66 is
    /// "wheel right" (positive horizontal), 67 is "wheel left". The
    /// server's mouse_handler decodes both via `event.wheel_left/right`.
    ///
    /// Convention matches `sendWheelEvent`'s "swipe up => wheel down"
    /// inversion: the wheel direction is the direction the *content*
    /// moves under the finger, not the direction the finger moves.
    /// A finger swiping left drags content leftward and reveals the
    /// right edge — the plugin interprets that as ScrollRight (which
    /// increases `viewport_h_pan`). Likewise finger right => ScrollLeft.
    const sendHorizontalWheelEvent = (direction, touch) => {
        const { col, row } = reportCoords(touch.clientX, touch.clientY);
        const button = direction < 0 ? 66 : 67; // finger left => wheel right
        sendFunction(`\x1b[<${button};${col + 1};${row + 1}M`);
    };

    // Euclidean distance between the first two active TouchList
    // entries. Returns 0 if fewer than two touches are present so the
    // caller can decide what to do with a degenerate gesture.
    const touchPairDistance = (touches) => {
        if (touches.length < 2) {
            return 0;
        }
        const dx = touches[0].clientX - touches[1].clientX;
        const dy = touches[0].clientY - touches[1].clientY;
        return Math.hypot(dx, dy);
    };

    // Apply a new font size during a pinch. We deliberately do NOT
    // call `fitAddon.fit()` here, even though it is the obvious
    // thing to do after a font-size change:
    //
    //   fit() synchronously calls term.resize(cols, rows) with the
    //   new dimensions. The custom event we dispatch below reaches
    //   `setupResizeHandler.resizeTerminal`, which compares
    //   `proposeDimensions()` against `term.rows`/`term.cols`. After
    //   fit() those values are equal, so resizeTerminal short-
    //   circuits and never sends the resize to the server. The
    //   server keeps rendering for the old grid size, the browser
    //   grid is smaller, and the mobile plugin's bottom rows (the
    //   keyboard) get clipped out of both views. Pinching back in
    //   does not bring them back because the buffer rows were
    //   already discarded by xterm's resize.
    //
    // By skipping fit() here, term.rows/cols stay at the OLD grid
    // when the resize handler fires. It then sees a real mismatch
    // (proposeDimensions reflects the new cell size, term does
    // not), calls term.resize itself, and emits the resize control
    // message — which is what causes the server to redraw the
    // mobile plugin at the new dimensions.
    //
    // The xterm.js renderer updates its cell metrics synchronously
    // when `term.options.fontSize` is assigned (the OptionsService
    // → renderer change handler runs inline), so proposeDimensions
    // returns correct new geometry by the time the resize handler
    // reads it on the next animation frame.
    //
    // We fire a dedicated `zellij:rendering-resize` event rather
    // than a plain `resize`. `setupResizeHandler` routes the
    // former to a `TerminalResizeRendering` payload, which the
    // server bridge translates into `ResizeCause::RenderingPreference`
    // and the server's TerminalResize handler skips mobile-mode
    // re-evaluation for. Without this distinction, a pinch that
    // happens to push the cell grid past the mobile threshold
    // would auto-demote the client out of the mobile layout.
    const applyPinchFontSize = (px) => {
        const clamped = clampFontSize(px);
        if (term.options.fontSize === clamped) {
            return;
        }
        term.options.fontSize = clamped;
        window.dispatchEvent(new CustomEvent("zellij:rendering-resize"));
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
                // Record the spread + current font size for pinch
                // zoom. `pinch_active` stays false until touchmove
                // sees the spread change past the activation
                // threshold; until then, a 2-finger release within
                // the tap window still counts as the keyboard
                // toggle.
                pinch_initial_distance = touchPairDistance(event.touches);
                pinch_initial_font_size = clampFontSize(
                    term.options.fontSize || 16
                );
                pinch_active = false;
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
                // Summon the OS soft keyboard *before* preventDefault.
                // iOS/Android honour focus() as an OS-keyboard summon
                // only when it happens inside a still-active user
                // gesture and the gesture has not been preventDefault'd
                // yet. The preventDefault below suppresses the
                // synthetic mouse-event cascade (and the focus shifts
                // it would carry), which is necessary to stop xterm.js
                // from sending a second SGR click — but it also
                // cancels the implicit "click brings up keyboard"
                // behavior. Doing the focus call here, ahead of
                // preventDefault, gives the browser a valid summon
                // signal that survives the suppression.
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
                last_touch_x = touch.clientX;
                pending_scroll = 0;
                pending_h_scroll = 0;
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
            // Pinch path: when two fingers are down and we recorded
            // the initial spread on touchstart, drive a ratio-based
            // font-size change. This runs *before* the single-finger
            // swipe logic so a pinch never accidentally emits scroll
            // wheel events on the underlying terminal.
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
                    // Snapshot now. From this point on the user
                    // sees the overlay rather than the xterm
                    // canvas directly, so the canvas can
                    // clear/repaint freely without a visible
                    // flash. The overlay is then refreshed on
                    // every term.onRender, so the user does see
                    // server updates that land mid-pinch.
                    createPinchOverlay();
                }
                if (pinch_active) {
                    const ratio = dist / pinch_initial_distance;
                    applyPinchFontSize(pinch_initial_font_size * ratio);
                }
                return;
            }

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

            // Per-axis accumulators are advanced independently — there
            // is no axis lock. A diagonal swipe of, say, 48 px on each
            // axis fires two vertical ticks AND two horizontal ticks
            // (interleaved at whatever order frame deltas cross the
            // threshold). The mobile plugin clamps both pan offsets
            // against the cached viewport extent so an off-axis tick
            // that lands at the current max is a harmless no-op.
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
                const wasPinch = pinch_active;
                two_finger_gesture_t = null;
                pinch_active = false;
                pinch_initial_distance = null;
                pinch_initial_font_size = null;
                if (wasPinch) {
                    // The user zoomed; the new font size lives only
                    // in `term.options.fontSize` for the duration of
                    // this session. A page reload resets to the
                    // server-/browser-driven default — deliberately
                    // ephemeral so a stale zoom from a previous
                    // session cannot bias the fresh-attach
                    // measurements that drive mobile-mode routing.
                    // The 2-finger-tap keyboard toggle is suppressed
                    // because the pinch is a different intent.
                    //
                    // Arm overlay removal: as soon as xterm.js paints
                    // the new content (next term.onRender), the
                    // overlay is refreshed once more and then
                    // dropped after the next rAF. Until then the
                    // user keeps seeing the latest committed
                    // snapshot, so the canvas-clear that happens
                    // during the final commit is invisible.
                    armPinchOverlayRemoval();
                    return;
                }
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
            last_touch_x = null;
            pending_scroll = 0;
            pending_h_scroll = 0;
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
            last_touch_x = null;
            pending_scroll = 0;
            pending_h_scroll = 0;
            touch_origin = null;
            touch_moved = false;
            long_press_fired = false;
            two_finger_gesture_t = null;
            // If the cancel arrives mid-pinch we keep whatever font
            // size the gesture had already applied; only the
            // bookkeeping is reset so a fresh gesture starts clean.
            // The persisted value is intentionally not written here
            // since a cancelled gesture is ambiguous intent.
            pinch_initial_distance = null;
            pinch_initial_font_size = null;
            pinch_active = false;
            // Drop the overlay immediately on cancel — there will
            // be no touchend to schedule the orderly removal, so
            // we can't wait for term.onRender. The user may briefly
            // see the canvas behind in whatever state xterm.js
            // left it; tolerable for a cancelled gesture.
            destroyPinchOverlay();
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
 * Capture mobile soft-keyboard keystrokes for the terminal.
 *
 * ## The problem
 *
 * The OS soft keyboard refuses to send keystrokes anywhere except into a
 * text-input element it owns, and once it owns one it insists on running
 * its own predictive / autocorrect / composition engine on top of every
 * keystroke — an engine that holds invisible state, batches characters,
 * replaces them after the fact, and re-flushes them at moments of its own
 * choosing. None of that is what a terminal wants. The user expects each
 * tap to deliver one byte to the shell, immediately and exactly once.
 *
 * ## The hack: three independent layers stacked
 *
 * This function keeps the OS keyboard happy ("yes, I am pointed at a real
 * text input, please send me keystrokes") while neutralizing everything
 * the keyboard tries to do on top of those keystrokes. Three independent
 * mechanisms are stacked, each addressing a constraint imposed by the
 * layer below it:
 *
 *   1. **A hidden text-input element** exists so the OS keyboard has a
 *      target. 1×1 px, fully transparent, `pointer-events:none`,
 *      fixed-positioned at (0,0). Invisible to the user but focusable —
 *      `display:none` and `visibility:hidden` would close the keyboard.
 *
 *   2. **The input is `type="password"`** so the keyboard's predictive
 *      engine is disabled. Every mobile keyboard vendor (GBoard, SwiftKey,
 *      Samsung, iOS, Firefox Android) exempts password fields from
 *      prediction, autocorrect, autosuggest, cross-keystroke composition,
 *      and "learn this word" history — by policy, because a keyboard that
 *      predicted bank passwords would be a security incident. Every tap
 *      becomes a single character, dispatched immediately, never revisited.
 *      The hidden composition buffer that was the source of the original
 *      Tab+Enter doubling bug (typing "ser" → tab-complete to "zellij-server"
 *      → Enter would submit "zellij-serverser" because the IME re-flushed
 *      the stale "ser" prefix when its composition state lost coherence
 *      with the shell's view of the line) simply does not exist for
 *      password fields. `.value` still exposes typed characters as plain
 *      text, so the diff pipeline below works unchanged; the "dot
 *      rendering" is irrelevant because the element is invisible anyway.
 *
 *   3. **The input lives inside a closed shadow root** so password managers
 *      (1Password, LastPass, Bitwarden, Dashlane, browser built-ins) cannot
 *      find it. Their content scripts detect targets via
 *      `document.querySelectorAll('input[type="password"]')`, which does
 *      not pierce closed shadow trees. The reference to the shadow root
 *      is held privately in this function and never exposed; even
 *      `host.shadowRoot` returns `null` because the shadow was created
 *      with `mode:"closed"`. Events fired from inside the shadow tree
 *      bubble out *retargeted* at the shadow host (a plain `<div>`), so
 *      extension listeners that key off global `input` events see activity
 *      on a `<div>` and cannot classify it as a password field either.
 *
 * Each layer addresses one constraint imposed by the layer below it:
 * the keyboard wants a text input (layer 1); the text input wants to be
 * smart (defeated by layer 2); the smart-input watchers want to be helpful
 * (defeated by layer 3).
 *
 * ## DOM picture
 *
 * ```
 * document.body
 *   └─ host <div>     ← 1×1 px transparent, aria-hidden
 *        └─ #shadow-root (closed)     ← invisible to extension queries
 *             └─ <input type="password" value=BASELINE>
 *                  ↑
 *                  OS keyboard delivers keystrokes here.
 *                  Each `input` event → diff(lastText, value) → pty bytes.
 * ```
 *
 * ## Keystroke pipeline (the "RestoreDOM" pattern, from Slate / CodeMirror)
 *
 * Mobile keyboards do not reliably fire `keydown` for soft keys (Android
 * typically reports `keyCode 229` with `key === "Unidentified"`;
 * autocorrect rewrites the field after the fact, batching changes). So we
 * do not listen to `keydown` for characters — we let the keyboard mutate
 * the input's `.value`, then compute what changed.
 *
 * The element is seeded with `BASELINE`: eight non-breaking-space padding
 * characters with the caret in the middle. On every `input` event we diff
 * the new value against `lastText` using a prefix-suffix match: the longest
 * common prefix and suffix are identified, and the differing middle is
 * "N characters deleted, M characters inserted". Those translate to N
 * `\x7f` bytes followed by the M inserted characters, dispatched to the
 * pty. The middle-of-padding caret causes a single typed character to land
 * cleanly between padding chars and produce a "delete 0, insert 1" diff.
 * The padding around the caret also gives Backspace something to delete on
 * an "empty" line, so the next `input` event reports a deletion that we
 * can dispatch as `\x7f` — without padding, Firefox Android / GBoard
 * silently no-op a backspace on empty content (w3c/input-events#75,
 * Bugzilla #1104817).
 *
 * The value is reset to BASELINE on Enter to bound growth and refill
 * backspace fodder for the next command.
 *
 * ## Consequence elsewhere: `document.activeElement` lies
 *
 * One side-effect of the closed shadow root: `document.activeElement`
 * returns the shadow *host* (not the input inside) when focus is "inside"
 * the shadow tree. This is a privacy property of closed shadow roots and
 * cannot be bypassed without exposing the shadow root reference. Three
 * call sites that previously asked "is the capture focused?" via
 * `document.activeElement === capture` would silently always evaluate to
 * false. They now read a mirrored `state.isFocused` flag that this
 * function keeps in sync via `focus` and `blur` listeners on the input.
 *
 * ## Wire path
 *
 * When soft keyboard mode is on (`__zjSoftKbdEnabled === true`),
 * `setSoftKeyboard` focuses this hidden input instead of xterm.js's
 * textarea. When soft keyboard mode is off, focus returns to xterm.js's
 * textarea (which keeps `inputmode="none"`), preserving hardware-keyboard
 * typing through xterm.js's normal `keydown` handler.
 *
 * ## Coexistence
 *
 *  - `installImeBypass` (desktop ibus/fcitx) operates on xterm.js's
 *    textarea, independent of this path.
 *  - The mobile plugin's modifier-bar cells (TAB, ESC, CTRL, ALT, arrows)
 *    are SGR mouse clicks on the terminal, routed to the plugin
 *    server-side, and write directly to the pane's pty — they do not
 *    touch the capture input.
 *
 * ## Idempotency
 *
 * `setupInputHandlers` runs twice (placeholder sender, then real WebSocket
 * sender post-`initWebSockets`); subsequent calls just refresh
 * `state.sendFn`.
 */
function installSoftKeyboardCapture(term, sendFunction) {
    if (!isCoarsePointerDevice()) {
        return;
    }
    if (typeof window.__zjSoftKbdCapture === "undefined") {
        window.__zjSoftKbdCapture = {
            installed: false,
            sendFn: sendFunction,
            element: null,
            // Mirrors the input's focus state. `document.activeElement`
            // returns the shadow host (not the input inside) when the
            // input lives in a closed shadow root, so identity-based
            // focus checks against `element` can't work directly. The
            // `focus`/`blur` listeners installed below keep this flag
            // in sync; all `is the capture focused?` checks read it.
            isFocused: false,
        };
    }
    window.__zjSoftKbdCapture.sendFn = sendFunction;

    if (window.__zjSoftKbdCapture.installed) {
        return;
    }
    window.__zjSoftKbdCapture.installed = true;
    const state = window.__zjSoftKbdCapture;

    // xterm.js considers the terminal "unfocused" whenever its
    // textarea loses focus to a different DOM element. The soft-
    // keyboard capture installed below takes focus on every tap
    // (the OS keyboard refuses to surface otherwise), so xterm.js
    // would render the cursor with `cursorInactiveStyle` (default
    // `"outline"`) instead of `cursorStyle` — and on a small mobile
    // viewport an outline-only rectangle reads as "no cursor at
    // all". Mirror the active style into the inactive slot so the
    // embedded pane's cursor stays visible while typing through the
    // capture input. Scoped to coarse-pointer devices (this whole
    // function is), so desktop's "click elsewhere → outline cursor"
    // affordance is preserved. Exposed on `window` so
    // `websockets.js`'s `SetConfig` handler can re-mirror after
    // user-config updates change `cursorStyle`.
    const syncInactiveCursorStyle = () => {
        const active = term.options.cursorStyle || "block";
        term.options.cursorInactiveStyle = active;
    };
    syncInactiveCursorStyle();
    window.__zjSyncInactiveCursorStyle = syncInactiveCursorStyle;

    // Padding of non-breaking spaces (U+00A0). The caret sits in
    // the middle of the padding run so a single typed character
    // lands cleanly between padding chars and produces a "delete
    // 0, insert 1" diff. The padding around the caret also gives
    // backspace something to delete on an "empty" line so the
    // next `input` event reports a deletion we can dispatch as
    // `\x7f` (see the function docstring for the Firefox Android
    // / GBoard no-op-on-empty bug this works around). 8 padding
    // chars is comfortable headroom for any plausible backspace
    // burst between two `input` events.
    const PADDING_CHAR =" ";

    const PADDING_LEN = 8;
    const CARET_OFFSET = PADDING_LEN / 2;
    const BASELINE = PADDING_CHAR.repeat(PADDING_LEN);

    // Build the capture element. See the function docstring for
    // the full architectural reasoning; the short version is
    // three stacked tricks:
    //
    //   1. Hidden text input — so the OS keyboard has a target.
    //   2. `type="password"` — so the keyboard does not run its
    //      smart layer (prediction / autocorrect / composition)
    //      on top.
    //   3. Closed shadow root — so password managers cannot find
    //      the field via `querySelectorAll('input[type=password]')`.
    //
    // Construction order: host → shadow → input. The host carries
    // the 1×1 transparent positioning so any popover the OS or an
    // extension still tries to anchor has nothing visible to
    // attach to. `mode:"closed"` means `host.shadowRoot` returns
    // `null` from outside; the only reference is `captureShadow`
    // below, held inside this closure.
    const captureHost = document.createElement("div");
    captureHost.id = "zj-mobile-capture-host";
    // Same 1×1 transparent positioning as the input itself, so the
    // host has no visible footprint and cannot be the target of
    // any autofill popover the manager might still try to anchor.
    captureHost.style.cssText =
        "position:fixed;top:0;left:0;" +
        "width:1px;height:1px;" +
        "opacity:0;pointer-events:none;" +
        "overflow:hidden;";
    captureHost.setAttribute("aria-hidden", "true");
    document.body.appendChild(captureHost);
    // `mode: "closed"` — the returned shadow root is the only way
    // to reach inside, and we hold that reference privately.
    // Extension content scripts have no way to obtain it.
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
    // Belt-and-braces opt-outs for managers that *do* pierce
    // shadow roots (some enterprise / niche forks) — cheap to add.
    div.setAttribute("data-1p-ignore", "true");
    div.setAttribute("data-lpignore", "true");
    div.setAttribute("data-bwignore", "true");
    div.setAttribute("data-form-type", "other");
    div.setAttribute("data-dashlane-ignore", "true");
    div.tabIndex = -1;
    // CSS that preserves focusability — `display:none` and
    // `visibility:hidden` would close the soft keyboard. 1×1 px,
    // fully transparent, pointer-events disabled so touches pass
    // through to the terminal below.
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

    // Mirror focus state into the shared global so other call
    // sites (touchstart focus-on-first-tap, ensureCaptureFocused)
    // can answer "is the capture currently focused?" without
    // relying on `document.activeElement` — which, with the
    // closed shadow root above, always returns the host, never
    // the input.
    div.addEventListener("focus", () => { state.isFocused = true; });
    div.addEventListener("blur", () => { state.isFocused = false; });

    const setCaretToMiddle = () => {
        try {
            const pos = Math.min(CARET_OFFSET, div.value.length);
            div.setSelectionRange(pos, pos);
        } catch (_) {
            // setSelectionRange throws if the element is not yet
            // attached / focusable; caret position is best-effort.
        }
    };

    const resetBaseline = () => {
        // Restore the padding-around-caret invariant. Called at
        // install time (initial seed) and on Enter (bound growth +
        // refill backspace fodder for the next command line). With
        // `type="password"` there is no IME composition state to
        // interrupt, so the assignment is a simple overwrite with
        // no observable side effects beyond the value change.
        div.value = BASELINE;
        setCaretToMiddle();
    };

    resetBaseline();

    // Tight per-character dedupe (8 ms). Some IMEs deliver the same
    // character via overlapping event paths within microseconds.
    // 8 ms is well under any key-autorepeat interval (30 ms+) so it
    // does not false-positive on held keys.
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

    // Diff previous value (`a`) vs the post-mutation value (`b`).
    // Both are `.value` strings on the capture `<input>`. Returns
    // the number of characters deleted from the middle of `a` and
    // the substring inserted in the middle of `b`, by walking the
    // longest common prefix and then the longest common suffix
    // (in the remaining span). Padding chars are stripped from the
    // insertion in the rare case the IME produces a no-op mutation
    // that swaps padding for itself — without the strip we would
    // dispatch the padding character as if the user had typed it.
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

    // Track the last observed value so the next `input` event can
    // compute the delta. With `type="password"` the keyboard does
    // not run a composition / prediction engine on top of the
    // field (see the function docstring for why), so each `input`
    // event represents one user-driven mutation — either a single
    // character insertion, a single backspace deletion, or a paste.
    // The value accumulates across keystrokes until Enter resets
    // it; that is harmless because there is no IME state on the
    // keyboard side that could later re-flush the accumulated
    // contents back into the dispatch path.
    let lastText = BASELINE;

    div.addEventListener("input", () => {
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
    });

    // Special keys: Enter / Tab / Escape / Arrows do not always
    // produce observable value mutations. Intercept in keydown and
    // dispatch the right escape sequence directly. Backspace is
    // handled by the input-event diff via the padding fodder.
    //
    // Enter additionally resets the textarea back to the padded
    // baseline, bounding how much the value can grow across a
    // session and refilling backspace fodder for the next command
    // line.
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

    // Mobile browsers ignore programmatic focus() outside of a user
    // gesture, so the plugin's startup `set_soft_keyboard(true)` call
    // (which lands in `setSoftKeyboard` and tries `capture.focus()`)
    // cannot actually summon the OS keyboard before the user has
    // touched the screen even once.
    //
    // Hook three events to recover. `click` is the primary path: it
    // bubbles AFTER xterm.js's mousedown-driven textarea-focus, so a
    // capture.focus() here wins the focus race and the browser
    // surfaces the OS keyboard inside the still-active gesture
    // window. `touchend` covers the case where the tap never resolves
    // to a click (long-press, scroll). `pointerdown` in capture phase
    // is a belt-and-braces fallback for browsers that synthesize
    // focus differently. All three are idempotent — once `div` owns
    // focus, every handler short-circuits.
    //
    // Gated on `__zjSoftKbdEnabled` so a user who disabled
    // soft-keyboard mode (2-finger gesture) is not re-summoned
    // against their will. `passive: true` keeps us out of the
    // scrolling/zooming gesture pipeline.
    const ensureCaptureFocused = () => {
        if (!window.__zjSoftKbdEnabled) {
            return;
        }
        // If the capture already has focus but the OS keyboard was
        // dismissed externally (Android back button on Firefox /
        // Chrome typically hides the keyboard without blurring the
        // focused element), a plain focus() is a no-op and the
        // keyboard stays hidden. A blur+focus sequence inside the
        // user gesture forces the browser to re-evaluate the focus
        // and re-summon the keyboard. Cheap; no observable side-
        // effects on the value-diff capture path (lastText, value,
        // and input listener all survive the blur/focus pair).
        // `state.isFocused` is used in place of an `activeElement`
        // identity check because the closed shadow root above
        // means `document.activeElement` always returns the host,
        // not the input inside.
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
    // Default to "soft keyboard wanted" so the first-touch focus
    // fallback in `installSoftKeyboardCapture` works even if the
    // mobile plugin's startup `set_soft_keyboard(true)` IPC message
    // hasn't arrived (or never arrives — e.g. plain ssh sessions
    // attached via the web client). Users who explicitly disable via
    // the 2-finger toggle flip this back to false.
    window.__zjSoftKbdEnabled = true;
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
 * available". Order matters — most permissive checks last so the
 * cheap matchMedia hit fires first on real mobile browsers.
 *
 * Coverage rationale:
 * - `pointer: coarse` — Chrome/Safari on iOS/Android, also Chrome
 *   DevTools mobile emulation.
 * - `maxTouchPoints > 0` — multi-touch capable surfaces; catches
 *   WebViews and in-app browsers that don't report pointer:coarse.
 * - `'ontouchstart' in window` — legacy touch event API; catches
 *   older WebViews and embedded browsers that lack maxTouchPoints.
 * - UA regex — final fallback for browsers with quirky media query
 *   support (older WebKit, custom shells).
 */
function isCoarsePointerDevice() {
    if (
        window.matchMedia &&
        window.matchMedia("(pointer: coarse)").matches
    ) {
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
    // Mechanics: the soft keyboard is summoned by focusing an element
    // with a text-capable `inputmode`. We route summoning through the
    // dedicated hidden capture `<input type="password">` (installed
    // by `installSoftKeyboardCapture`) rather than xterm.js's
    // textarea, so soft-keyboard keystrokes flow through the
    // value-diff capture path (see that function's docstring for why
    // the field is a password input wrapped in a closed shadow root).
    // xterm.js's textarea keeps `inputmode="none"` permanently and
    // only takes focus when soft keyboard mode is off, so
    // hardware-keyboard typing still flows through xterm.js's normal
    // keydown handler.
    const capture = window.__zjSoftKbdCapture && window.__zjSoftKbdCapture.element;
    if (on) {
        if (capture) {
            capture.focus();
        } else {
            // Fallback if the capture input failed to install
            // (shouldn't happen on coarse-pointer): summon via the
            // textarea, accepting the keydown-reliability tradeoff.
            ta.removeAttribute("inputmode");
            ta.focus();
        }
    } else {
        if (capture) {
            capture.blur();
        }
        // Re-focus xterm.js's textarea so attached hardware keyboards
        // continue to drive xterm's normal keydown handler. The
        // textarea's `inputmode="none"` (set by `suppressSoftKeyboardOnTouch`)
        // prevents focusing it from re-summoning the soft keyboard.
        ta.setAttribute("inputmode", "none");
        ta.focus();
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
