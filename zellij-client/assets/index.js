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
      console.log(ev);
      let key_code = ev.key.charCodeAt(0);
      send_ansi_key(`\x1b[${key_code};${modifier_string}u`);
    };
    term.attachCustomKeyEventHandler((ev) => {
        // workarounds for https://github.com/xtermjs/xterm.js/blob/41e8ae395937011d6bf6c7cb618b851791aed395/src/common/input/Keyboard.ts#L158
        if (ev.type === "keydown") {
          console.log(ev);
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
          if ((modifiers_count > 1 || ev.metaKey) && ev.keyCode != shift_keycode && ev.keyCode != alt_keycode && ev.keyCode != ctrl_keycode) {
            encode_kitty_key(ev);
            return false;
          }
            // TODO: CONTINUE HERE - alt + doesn't seem to be sent... fix it
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

    let ws_terminal = new WebSocket("ws://127.0.0.1:8082/ws/terminal/default");
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
            ws_control = new WebSocket(
                "ws://127.0.0.1:8082/ws/control/default"
            );
            start_ws_control();
        }
        term.write(msg.bytes);
    };

    ws_terminal.onclose = function () {
        console.log("Disconnected from WebSocket terminal server");
    };
});
