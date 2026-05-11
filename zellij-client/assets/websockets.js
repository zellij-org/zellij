/**
 * WebSocket management for terminal and control connections
 */

import { handleReconnection, handleDisconnected, markConnectionEstablished } from "./connection.js";
import { getBaseUrl, getWebSocketBaseUrl } from "./utils.js";

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
function sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols) {
    if (!wsControl || !ownWebClientId) {
        return;
    }
    wsControl.send(
        JSON.stringify({
            web_client_id: ownWebClientId,
            payload: {
                type: "TerminalResize",
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
            const body = document.querySelector("body");
            body.style.background = theme.background || "black";

            const terminal = document.getElementById("terminal");
            terminal.style.background = theme.background;

            const fitDimensions = fitAddon.proposeDimensions();
            if (fitDimensions === undefined) {
                console.warn("failed to get new fit dimensions");
                return;
            }

            const { rows, cols } = fitDimensions;
            if (rows !== term.rows || cols !== term.cols) {
                term.resize(cols, rows);
            }
            // Always emit a size update on SetConfig: even if the grid
            // didn't change, font metrics may have shifted and the
            // pixel-cell measurements in TerminalMetrics need to
            // refresh so host-terminal queries get accurate values.
            sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
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

    const updateViewportVars = () => {
        const root = document.documentElement;
        const viewport = window.visualViewport;
        const height = viewport ? viewport.height : window.innerHeight;
        const width = viewport ? viewport.width : window.innerWidth;
        root.style.setProperty("--dynamic-vh", `${height}px`);
        root.style.setProperty("--dynamic-vw", `${width}px`);
    };

    const resizeTerminal = () => {
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
        sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
    };

    const handleViewportChange = () => {
        updateViewportVars();
        resizeTerminal();
    };

    const scheduleResize = () => {
        if (resizeScheduled) {
            return;
        }
        resizeScheduled = true;
        requestAnimationFrame(() => {
            resizeScheduled = false;
            handleViewportChange();
        });
    };

    updateViewportVars();
    addEventListener("resize", scheduleResize);
    if (window.visualViewport) {
        window.visualViewport.addEventListener("resize", scheduleResize);
    }
}
