/**
 * WebSocket management for terminal and control connections
 */

import { is_https } from './utils.js';
import { handleReconnection, markConnectionEstablished } from './connection.js';

/**
 * Initialize both terminal and control WebSocket connections
 * @param {string} webClientId - Client ID from authentication
 * @param {string} sessionName - Session name from URL
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {function} sendAnsiKey - Function to send ANSI key sequences
 * @returns {object} Object containing WebSocket instances and cleanup function
 */
export function initWebSockets(webClientId, sessionName, term, fitAddon, sendAnsiKey) {
    let ownWebClientId = "";
    let wsTerminal;
    let wsControl;
    
    const wsUrlPrefix = is_https() ? "wss" : "ws";
    const url = sessionName === ""
        ? `${wsUrlPrefix}://${window.location.host}/ws/terminal`
        : `${wsUrlPrefix}://${window.location.host}/ws/terminal/${sessionName}`;
    
    const queryString = `?web_client_id=${encodeURIComponent(webClientId)}`;
    const wsTerminalUrl = `${url}${queryString}`;
    
    wsTerminal = new WebSocket(wsTerminalUrl);
    
    wsTerminal.onopen = function () {
        markConnectionEstablished();
    };
    
    wsTerminal.onmessage = function (event) {
        if (ownWebClientId == "") {
            ownWebClientId = webClientId;
            const wsControlUrl = `${wsUrlPrefix}://${window.location.host}/ws/control`;
            wsControl = new WebSocket(wsControlUrl);
            startWsControl(wsControl, term, fitAddon, ownWebClientId);
        }

        let data = event.data;
        
        if (typeof data === 'string') {
            // Handle ANSI title change sequences
            const titleRegex = /\x1b\]0;([^\x07\x1b]*?)(?:\x07|\x1b\\)/g;
            let match;
            while ((match = titleRegex.exec(data)) !== null) {
                document.title = match[1];
            }
            
            if (data.includes('\x1b[0 q')) {
                const shouldBlink = term.options.cursorBlink;
                const cursorStyle = term.options.cursorStyle;
                let replacement;
                switch (cursorStyle) {
                    case 'block':
                        replacement = shouldBlink ? '\x1b[1 q' : '\x1b[2 q';
                        break;
                    case 'underline':
                        replacement = shouldBlink ? '\x1b[3 q' : '\x1b[4 q';
                        break;
                    case 'bar':
                        replacement = shouldBlink ? '\x1b[5 q' : '\x1b[6 q';
                        break;
                    default:
                        replacement = '\x1b[2 q';
                        break;
                }
                data = data.replace(/\x1b\[0 q/g, replacement);
            }
        }
        
        term.write(data);
    };
    
    wsTerminal.onclose = function () {
        handleReconnection();
    };
    
    // Update sendAnsiKey to use the actual WebSocket
    const originalSendAnsiKey = sendAnsiKey;
    sendAnsiKey = (ansiKey) => {
        if (ownWebClientId !== "") {
            wsTerminal.send(ansiKey);
        }
    };
    
    // Setup resize handler
    setupResizeHandler(term, fitAddon, () => wsControl, () => ownWebClientId);
    
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
        }
    };
}

/**
 * Start the control WebSocket and set up its handlers
 * @param {WebSocket} wsControl - Control WebSocket instance
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {string} ownWebClientId - Own web client ID
 */
function startWsControl(wsControl, term, fitAddon, ownWebClientId) {
    wsControl.onopen = function (event) {
        const fitDimensions = fitAddon.proposeDimensions();
        const { rows, cols } = fitDimensions;
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
                cursor_inactive_style
            } = msg;
            term.options.fontFamily = font;
            term.options.theme = theme;
            if (cursor_blink !== 'undefined') {
                term.options.cursorBlink = cursor_blink;
            }
            if (mac_option_is_meta !== 'undefined') {
                term.options.macOptionIsMeta = mac_option_is_meta;
            }
            if (cursor_style !== 'undefined') {
                term.options.cursorStyle = cursor_style;
            }
            if (cursor_inactive_style !== 'undefined') {
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
            if (rows === term.rows && cols === term.cols) {
                return;
            }
            term.resize(cols, rows);

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
        } else if (msg.type === "QueryTerminalSize") {
            const fitDimensions = fitAddon.proposeDimensions();
            const { rows, cols } = fitDimensions;
            if (rows !== term.rows || cols !== term.cols) {
                term.resize(cols, rows);
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
            window.location.pathname = `/${new_session_name}`;
        }
    };

    wsControl.onclose = function () {
        handleReconnection();
    };
}

/**
 * Set up window resize event handler
 * @param {Terminal} term - Terminal instance
 * @param {FitAddon} fitAddon - Terminal fit addon
 * @param {function} getWsControl - Function that returns control WebSocket
 * @param {function} getOwnWebClientId - Function that returns own web client ID
 */
export function setupResizeHandler(term, fitAddon, getWsControl, getOwnWebClientId) {
    addEventListener("resize", (event) => {
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
        if (wsControl) {
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
        }
    });
}
