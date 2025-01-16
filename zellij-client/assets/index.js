document.addEventListener("DOMContentLoaded", (event) => {
    var term = new Terminal({ fontFamily: "MonaspaceNeon" });
    var fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    var own_web_client_id = "";
    // term.resize(234, 43);
    term.open(document.getElementById("terminal"));

    let send_ansi_key = (ansi_key) => {
        if (!own_web_client_id == "") {
            ws_terminal.send(
                JSON.stringify({
                    web_client_id: own_web_client_id,
                    stdin: ansi_key,
                })
            );
        }
    };
    term.attachCustomKeyEventHandler((ev) => {
        // workarounds for https://github.com/xtermjs/xterm.js/blob/41e8ae395937011d6bf6c7cb618b851791aed395/src/common/input/Keyboard.ts#L158
        if (ev.type === "keydown") {
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
        }
        return true;
    });

    term.onData((data) => {
        if (!own_web_client_id == "") {
            ws_terminal.send(
                JSON.stringify({
                    web_client_id: own_web_client_id,
                    stdin: data,
                })
            );
        }
    });
    term.onBinary((data) => {
        const buffer = new Uint8Array(data.length);
        for (let i = 0; i < data.length; ++i) {
            buffer[i] = data.charCodeAt(i) & 255;
        }
        ws_terminal.send(
            JSON.stringify({
                web_client_id: own_web_client_id,
                stdin: buffer,
            })
        );
    });

    let ws_terminal = new WebSocket("ws://127.0.0.1:8080");
    // let ws_control = new WebSocket('ws://127.0.0.1:8081');
    let ws_control;

    addEventListener("resize", (event) => {
        if (!own_web_client_id == "") {
            let fit_dimensions = fitAddon.proposeDimensions();
            if (fit_dimensions) {
                let rows = fit_dimensions.rows;
                let cols = fit_dimensions.cols;
                term.resize(cols, rows);

                ws_control.send(
                    JSON.stringify({
                        web_client_id: own_web_client_id,
                        message: {
                            TerminalResize: {
                                rows: rows,
                                cols: cols,
                            },
                        },
                    })
                );
            }
        }
    });

    ws_terminal.onopen = function () {
        console.log("Connected to WebSocket terminal server");
    };
    function start_ws_control() {
        ws_control.onopen = function () {
            console.log("Connected to WebSocket control server");
            if (!own_web_client_id == "") {
                let fit_dimensions = fitAddon.proposeDimensions();
                if (fit_dimensions) {
                    let rows = fit_dimensions.rows;
                    let cols = fit_dimensions.cols;
                    term.resize(cols, rows);

                    ws_control.send(
                        JSON.stringify({
                            web_client_id: own_web_client_id,
                            message: {
                                TerminalResize: {
                                    rows: rows,
                                    cols: cols,
                                },
                            },
                        })
                    );
                }
            }
        };
        ws_control.onclose = function () {
            console.log("Disconnected from WebSocket control server");
        };
    }

    ws_terminal.onmessage = function (event) {
        let msg = JSON.parse(event.data);
        if (own_web_client_id == "") {
            own_web_client_id = msg.web_client_id;
            ws_control = new WebSocket("ws://127.0.0.1:8081");
            start_ws_control();
        }
        term.write(msg.bytes);
    };

    ws_terminal.onclose = function () {
        console.log("Disconnected from WebSocket terminal server");
    };
});
