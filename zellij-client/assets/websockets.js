/**
 * WebSocket management for terminal and control connections
 */

import { handleReconnection, handleDisconnected, markConnectionEstablished } from "./connection.js";
import { getBaseUrl, getWebSocketBaseUrl } from "./utils.js";
import { setSoftKeyboard } from "./input.js";
import { applyFontSize } from "./terminal.js";

const NATURAL_MIN_TOTAL_ROWS = 25;
const MOBILE_LEGIBLE_FLOOR_PX = 16;
const MOBILE_ADAPTIVE_MAX_ITERATIONS = 4;

/**
 * Read cell pixel dimensions from xterm.js. Tries the internal
 * _renderService first (matches what the vendored FitAddon uses) and
 * falls back to a DOM measurement of .xterm-char-measure-element — a
 * hidden helper element xterm.js creates explicitly for character
 * measurement. Returns null if neither path yields usable numbers.
 */
function getCellPixelDimensions(term) {
    try {
        const cell =
            term && term._core && term._core._renderService &&
            term._core._renderService.dimensions &&
            term._core._renderService.dimensions.css &&
            term._core._renderService.dimensions.css.cell;
        if (cell && cell.width && cell.height) {
            return { width: cell.width, height: cell.height };
        }
    } catch (_) {}
    const el = term && term.element &&
        term.element.querySelector(".xterm-char-measure-element");
    if (el) {
        const rect = el.getBoundingClientRect();
        if (rect.width && rect.height) {
            return { width: rect.width, height: rect.height };
        }
    }
    return null;
}

/**
 * Send both control messages that describe the client's display state
 * to the Zellij server: TerminalResize (grid rows/cols) and
 * TerminalMetrics (pixel dimensions used to answer host-terminal
 * queries such as CSI 14t / 16t and OSC 11;?).
 *
 * Single chokepoint so the protocol contract lives in one place — any
 * site that updates terminal size or theme must call this helper, and
 * any future field added to the protocol is added here once. The
 * server's TerminalResize handler is idempotent, so calling this even
 * when the grid hasn't changed (e.g. after a theme reload that only
 * shifts font metrics) is safe.
 */
function sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols, cause) {
    if (!wsControl || !ownWebClientId) {
        return;
    }
    // The payload `type` discriminator is what the server bridge
    // (web_client/websocket_handlers.rs) translates into
    // `ResizeCause::Viewport` vs `ResizeCause::RenderingPreference`.
    // Only the former triggers the server's mobile-mode re-evaluation.
    const resizeType =
        cause === "RenderingPreference"
            ? "TerminalResizeRendering"
            : "TerminalResize";
    wsControl.send(
        JSON.stringify({
            web_client_id: ownWebClientId,
            payload: {
                type: resizeType,
                rows,
                cols,
            },
        })
    );
    const cell = getCellPixelDimensions(term);
    if (!cell) {
        return;
    }
    wsControl.send(
        JSON.stringify({
            web_client_id: ownWebClientId,
            payload: {
                type: "TerminalMetrics",
                cell_pixel_width: Math.round(cell.width),
                cell_pixel_height: Math.round(cell.height),
                text_area_pixel_width: Math.round(cols * cell.width),
                text_area_pixel_height: Math.round(rows * cell.height),
            },
        })
    );
}

/**
 * Initialize both terminal and control WebSocket connections
 * @param {string} webClientId - Client ID from authentication
 * @param {string} sessionName - Session name from URL
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {function} sendAnsiKey - Function to send ANSI key sequences
 * @returns {object} Object containing WebSocket instances and cleanup function
 */
export function initWebSockets(
    webClientId,
    sessionName,
    term,
    fitAddon,
    sendAnsiKey
) {
    let ownWebClientId = "";
    let wsTerminal;
    let wsControl;
    const userConfig = { blink: false, style: false };

    const wsBaseUrl = getWebSocketBaseUrl();
    const url =
        sessionName === ""
            ? `${wsBaseUrl}/ws/terminal`
            : `${wsBaseUrl}/ws/terminal/${sessionName}`;

    const queryString = `?web_client_id=${encodeURIComponent(webClientId)}`;
    const wsTerminalUrl = `${url}${queryString}`;

    wsTerminal = new WebSocket(wsTerminalUrl);

    wsTerminal.onopen = function () {
        markConnectionEstablished();
    };

    wsTerminal.onmessage = function (event) {
        if (ownWebClientId == "") {
            ownWebClientId = webClientId;
            const wsControlUrl = `${wsBaseUrl}/ws/control`;
            wsControl = new WebSocket(wsControlUrl);
            startWsControl(wsControl, term, fitAddon, ownWebClientId, userConfig);
        }

        let data = event.data;

        if (typeof data === "string") {
            // Handle ANSI title change sequences
            const titleRegex = /\x1b\]0;([^\x07\x1b]*?)(?:\x07|\x1b\\)/g;
            let match;
            while ((match = titleRegex.exec(data)) !== null) {
                document.title = match[1];
            }

            if ((userConfig.blink || userConfig.style) && (
                data.includes("\x1b[0 q") ||
                data.includes("\x1b[1 q") ||
                data.includes("\x1b[2 q") ||
                data.includes("\x1b[3 q") ||
                data.includes("\x1b[4 q") ||
                data.includes("\x1b[5 q") ||
                data.includes("\x1b[6 q")
            )) {
                data = data.replace(/\x1b\[([0-6]) q/g, (match, p1) => {
                    const id = parseInt(p1);

                    // Decode app-requested blink and shape from DECSCUSR id
                    // id 0 = reset-to-default (null = no preference)
                    const appBlink = id === 0 ? null : (id % 2 === 1);
                    const appShapes = [null, "block", "block", "underline", "underline", "bar", "bar"];
                    const appShape  = appShapes[id];

                    // Apply user overrides only for what was explicitly configured;
                    // otherwise pass through the app's value (or fall back to term.options)
                    const effectiveBlink = userConfig.blink ? term.options.cursorBlink
                                                            : (appBlink !== null ? appBlink : term.options.cursorBlink);
                    const effectiveShape = userConfig.style ? term.options.cursorStyle
                                                            : (appShape !== null ? appShape : term.options.cursorStyle);

                    if (effectiveShape === "block")     return effectiveBlink ? "\x1b[1 q" : "\x1b[2 q";
                    if (effectiveShape === "underline") return effectiveBlink ? "\x1b[3 q" : "\x1b[4 q";
                    if (effectiveShape === "bar")       return effectiveBlink ? "\x1b[5 q" : "\x1b[6 q";
                    return match;
                });
            }
        }

        term.write(data);
    };

    wsTerminal.onclose = function (event) {
        if (event.code === 4001) {
            handleDisconnected();
        } else {
            handleReconnection();
        }
    };

    // Update sendAnsiKey to use the actual WebSocket
    const originalSendAnsiKey = sendAnsiKey;
    sendAnsiKey = (ansiKey) => {
        if (ownWebClientId !== "") {
            wsTerminal.send(ansiKey);
        }
    };

    // Setup resize handler
    setupResizeHandler(
        term,
        fitAddon,
        () => wsControl,
        () => ownWebClientId
    );

    return {
        wsTerminal,
        getWsControl: () => wsControl,
        getOwnWebClientId: () => ownWebClientId,
        sendAnsiKey,
        cleanup: () => {
            if (wsTerminal) {
                wsTerminal.close();
            }
            if (wsControl) {
                wsControl.close();
            }
        },
    };
}

/**
 * Start the control WebSocket and set up its handlers
 * @param {WebSocket} wsControl - Control WebSocket instance
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {string} ownWebClientId - Own web client ID
 */
function startWsControl(wsControl, term, fitAddon, ownWebClientId, userConfig) {
    wsControl.onopen = function (event) {
        const fitDimensions = fitAddon.proposeDimensions();
        const { rows, cols } = fitDimensions;
        sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
    };

    wsControl.onmessage = function (event) {
        const msg = JSON.parse(event.data);
        if (msg.type === "SetConfig") {
            const {
                font,
                theme,
                cursor_blink,
                mac_option_is_meta,
                cursor_style,
                cursor_inactive_style,
                font_size,
            } = msg;
            term.options.fontFamily = font;
            term.options.theme = theme;
            if (cursor_blink !== "undefined") {
                term.options.cursorBlink = cursor_blink;
                userConfig.blink = true;
            }
            if (mac_option_is_meta !== "undefined") {
                term.options.macOptionIsMeta = mac_option_is_meta;
            }
            if (cursor_style !== "undefined") {
                term.options.cursorStyle = cursor_style;
                userConfig.style = true;
            }
            if (cursor_inactive_style !== "undefined") {
                term.options.cursorInactiveStyle = cursor_inactive_style;
            }
            // The soft-keyboard capture (installed by `input.js`) takes
            // focus on every tap, so xterm.js renders the cursor with
            // `cursorInactiveStyle` rather than `cursorStyle` on mobile.
            // After a user-config update has potentially changed
            // `cursorStyle`, re-mirror it into the inactive slot so the
            // cursor stays visible. The sync function is installed only
            // on coarse-pointer devices, so this is a no-op on desktop.
            // Runs after both branches above so an explicit
            // `cursor_inactive_style` from the SetConfig payload is
            // overridden — the soft-keyboard's focus dance makes the
            // "inactive" rendering unavoidable on touch, and the only
            // way to honour the user's intent for a visible cursor is
            // to ignore an explicit inactive-style preference here.
            if (typeof window.__zjSyncInactiveCursorStyle === "function") {
                window.__zjSyncInactiveCursorStyle();
            }
            // Font size: explicit config wins, otherwise pick a default
            // suited to the device. Mobile heuristic: coarse pointer
            // (touch) AND a narrow viewport, OR a UA string that
            // identifies a known mobile platform.
            //
            // Mobile starts from 24 px and then steps down adaptively
            // until either the resulting grid hosts the mobile-plugin
            // keyboard's natural tier (>= NATURAL_MIN_TOTAL_ROWS) or
            // we hit MOBILE_LEGIBLE_FLOOR_PX. The terminal element's
            // CSS height is bound to 100dvh (style.css), so
            // `fitAddon.proposeDimensions()` already measures against
            // the *visible* canvas — the earlier off-screen-keyboard
            // bug (where 18 px proposed more rows than visualViewport
            // could host) is prevented by the dynamic viewport CSS,
            // not by the font size floor.
            const isMobileViewport =
                (window.matchMedia &&
                    window.matchMedia("(pointer: coarse)").matches &&
                    window.innerWidth < 600) ||
                /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent);
            const hasExplicitFontSize =
                typeof font_size === "number" && font_size > 0;
            const initialCandidate = hasExplicitFontSize
                ? font_size
                : isMobileViewport
                ? 24
                : 12;
            // Capture dims BEFORE applyFontSize so the post-fit
            // comparison detects any change driven by the font-size
            // swap. applyFontSize calls `fitAddon.fit()` internally,
            // which synchronously mutates term.rows/term.cols. A
            // previous implementation read proposeDimensions() AFTER
            // applyFontSize and compared against the already-mutated
            // term.rows/term.cols — the comparison short-circuited,
            // no resize was broadcast, and the server kept rendering
            // the plugin pane at the original (pre-font-change)
            // dimensions. On mobile that meant the server sent a
            // 46×38 grid to a 28×23 client and the on-screen content
            // landed at the wrong rows. Capturing prev dims and
            // comparing afterwards fixes that gap.
            const prevRows = term.rows;
            const prevCols = term.cols;
            applyFontSize(term, fitAddon, initialCandidate);
            // Adaptive walk: only when no explicit font_size was
            // configured AND the device looks mobile. If the natural
            // tier already fits at the starting candidate, the loop
            // exits immediately and the font stays at 24 px.
            if (!hasExplicitFontSize && isMobileViewport) {
                let candidate = initialCandidate;
                for (
                    let i = 0;
                    i < MOBILE_ADAPTIVE_MAX_ITERATIONS &&
                    term.rows < NATURAL_MIN_TOTAL_ROWS &&
                    candidate > MOBILE_LEGIBLE_FLOOR_PX;
                    i++
                ) {
                    // Cell height is quasi-linear in font size, so
                    // scaling the candidate by (current_rows /
                    // target_rows) lands close to the desired row
                    // count in one pass. Round down so we err on the
                    // smaller-font / more-rows side, then clamp at
                    // the legibility floor.
                    const scaled = Math.floor(
                        (candidate * term.rows) / NATURAL_MIN_TOTAL_ROWS
                    );
                    const next = Math.max(scaled, MOBILE_LEGIBLE_FLOOR_PX);
                    if (next >= candidate) {
                        // No forward progress (e.g. already at the
                        // floor, or rounding produced a no-op). Stop
                        // to avoid an infinite-loop edge case.
                        break;
                    }
                    candidate = next;
                    applyFontSize(term, fitAddon, candidate);
                }
            }
            const body = document.querySelector("body");
            body.style.background = theme.background || "black";

            const terminal = document.getElementById("terminal");
            terminal.style.background = theme.background;

            const newRows = term.rows;
            const newCols = term.cols;
            if (newRows === prevRows && newCols === prevCols) {
                return;
            }
            // Tag as a *rendering* resize so the server's mobile-mode
            // re-evaluation does NOT fire. SetConfig changing the font
            // size is a rendering preference, not a device-viewport
            // change — the device is the same screen, just with
            // different cell pixel dimensions. The server still needs
            // to know about the new cell count so it lays out the
            // panes (and therefore serialises payloads) at the
            // client's actual grid size. The TerminalMetrics half of
            // sendSizeUpdate also refreshes — font metrics may have
            // shifted even when the grid count happens to match.
            sendSizeUpdate(
                wsControl,
                ownWebClientId,
                term,
                newRows,
                newCols,
                "RenderingPreference"
            );
        } else if (msg.type === "QueryTerminalSize") {
            const fitDimensions = fitAddon.proposeDimensions();
            const { rows, cols } = fitDimensions;
            if (rows !== term.rows || cols !== term.cols) {
                term.resize(cols, rows);
            }
            sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
        } else if (msg.type === "Log") {
            const { lines } = msg;
            for (const line in lines) {
                console.log(line);
            }
        } else if (msg.type === "LogError") {
            const { lines } = msg;
            for (const line in lines) {
                console.error(line);
            }
        } else if (msg.type === "SwitchedSession") {
            const { new_session_name } = msg;
            const baseUrl = getBaseUrl();
            window.location.href = `${baseUrl}/${encodeURIComponent(new_session_name)}`;
        } else if (msg.type === "SetSoftKeyboard") {
            // The server (driven by the mobile plugin's load() call)
            // wants the soft keyboard either shown or hidden. On
            // desktops `setSoftKeyboard` no-ops; on touch devices it
            // focuses or blurs the dedicated capture <textarea> so
            // the OS keyboard surfaces or dismisses.
            const { on } = msg;
            setSoftKeyboard(term, !!on);
        }
    };

    wsControl.onclose = function (event) {
        if (event.code === 4001) {
            handleDisconnected();
        } else {
            handleReconnection();
        }
    };
}

/**
 * Set up window resize event handler
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {function} getWsControl - Function that returns control WebSocket
 * @param {function} getOwnWebClientId - Function that returns own web client ID
 */
export function setupResizeHandler(
    term,
    fitAddon,
    getWsControl,
    getOwnWebClientId
) {
    let resizeScheduled = false;
    // Tracks what caused the pending resize to be scheduled. Each
    // tick collects 0..N signals from different sources (window
    // resize, visualViewport resize, pinch). Viewport wins if it
    // arrived at any point during the tick — only a tick whose
    // *every* signal was rendering-only is reported as
    // RenderingPreference to the server. That conservative
    // collapse means a true device-side viewport change is never
    // silently re-labelled as cosmetic, even if a pinch fires in
    // the same animation frame.
    let pendingViewportSignal = false;
    let pendingRenderingSignal = false;

    const updateViewportVars = () => {
        const root = document.documentElement;
        const viewport = window.visualViewport;
        const height = viewport ? viewport.height : window.innerHeight;
        const width = viewport ? viewport.width : window.innerWidth;
        root.style.setProperty("--dynamic-vh", `${height}px`);
        root.style.setProperty("--dynamic-vw", `${width}px`);
    };

    const resizeTerminal = (cause) => {
        const ownWebClientId = getOwnWebClientId();
        if (ownWebClientId === "") {
            return;
        }

        const fitDimensions = fitAddon.proposeDimensions();
        if (fitDimensions === undefined) {
            console.warn("failed to get new fit dimensions");
            return;
        }

        const { rows, cols } = fitDimensions;
        if (rows === term.rows && cols === term.cols) {
            return;
        }

        term.resize(cols, rows);

        const wsControl = getWsControl();
        sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols, cause);
    };

    const handleViewportChange = (cause) => {
        updateViewportVars();
        resizeTerminal(cause);
    };

    const scheduleResize = (cause) => {
        if (cause === "RenderingPreference") {
            pendingRenderingSignal = true;
        } else {
            pendingViewportSignal = true;
        }
        if (resizeScheduled) {
            return;
        }
        resizeScheduled = true;
        requestAnimationFrame(() => {
            // Resolve the cause for this tick. Mixed signals fall
            // back to Viewport (safer: a real device change wins).
            const tickCause =
                pendingRenderingSignal && !pendingViewportSignal
                    ? "RenderingPreference"
                    : "Viewport";
            pendingViewportSignal = false;
            pendingRenderingSignal = false;
            resizeScheduled = false;
            handleViewportChange(tickCause);
        });
    };

    const scheduleViewportResize = () => scheduleResize("Viewport");
    const scheduleRenderingResize = () => scheduleResize("RenderingPreference");

    updateViewportVars();
    addEventListener("resize", scheduleViewportResize);
    if (window.visualViewport) {
        window.visualViewport.addEventListener(
            "resize",
            scheduleViewportResize
        );
    }
    // The pinch handler in `input.js` fires this custom event
    // instead of a plain "resize" so the server can distinguish
    // a cosmetic font-size-driven grid change from a real
    // device-viewport change. Without the distinction, pinching
    // would push the mobile client past the threshold and
    // auto-demote it out of the mobile layout.
    addEventListener("zellij:rendering-resize", scheduleRenderingResize);

    setupSoftKeyboardVisibilityTracker(getWsControl, getOwnWebClientId);
}

/**
 * Watch `window.visualViewport.height` for shrink/grow swings and
 * report OS soft-keyboard visibility changes to the server over the
 * control WebSocket. The server forwards each report to subscribed
 * plugins as `Event::SoftKeyboardVisibilityChanged(visible)` so the
 * mobile plugin can show/hide its modifier bar in lockstep with the
 * OS keyboard.
 *
 * Heuristic: a one-shot delta of more than 150 px between consecutive
 * resize events is treated as keyboard show (shrink) or dismiss
 * (grow). 150 px sits above typical mobile URL-bar shrink (~50–100
 * px) and well below typical soft-keyboard heights (~250–400 px), so
 * routine address-bar transitions do not false-trigger.
 *
 * Device rotation can spuriously trip the threshold once, but the
 * follow-up gesture (re-tap to summon the keyboard, or the user
 * dismissing) corrects the state. False positives are self-healing.
 *
 * Also: on dismiss-detection we blur the dedicated capture textarea
 * so the next user tap goes through a fresh focus() that re-summons
 * the OS keyboard. Without this, the textarea stays focused after an
 * external dismissal (Android back button on Firefox / Chrome), and
 * a plain focus() on an already-focused element is a no-op for OS
 * keyboard summon. `input.js` also handles this defensively inside
 * `ensureCaptureFocused` (blur+focus on each gesture), so the two
 * mechanisms reinforce each other.
 *
 * No-op on devices without `visualViewport`.
 */
function setupSoftKeyboardVisibilityTracker(getWsControl, getOwnWebClientId) {
    if (!window.visualViewport) {
        return;
    }
    const VIEWPORT_DELTA_THRESHOLD_PX = 150;
    let lastViewportHeight = window.visualViewport.height;
    let kbdVisible = false;

    const onResize = () => {
        const newHeight = window.visualViewport.height;
        const delta = newHeight - lastViewportHeight;
        let newKbdVisible = kbdVisible;
        if (delta < -VIEWPORT_DELTA_THRESHOLD_PX) {
            newKbdVisible = true;
        } else if (delta > VIEWPORT_DELTA_THRESHOLD_PX) {
            newKbdVisible = false;
        }
        lastViewportHeight = newHeight;
        if (newKbdVisible === kbdVisible) {
            return;
        }
        kbdVisible = newKbdVisible;

        // Blur the capture input on external dismiss so the next
        // user tap can re-focus it and re-summon the OS keyboard.
        // The capture lives inside a closed shadow root, so
        // `document.activeElement` always returns the shadow host
        // when focus is "inside" — we rely on the mirrored
        // `isFocused` flag installed by installSoftKeyboardCapture.
        if (!kbdVisible) {
            const capture =
                window.__zjSoftKbdCapture &&
                window.__zjSoftKbdCapture.element;
            if (capture && window.__zjSoftKbdCapture.isFocused) {
                capture.blur();
            }
        }

        const wsControl = getWsControl();
        const ownWebClientId = getOwnWebClientId();
        if (!wsControl || ownWebClientId === "") {
            return;
        }
        wsControl.send(
            JSON.stringify({
                web_client_id: ownWebClientId,
                payload: {
                    type: "SoftKeyboardVisibilityChanged",
                    visible: kbdVisible,
                },
            })
        );
    };

    window.visualViewport.addEventListener("resize", onResize);
}
