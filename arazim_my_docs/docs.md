# basic uses

### technical commands
- to compile the code, use `cargo xtask build -r --no-web`
- the code compiles at `target/release/zellij`



### time data
- running a fresh build from zero takes about ~$35m$.
- running it after the first time, takes about ~$13m$.
as a result, think a lot before you try doing some changes, as it costs a lot :(



# important files
While waiting for the code to compile, I find for myself important files to read for better understanding of the code.
If each compile takes me so much time, I at least get more knowledge about the code in that time.
It also makes me to be really carefull with each test because of its cost, so it might even be better that way.
- `default-plugins/configuration/src/presets.rs`
    - this has a list of all commands in the taskbar.
- `src/tests/fixtures/configs/load_background_plugins.kdl`
    - also a file consisting of ommands so I add the command there too.
- `docs/ARCHITECTURE.md`
    - short markdown file about important files in the codebase
    - probably helpful
    - found this file `zellij-server/src/screen.rs`, which is the one that controls creating new panes.
- `docs/MANPAGE`
    - also some explanation about the codebase.

- `default-plugins/status-bar/src/one_line_ui.rs`
    - at start it gets all the bindings, and probably set them in the status bar, want to add the '+' there.
- `zellij-utils/src/data.rs`
    - it defines the `ModeInfo` struct, which has the field `keybinds`
- `default-plugins/compact-bar/src/keybind_utils.rs`
    - the definition of `InputMode::Pane`
- `zellij-utils/src/input/mod.rs`
    - it defines the `get_mode_info` function which returns a `ModeInfo` struct with an instance of keybinds.
- `zellij-server/src/lib.rs`
    - this is where we load the config.
    - I will just add manually there a keybinding for the new command, and it will hopefully works.
    - we should also note the `zellij-utils/src/input/config.rs` that defines the `Config` struct.
- `zellij-utils/assets/config/default.kdl`
    - default config file when loading for the first time.


It is possible to just change the config file located at `~/.config/zellij` and add the binding. I guess it is not the intended solution as it is not changing the code. So I will try to find in the code the relevant thing.