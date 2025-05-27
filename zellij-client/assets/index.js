function is_https() {
    return document.location.protocol === "https:";
}

document.addEventListener("DOMContentLoaded", async (event) => {
    const web_client_id = await get_client_id();

    var own_web_client_id = "";
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
    const ws_terminal_url =
        session_name === ""
            ? `${ws_url_prefix}://${window.web_server_ip}:${window.web_server_port}/ws/terminal?web_client_id=${web_client_id}`
            : `${ws_url_prefix}://${window.web_server_ip}:${window.web_server_port}/ws/terminal/${session_name}?web_client_id=${web_client_id}`;

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
                const { font, background } = msg;
                term.options.fontFamily = font;
                term.options.theme = { ...term.options.theme, background };

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
        };
    }

    ws_terminal.onmessage = function (event) {
        //         console.log(
        //             "Received message from WebSocket terminal server",
        //             event.data
        //         );
        if (own_web_client_id == "") {
            own_web_client_id = web_client_id;
            const ws_control_url = `${ws_url_prefix}://${window.web_server_ip}:${window.web_server_port}/ws/control`;

            ws_control = new WebSocket(ws_control_url);
            start_ws_control();
        }
        term.write(event.data);
    };

    ws_terminal.onclose = function () {
        console.log("Disconnected from WebSocket terminal server");
    };
});

function initTerminal() {
    const term = new Terminal({
        fontFamily: "Monospace",
        allowProposedApi: true,
    });
    // window.term = term;
    const fitAddon = new FitAddon.FitAddon();
    const clipboardAddon = new ClipboardAddon.ClipboardAddon();
    const webLinksAddon = new WebLinksAddon.WebLinksAddon();
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

function get_client_id() {
    let url_prefix = is_https() ? "https" : "http";
    return fetch(
        `${url_prefix}://${window.web_server_ip}:${window.web_server_port}/session`,
        {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify({}),
        }
    )
        .then((data) => data.json())
        .then((data) => data.web_client_id);
}
