import { handleReconnection, handleDisconnected, markConnectionEstablished } from "./connection.js";
import { getBaseUrl, getWebSocketBaseUrl } from "./utils.js";
import { setSoftKeyboard } from "./input.js";
import { applyFontSize } from "./terminal.js";

const NATURAL_MIN_TOTAL_ROWS = 25;
const MOBILE_LEGIBLE_FLOOR_PX = 16;

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

function sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols, cause) {
    if (!wsControl || !ownWebClientId) {
        return;
    }
    const resizeType =
        cause === "RenderingPreference"
            ? "TerminalResizeRendering"
            : cause === "Settled"
            ? "TerminalSizeSettled"
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

    const originalSendAnsiKey = sendAnsiKey;
    sendAnsiKey = (ansiKey) => {
        if (ownWebClientId !== "") {
            wsTerminal.send(ansiKey);
        }
    };

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
            if (typeof window.__zjSyncInactiveCursorStyle === "function") {
                window.__zjSyncInactiveCursorStyle();
            }
            const isMobileViewport =
                (window.matchMedia &&
                    window.matchMedia("(pointer: coarse)").matches &&
                    window.innerWidth < 600) ||
                /Mobi|Android|iPhone|iPad/i.test(navigator.userAgent);
            const hasExplicitFontSize =
                typeof font_size === "number" && font_size > 0;
            const baseFontPx = hasExplicitFontSize
                ? font_size
                : isMobileViewport
                ? 24
                : 12;
            applyFontSize(term, fitAddon, baseFontPx);
            const needsMobileDownscale =
                !hasExplicitFontSize &&
                isMobileViewport &&
                term.rows < NATURAL_MIN_TOTAL_ROWS;
            if (needsMobileDownscale) {
                const downscaledPx = Math.max(
                    Math.floor(
                        (baseFontPx * term.rows) / NATURAL_MIN_TOTAL_ROWS
                    ),
                    MOBILE_LEGIBLE_FLOOR_PX
                );
                if (downscaledPx < baseFontPx) {
                    applyFontSize(term, fitAddon, downscaledPx);
                }
            }
            const body = document.querySelector("body");
            body.style.background = theme.background || "black";

            const terminal = document.getElementById("terminal");
            terminal.style.background = theme.background;

            sendSizeUpdate(
                wsControl,
                ownWebClientId,
                term,
                term.rows,
                term.cols,
                "Settled"
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

export function setupResizeHandler(
    term,
    fitAddon,
    getWsControl,
    getOwnWebClientId
) {
    let resizeScheduled = false;
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

        const wsControl = getWsControl();
        term.resize(cols, rows);

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
    addEventListener("zellij:rendering-resize", scheduleRenderingResize);

    setupSoftKeyboardVisibilityTracker(getWsControl, getOwnWebClientId);
}

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
