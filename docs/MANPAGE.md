NAME
====

**zellij** - run zellij

DESCRIPTION
===========

Zellij is a workspace aimed at developers, ops-oriented people and anyone who
loves the terminal. At its core, it is a terminal multiplexer (similar to tmux
and screen), but this is merely its infrastructure layer.

Zellij includes a layout system, and a plugin system allowing one to create
plugins in any language that compiles to WebAssembly.

To list currently running sessions run: `zellij list-sessions`
To attach to a currently running session run: `zellij attach [session-name]`

OPTIONS
=======

    -c, --config <CONFIG>
            Change where zellij looks for the configuration file [env: ZELLIJ_CONFIG_FILE=]

    --config-dir <CONFIG_DIR>
            Change where zellij looks for the configuration directory [env: ZELLIJ_CONFIG_DIR=]

    -d, --debug
            Specify emitting additional debug information

    --data-dir <DATA_DIR>
            Change where zellij looks for plugins

    -h, --help
            Print help information

    -l, --layout <LAYOUT>
            Name of a predefined layout inside the layout directory or the path to a layout file if
            inside a session (or using the --session flag) will be added to the session as a new tab
            or tabs, otherwise will start a new session

    --max-panes <MAX_PANES>
            Maximum panes on screen, caution: opening more panes will close old ones

    -n, --new-session-with-layout <NEW_SESSION_WITH_LAYOUT>
            Name of a predefined layout inside the layout directory or the path to a layout file
            Will always start a new session, even if inside an existing session

    -s, --session <SESSION>
            Specify name of a new session

    -S, --session-name-generator <SESSION_NAME_GENERATOR>
            Specify the session name generator (e.g. "numbered")

    -V, --version
            Print version information

CONFIGURATION
=============

Zellij looks for configuration file in the following order:

1. the file provided with _--config_
2. under the path provided in _ZELLIJ_CONFIG_FILE_ environment variable
3. the default location (see FILES section)
4. the system location

Run `zellij setup --check` in order to see possible issues with the
configuration.

LAYOUTS
=======

Layouts are yaml files which Zellij can load on startup when _--layout_ flag is
provided.
By default Zellij will load a layout called `default.yaml`,
but this can be changed by using the `default_layout: [LAYOUT_NAME]` configuration option.

For example a file like this:

```
---
direction: Vertical
parts:
    - direction: Horizontal
      split_size:
        Percent: 50
      parts:
        - direction: Vertical
          split_size:
            Percent: 50
        - direction: Vertical
          split_size:
            Percent: 50
    - direction: Horizontal
      split_size:
        Percent: 50
```

will tell Zellij to create this layout:

```
┌─────┬─────┐
│     │     │
├─────┤     │
│     │     │
└─────┴─────┘
```

CREATING LAYOUTS
----------------

A layout file is a nested tree structure. Each node describes either a pane
(leaf), or a space in which its parts (children) will be created.

Each node has following fields:

* **direction: <Horizontal / Vertical\>** - node's children will be created by a
  split in given direction.
* **split_size:** - this indicates either a percentage of the node's parent's
  space or a fixed size of columns/rows from its parent's space.
  * **Percent: <1-100\>**
  * **Fixed: <lines_number/columns_number\>**
* **plugin: /path/to/plugin.wasm** - optional path to a compiled Zellij plugin.
  If indicated loads a plugin into the created space. For more information see
  PLUGINS section.

KEYBINDINGS
===========

Zellij comes with a default set of keybindings which aims to fit as many users
as possible but that behaviour can be overridden or modified in user
configuration files. The information about bindings is available in the
_keybinds_ section of configuration. For example, to introduce a keybinding that
will create a new tab and go to tab 1 after pressing 'c' one can write:

```
keybinds:
    normal:
        - action: [ NewTab, GoToTab: 1,]
          key: [ Char: 'c',]
```

where "normal" stands for a mode name (see MODES section), "action" part
specifies the actions to be executed by Zellij (see ACTIONS section) and "key"
is used to list  keys or key combinations bound to given actions (see KEYS).

The default keybinds can be unbound either for a specific mode, or for every mode.
It supports either a list of `keybinds`, or a bool indicating that every keybind
should be unbound:

```
keybinds:
    unbind: true
```

Will unbind every default binding.

```
keybinds:
    unbind: [ Ctrl: 'p']
```

Will unbind every default `^P` binding for each mode.

```
keybinds:
    normal:
        - unbind: true
```

Will unbind every default keybind for the `normal` mode.

```
keybinds:
    normal:
        - unbind: [ Alt: 'n', Ctrl: 'g']
```

Will unbind every default keybind for `n` and `^g` for the `normal` mode.

ACTIONS
-------

* **Quit** - quits Zellij
* **SwitchToMode: <InputMode\>** - switches to the specified input mode. See
  MODES section for possible values.
* **Resize: <Direction\>** - resizes focused pane in the specified direction
  (one of: Left, Right, Up, Down).
* **FocusNextPane** - switches focus to the next pane to the right or below if
  on  screen edge.
* **FocusPreviousPane** - switches focus to the next pane to the left or above
  if on  screen edge.
* **SwitchFocus** - left for legacy support. Switches focus to a pane with the
  next ID.
* **MoveFocus: <Direction\>** -  moves focus in the specified direction (Left,
  Right, Up, Down).
* **Clear** - clears current screen.
* **DumpScreen: <File\>** - dumps the screen in the specified file.
* **DumpLayout: <File\>** - dumps the screen in the specified or default file.
* **EditScrollback** - replaces the current pane with the scrollback buffer.
* **ScrollUp** - scrolls up 1 line in the focused pane.
* **ScrollDown** - scrolls down 1 line in the focused pane.
* **PageScrollUp** - scrolls up 1 page in the focused pane.
* **PageScrollDown** - scrolls down 1 page in the focused pane.
* **ToggleFocusFullscreen** - toggles between fullscreen focus pane and normal
  layout.
* **NewPane: <Direction\>** - opens a new pane in the specified direction (Left,
  Right, Up, Down) relative to focus.
* **CloseFocus** - closes focused pane.
* **NewTab** - creates a new tab.
* **GoToNextTab** - goes to the next tab.
* **GoToPreviousTab** - goes to previous tab.
* **CloseTab** - closes current tab.
* **GoToTab: <Index\>** - goes to the tab with the specified index number.
* **Detach** - detach session and exit.
* **ToggleActiveSyncTab** - toggle between sending text commands to all panes
  on the current tab and normal mode.
* **UndoRenameTab** - undoes the changed tab name and reverts to the previous name.
* **UndoRenamePane** - undoes the changed pane name and reverts to the previous name.

KEYS
----

* **Char: <character\>** - a single character with no modifier.
* **Alt: <character\>** - a single character with `Alt` key as modifier.
* **Ctrl: <character\>** - a single character with `Ctrl` key as modifier.
* **F: <1-12\>** - one of `F` keys (usually at the top of the keyboard).
* **Backspace**
* **Left / Right / Up / Down** - arrow keys on the keyboard.
* **Home**
* **End**
* **PageUp / PageDown**
* **BackTab** - a backward Tab key.
* **Delete**
* **Insert**
* **Esc**

MODES
-----

* **normal** - the default startup mode of Zellij. Provides the ability to
  switch to different modes, as well as some quick navigation shortcuts.
* **locked** - disables all keybindings except the one that would switch the
  mode to normal (_ctrl-g_ by default). Useful when Zellij's keybindings
  conflict with those of a chosen terminal app.
* **tmux** - provides convenience keybindings emulating simple tmux behaviour
* **pane** - includes instructions that manipulate the panes (adding new panes,
  moving, closing).
* **tab** - includes instructions that manipulate the tabs (adding new tabs,
  moving, closing).
* **resize** - allows resizing of the focused pane.
* **scroll** - allows scrolling within the focused pane.
* **renametab** - is a "hidden" mode that can be passed to _SwitchToMode_
  action. It will trigger renaming of a tab.
* **renamepane** - is a "hidden" mode that can be passed to _SwitchToMode_
  action. It will trigger renaming of a pane.
* **session** - allows detaching from a session.

Theme
=====

A color theme can be defined either in truecolor, 256 or hex color format.
Truecolor:

```
fg: [0, 0, 0]
```

256:

```
fg: 0
```

Hex color:

```
fg: "#000000"
bg: "#000"
```

The color theme can be specified in the following way:

```
themes:
  default:
    fg: [0,0,0]
    bg: [0,0,0]
    black: [0,0,0]
    red: [0,0,0]
    green: [0,0,0]
    yellow: [0,0,0]
    blue: [0,0,0]
    magenta: [0,0,0]
    cyan: [0,0,0]
    white: [0,0,0]
    orange: [0,0,0]
```

If the theme is called `default`, then zellij will pick it on startup.
To specify a different theme, run zellij with:

```
zellij options --theme [NAME]
```

or put the name in the configuration file with `theme: [NAME]`.

PLUGINS
=======

Zellij has a plugin system based on WebAssembly. Any language that can run on
WASI can be used to develop a plugin. To load a plugin include it in a layout
file. Zellij comes with default plugins included: _status-bar_, _strider_,
_tab-bar_.

FILES
=====

Default user configuration directory location:

* Linux: _$XDG_HOME/zellij /home/alice/.config/zellij_
* macOS: _/Users/Alice/Library/Application Support/com.Zellij-Contributors.zellij_

Default user layout directory location:

* Subdirectory called `layouts` inside of the configuration directory.
* Linux: _$XDG_HOME/zellij/layouts /home/alice/.config/zellij/layouts
* macOS: _/Users/Alice/Library/Application/layouts Support/com.Zellij-Contributors.zellij/layouts_

Default plugin directory location:

* Linux: _$XDG_DATA_HOME/zellij/plugins /home/alice/.local/share/plugins

ENVIRONMENT
===========

ZELLIJ_CONFIG_FILE
  Path of Zellij config to load.
ZELLIJ_CONFIG_DIR
  Path of the Zellij config directory.

NOTES
=====

The manpage is meant to provide concise offline reference. For more detailed
instructions please visit:

<https://zellij.dev/documentation>
