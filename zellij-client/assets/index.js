function is_https() {
    return document.location.protocol === "https:";
}

document.addEventListener("DOMContentLoaded", async (event) => {

    let token;
    let remember;
    let has_authentication_cookie = window.is_authenticated;

    // Reconnection state
    let reconnectionAttempt = 0;
    let isReconnecting = false;
    let reconnectionTimeout = null;
    let hasConnectedBefore = false;
    let isPageUnloading = false;

    async function checkConnection() {
        try {
            let url_prefix = is_https() ? "https" : "http";
            const response = await fetch(`${url_prefix}://${window.location.host}/info/version`, {
                method: 'GET',
                timeout: 5000
            });
            return response.ok;
        } catch (error) {
            return false;
        }
    }

    function getReconnectionDelay(attempt) {
        const delays = [1, 2, 4, 8, 16];
        return delays[Math.min(attempt - 1, delays.length - 1)];
    }

    async function handleReconnection() {
        if (isReconnecting || !hasConnectedBefore || isPageUnloading) {
            return;
        }
        
        isReconnecting = true;
        let currentModal = null;
        
        while (isReconnecting) {
            reconnectionAttempt++;
            const delaySeconds = getReconnectionDelay(reconnectionAttempt);
            
            const result = await showReconnectionModal(reconnectionAttempt, delaySeconds);
            
            if (result.action === 'cancel') {
                if (result.cleanup) result.cleanup();
                isReconnecting = false;
                reconnectionAttempt = 0;
                return;
            }
            
            if (result.action === 'reconnect') {
                currentModal = result.modal;
                const connectionOk = await checkConnection();
                
                if (connectionOk) {
                    // Reset state and reload page
                    if (result.cleanup) result.cleanup();
                    isReconnecting = false;
                    reconnectionAttempt = 0;
                    window.location.reload();
                    return;
                } else {
                    // Clean up current modal before showing next one
                    if (result.cleanup) result.cleanup();
                    // Continue with next attempt (modal will show again in next loop iteration)
                    continue;
                }
            }
        }
    }

    // Detect page unload to prevent reconnection modal during refresh/navigation
    window.addEventListener('beforeunload', () => {
        isPageUnloading = true;
    });

    window.addEventListener('pagehide', () => {
        isPageUnloading = true;
    });

    async function wait_for_security_token() {
      token = null;
      remember = null;
      while (!token) {
        let result = await getSecurityToken();
        if (result) {
          token = result.token;
          remember = result.remember;
        } else {
          await showErrorModal("Error", "Must provide security token in order to log in.");
        }
      }
    }

    if (!has_authentication_cookie) {
      await wait_for_security_token();
    }
    let web_client_id;

    while (!web_client_id) {
      web_client_id = await get_client_id(token, remember, has_authentication_cookie);
      if (!web_client_id) {
        has_authentication_cookie = false;
        await wait_for_security_token()
      }
    }


    var own_web_client_id = ""; // TODO: what is this?
    const { term, fitAddon } = initTerminal();
    const session_name = location.pathname.split("/").pop();
    console.log("session_name", session_name);

    let send_ansi_key = (ansi_key) => {
        if (!own_web_client_id == "") {
            ws_terminal.send(ansi_key);
        }
    };
    let encode_kitty_key = (ev) => {
        let shift_value = 1;
        let alt_value = 2;
        let ctrl_value = 4;
        let super_value = 8;
        let modifier_string = 1;
        if (ev.shiftKey) {
            modifier_string += shift_value;
        }
        if (ev.altKey) {
            modifier_string += alt_value;
        }
        if (ev.ctrlKey) {
            modifier_string += ctrl_value;
        }
        if (ev.metaKey) {
            modifier_string += super_value;
        }
        let key_code = ev.key.charCodeAt(0);
        send_ansi_key(`\x1b[${key_code};${modifier_string}u`);
    };
    term.attachCustomKeyEventHandler((ev) => {
        if (ev.type === "keydown") {
            let modifiers_count = 0;
            let shift_keycode = 16;
            let alt_keycode = 17;
            let ctrl_keycode = 18;
            if (ev.altKey) {
                modifiers_count += 1;
            }
            if (ev.ctrlKey) {
                modifiers_count += 1;
            }
            if (ev.shiftKey) {
                modifiers_count += 1;
            }
            if (ev.metaKey) {
                modifiers_count += 1;
            }
            if (
                (modifiers_count > 1 || ev.metaKey) &&
                ev.keyCode != shift_keycode &&
                ev.keyCode != alt_keycode &&
                ev.keyCode != ctrl_keycode
            ) {
                ev.preventDefault();
                encode_kitty_key(ev);
                return false;
            }
            // workarounds for https://github.com/xtermjs/xterm.js/blob/41e8ae395937011d6bf6c7cb618b851791aed395/src/common/input/Keyboard.ts#L158
            if (ev.key == "ArrowLeft" && ev.altKey) {
                ev.preventDefault();
                send_ansi_key("\x1b[1;3D");
                return false;
            }
            if (ev.key == "ArrowRight" && ev.altKey) {
                ev.preventDefault();
                send_ansi_key("\x1b[1;3C");
                return false;
            }
            if (ev.key == "ArrowUp" && ev.altKey) {
                ev.preventDefault();
                send_ansi_key("\x1b[1;3A");
                return false;
            }
            if (ev.key == "ArrowDown" && ev.altKey) {
                ev.preventDefault();
                send_ansi_key("\x1b[1;3B");
                return false;
            }
            if (
                (ev.key == "=" && ev.altKey) ||
                (ev.key == "+" && ev.altKey) ||
                (ev.key == "-" && ev.altKey)
            ) {
                // these are not properly handled by xterm.js, so we bypass it and encode them as kitty to make things easier
                ev.preventDefault();
                encode_kitty_key(ev);
                return false;
            }
        }
        return true;
    });

    // TODO: test performance here
    let prev_col = 0;
    let prev_row = 0;
    let terminal_element = document.getElementById("terminal");
    terminal_element.addEventListener("mousemove", function (event) {
      window.term.focus();
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
                send_ansi_key(`\x1b[<35;${col + 1};${row + 1}M`);
            }
            prev_col = col;
            prev_row = row;
        }
    });

    document.addEventListener('contextmenu', function(event) {
        if (event.altKey) {
          // this is so that when the user does an alt-right-click to ungroup panes, the context menu will not appear
            event.preventDefault();
        }
    });

    term.onData((data) => {
        if (!own_web_client_id == "") {
            ws_terminal.send(data);
        }
    });
    term.onBinary((data) => {
        const buffer = new Uint8Array(data.length);
        for (let i = 0; i < data.length; ++i) {
            buffer[i] = data.charCodeAt(i) & 255;
        }
        ws_terminal.send(buffer);
    });

    let ws_url_prefix = is_https() ? "wss" : "ws";
    let url = session_name === ""
      ? `${ws_url_prefix}://${window.location.host}/ws/terminal`
      : `${ws_url_prefix}://${window.location.host}/ws/terminal/${session_name}`;

    let query_string = `?web_client_id=${encodeURIComponent(web_client_id)}`;
    const ws_terminal_url = `${url}${query_string}`;

    let ws_terminal = new WebSocket(ws_terminal_url);
    let ws_control;

    addEventListener("resize", (event) => {
        if (own_web_client_id === "") {
            console.debug("skipping resize event before init");
            return;
        }

        let fit_dimensions = fitAddon.proposeDimensions();
        if (fit_dimensions === undefined) {
            console.warn("failed to get new fit dimensions");
            return;
        }

        const { rows, cols } = fit_dimensions;
        if (rows === term.rows && cols === term.cols) {
            console.log("rows and cols unchanged, skipping resize");
            return;
        }
        console.log("resize term after resize event", rows, cols);

        term.resize(cols, rows);

        ws_control.send(
            JSON.stringify({
                web_client_id: own_web_client_id,
                payload: {
                    type: "TerminalResize",
                    rows,
                    cols,
                },
            })
        );
    });

    ws_terminal.onopen = function () {
        console.log("Connected to WebSocket terminal server");
        hasConnectedBefore = true;
    };
    function start_ws_control() {
        ws_control.onopen = function (event) {
            const fit_dimensions = fitAddon.proposeDimensions();
            const { rows, cols } = fit_dimensions;
            ws_control.send(
                JSON.stringify({
                    web_client_id: own_web_client_id,
                    payload: {
                        type: "TerminalResize",
                        rows,
                        cols,
                    },
                })
            );
        };
        ws_control.onmessage = function (event) {
            const msg = JSON.parse(event.data);
            if (msg.type === "SetConfig") {
                console.log("SetConfig message received", msg);
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
                body.style.background = theme.background;

                const terminal = document.getElementById("terminal");
                terminal.style.background = theme.background;

                const fit_dimensions = fitAddon.proposeDimensions();
                if (fit_dimensions === undefined) {
                    console.warn("failed to get new fit dimensions");
                    return;
                }

                const { rows, cols } = fit_dimensions;
                if (rows === term.rows && cols === term.cols) {
                    console.log("rows and cols unchanged, skipping resize");
                    return;
                }
                console.log("resize term after font change", rows, cols);
                term.resize(cols, rows);

                ws_control.send(
                    JSON.stringify({
                        web_client_id: own_web_client_id,
                        payload: {
                            type: "TerminalResize",
                            rows,
                            cols,
                        },
                    })
                );
            } else if (msg.type === "QueryTerminalSize") {
                const fit_dimensions = fitAddon.proposeDimensions();
                const { rows, cols } = fit_dimensions;
                if (rows !== term.rows || cols !== term.cols) {
                    term.resize(cols, rows);
                }
                // we do this anyway even if our size didn't change
                // because this means the server needs to know our
                // size (eg. if we switched sessions without refreshing
                // and our client state on the server was lost)
                ws_control.send(
                    JSON.stringify({
                        web_client_id: own_web_client_id,
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

        ws_control.onclose = function () {
            console.log("Disconnected from WebSocket control server");
            handleReconnection();
        };
    }

    ws_terminal.onmessage = function (event) {
        if (own_web_client_id == "") {
            own_web_client_id = web_client_id;
            const ws_control_url = `${ws_url_prefix}://${window.location.host}/ws/control`;
            ws_control = new WebSocket(ws_control_url);
            start_ws_control();
        }

        // workaround for: https://github.com/xtermjs/xterm.js/issues/3293
        // since in this version of xterm.js the default cursor is hard-coded to blinking
        // we need to replace it with the appropriate cursor shape / blinking based on the
        // user's config
        let data = event.data;
        if (typeof data === 'string' && data.includes('\x1b[0 q')) {
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
                    // Fallback to steady block if unknown style
                    replacement = '\x1b[2 q';
                    break;
            }
            // Replace \033[0 q with the appropriate sequence
            data = data.replace(/\x1b\[0 q/g, replacement);
        }
        term.write(data);
    };

    ws_terminal.onclose = function () {
        console.log("Disconnected from WebSocket terminal server");
        handleReconnection();
    };
});

function initTerminal() {
    const term = new Terminal({
        fontFamily: "Monospace",
        allowProposedApi: true,
        scrollback: 0,
    });
    // for debugging
    window.term = term;
    const fitAddon = new FitAddon.FitAddon();
    const clipboardAddon = new ClipboardAddon.ClipboardAddon();

    const { linkHandler, activateLink } = build_link_handler();
    const webLinksAddon = new WebLinksAddon.WebLinksAddon(activateLink, linkHandler);
    term.options.linkHandler = linkHandler;

    const webglAddon = new WebglAddon.WebglAddon();
    term.loadAddon(fitAddon);
    term.loadAddon(clipboardAddon);
    term.loadAddon(webLinksAddon);
    webglAddon.onContextLoss((e) => {
        // TODO: reload, or?
        webglAddon.dispose();
    });
    term.loadAddon(webglAddon);
    term.open(document.getElementById("terminal"));
    fitAddon.fit();
    console.log(`Initialized terminal, rows: ${term.rows}, cols: ${term.cols}`);
    return { term, fitAddon };
}

async function get_client_id(token, rememberMe, has_authentication_cookie) {
    let url_prefix = is_https() ? "https" : "http";
    if (!has_authentication_cookie) {
       let login_res = await fetch(`${url_prefix}://${window.location.host}/command/login`, {
         method: 'POST',
         headers: {
           'Content-Type': 'application/json',
         },
         body: JSON.stringify({
           auth_token: token,
           remember_me: rememberMe ? true : false
         }),
         credentials: 'include'
      });

      if (login_res.status === 401) {
        await showErrorModal("Error", "Unauthorized or revoked login token.");
        return;
      } else if (!login_res.ok) {
        await showErrorModal("Error", `Error ${login_res.status} connecting to server.`);
        return;
      }
    }
    let data = await fetch(`${url_prefix}://${window.location.host}/session`, {
        method: "POST",
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({}),
    });
    if (data.status === 401) {
      await showErrorModal("Error", "Unauthorized or revoked login token.");
    } else if (!data.ok) {
      await showErrorModal("Error", `Error ${data.status} connecting to server.`);
    } else {
      let body = await data.json();
      return body.web_client_id;
    }
}

function build_link_handler() {
    let _linkPopup;
    function removeLinkPopup (event, text, range) {
      if (_linkPopup) {
         _linkPopup.remove();
         _linkPopup = undefined;
      }
    }

    function showLinkPopup(event, text, range) {
      let popup = document.createElement('div');
      popup.classList.add('xterm-link-popup');
      popup.style.position = 'absolute';
      popup.style.top = (event.clientY + 25) + 'px';
      popup.style.left = (event.clientX + 25) + 'px';
      popup.style.fontSize = 'small';
      popup.style.lineBreak = 'normal';
      popup.style.padding = '4px';
      popup.style.minWidth = '15em';
      popup.style.maxWidth = '80%';
      popup.style.border = 'thin solid';
      popup.style.borderRadius = '6px';
      popup.style.background = '#6c4c4c';
      popup.style.borderColor = '#150262';
      popup.innerText = "Shift-Click: " + text;
      const topElement = event.target.parentNode;
      topElement.appendChild(popup);
      const popupHeight = popup.offsetHeight;
      _linkPopup = popup;
    }
    function activateLink(event, uri) {
      const newWindow = window.open(uri, '_blank');
      if (newWindow) newWindow.opener = null; // prevent the opened link from gaining access to the terminal instance
    }

    let linkHandler = {};
    linkHandler.hover = showLinkPopup;
    linkHandler.leave = removeLinkPopup;
    linkHandler.activate = activateLink;
    return { linkHandler, activateLink };
}
