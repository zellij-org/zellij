import { handleReconnection, handleDisconnected, markConnectionEstablished } from "./connection.js";
import {
    getBaseUrl,
    getWebSocketBaseUrl,
    isCurrentLocation,
} from "./utils.js";
import { setSoftKeyboard } from "./input.js";
import {
    applyTerminalBackground,
    terminalConfigOptions,
} from "./terminal.js";

let lastSentCellDimensions = null;

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

function getMobileRenderSizing() {
    return window.__zjMobileUi && window.__zjMobileUi.getRenderSizing
        ? window.__zjMobileUi.getRenderSizing()
        : { pinned: false };
}

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
    const cellWidth = Math.round(cell.width);
    const cellHeight = Math.round(cell.height);
    if (
        lastSentCellDimensions &&
        lastSentCellDimensions.width === cellWidth &&
        lastSentCellDimensions.height === cellHeight
    ) {
        return;
    }
    lastSentCellDimensions = { width: cellWidth, height: cellHeight };
    wsControl.send(
        JSON.stringify({
            web_client_id: ownWebClientId,
            payload: {
                type: "TerminalMetrics",
                cell_pixel_width: cellWidth,
                cell_pixel_height: cellHeight,
                text_area_pixel_width: Math.round(cols * cell.width),
                text_area_pixel_height: Math.round(rows * cell.height),
            },
        })
    );
}

export function initWebSockets(boot, term, fitAddon) {
    const ownWebClientId = boot.web_client_id;
    const sessionName = boot.session_name;
    let wsTerminal;
    let wsControl;
    const userConfig = { blink: false, style: false };
    if (boot.config) {
        if (typeof boot.config.cursor_blink !== "undefined") {
            userConfig.blink = true;
        }
        if (typeof boot.config.cursor_style !== "undefined") {
            userConfig.style = true;
        }
    }

    const wsBaseUrl = getWebSocketBaseUrl();
    const url =
        !sessionName || sessionName === ""
            ? `${wsBaseUrl}/ws/terminal`
            : `${wsBaseUrl}/ws/terminal/${encodeURIComponent(sessionName)}`;

    const fitDimensions = fitAddon.proposeDimensions() || {
        rows: term.rows,
        cols: term.cols,
    };
    if (
        fitDimensions.rows !== term.rows ||
        fitDimensions.cols !== term.cols
    ) {
        term.resize(fitDimensions.cols, fitDimensions.rows);
    }
    const cell = getCellPixelDimensions(term);
    let queryString = `?web_client_id=${encodeURIComponent(ownWebClientId)}&rows=${term.rows}&cols=${term.cols}`;
    if (cell) {
        const cellWidth = Math.round(cell.width);
        const cellHeight = Math.round(cell.height);
        queryString += `&cell_width=${cellWidth}&cell_height=${cellHeight}`;
        lastSentCellDimensions = { width: cellWidth, height: cellHeight };
    }
    const wsTerminalUrl = `${url}${queryString}`;

    wsTerminal = new WebSocket(wsTerminalUrl);

    wsControl = new WebSocket(
        `${wsBaseUrl}/ws/control?web_client_id=${encodeURIComponent(ownWebClientId)}`
    );
    startWsControl(wsControl, term, fitAddon, ownWebClientId, userConfig);
    window.__zjSendControl = function (payload) {
        if (wsControl && wsControl.readyState === WebSocket.OPEN) {
            wsControl.send(
                JSON.stringify({
                    web_client_id: ownWebClientId,
                    payload,
                })
            );
        }
    };

    wsTerminal.onopen = function () {
        markConnectionEstablished();
    };

    wsTerminal.onmessage = function (event) {
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

    const sendAnsiKey = (ansiKey) => {
        let payload = ansiKey;
        if (typeof window.__zjMobileMergeKey === "function") {
            payload = window.__zjMobileMergeKey(payload);
        }
        wsTerminal.send(payload);
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
    wsControl.onmessage = function (event) {
        const msg = JSON.parse(event.data);
        if (msg.type === "SetConfig") {
            const options = terminalConfigOptions(msg);
            for (const key of Object.keys(options)) {
                term.options[key] = options[key];
            }
            if (typeof msg.cursor_blink !== "undefined") {
                userConfig.blink = true;
            }
            if (typeof msg.cursor_style !== "undefined") {
                userConfig.style = true;
            }
            if (typeof window.__zjSyncInactiveCursorStyle === "function") {
                window.__zjSyncInactiveCursorStyle();
            }
            applyTerminalBackground(msg);
        } else if (msg.type === "QueryTerminalSize") {
            const sizing = getMobileRenderSizing();
            if (sizing.pinned) {
                if (sizing.rows !== term.rows || sizing.cols !== term.cols) {
                    term.resize(sizing.cols, sizing.rows);
                }
                sendSizeUpdate(
                    wsControl,
                    ownWebClientId,
                    term,
                    sizing.rows,
                    sizing.cols
                );
            } else {
                const fitDimensions = fitAddon.proposeDimensions();
                const { rows, cols } = fitDimensions;
                if (rows !== term.rows || cols !== term.cols) {
                    term.resize(cols, rows);
                }
                sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
            }
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
            const target = `${baseUrl}/${encodeURIComponent(new_session_name)}`;
            if (!isCurrentLocation(target)) {
                history.pushState(null, "", target);
                document.title = new_session_name;
            }
        } else if (msg.type === "SetSoftKeyboard") {
            const { on } = msg;
            setSoftKeyboard(term, !!on);
        } else if (msg.type === "MobileState") {
            const { payload } = msg;
            if (payload) {
                window.__zjLastMobileState = payload;
                if (window.__zjMobileUi) {
                    window.__zjMobileUi.setData(payload);
                }
            }
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
    let pendingResizeSignal = false;

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

        const sizing = getMobileRenderSizing();

        if (sizing.pinned) {
            if (sizing.rows !== term.rows || sizing.cols !== term.cols) {
                term.resize(sizing.cols, sizing.rows);
            }
            if (window.__zjMobilePan) {
                window.__zjMobilePan.recompute();
            }
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

        sendSizeUpdate(wsControl, ownWebClientId, term, rows, cols);
    };

    const handleViewportChange = () => {
        updateViewportVars();
        resizeTerminal();
    };

    const scheduleResize = () => {
        pendingResizeSignal = true;
        if (resizeScheduled) {
            return;
        }
        resizeScheduled = true;
        requestAnimationFrame(() => {
            resizeScheduled = false;
            if (!pendingResizeSignal) {
                return;
            }
            pendingResizeSignal = false;
            handleViewportChange();
        });
    };

    updateViewportVars();
    addEventListener("resize", scheduleResize);
    if (window.visualViewport) {
        window.visualViewport.addEventListener("resize", scheduleResize);
    }
    addEventListener("zellij:rendering-resize", scheduleResize);

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

        window.dispatchEvent(
            new CustomEvent("zellij:soft-keyboard-visibility", {
                detail: { visible: kbdVisible },
            })
        );

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
