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

Run `zellij --help` to see available flags and subcommamds.

CONFIGURATION
=============

Zellij looks for configuration file in the following order:

1. the file provided with _--config_
2. under the path provided in *ZELLIJ_CONFIG_FILE* environment variable
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
* __direction: <Horizontal / Vertical\>__ - node's children will be created by a
  split in given direction.
* **split_size:** - this indicates either a percentage of the node's parent's
  space or a fixed size of columns/rows from its parent's space.
    * __Percent: <1-100\>__
    * __Fixed: <lines_number/columns_number\>__
* __plugin: /path/to/plugin.wasm__ - optional path to a compiled Zellij plugin.
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

* __Quit__ - quits Zellij
* __SwitchToMode: <InputMode\>__ - switches to the specified input mode. See
  MODES section for possible values.
* __Resize: <Direction\>__ - resizes focused pane in the specified direction
  (one of: Left, Right, Up, Down).
* __FocusNextPane__ - switches focus to the next pane to the right or below if
  on  screen edge.
* __FocusPreviousPane__ - switches focus to the next pane to the left or above
  if on  screen edge.
* __SwitchFocus__ - left for legacy support. Switches focus to a pane with the
  next ID.
* __MoveFocus: <Direction\>__ -  moves focus in the specified direction (Left,
  Right, Up, Down).
* __Clear__ - clears current screen.
* __DumpScreen: <File\>__ - dumps the screen in the specified file.
* __DumpLayout: <File\>__ - dumps the screen in the specified or default file.
* __EditScrollback__ - replaces the current pane with the scrollback buffer.
* __ScrollUp__ - scrolls up 1 line in the focused pane.
* __ScrollDown__ - scrolls down 1 line in the focused pane.
* __PageScrollUp__ - scrolls up 1 page in the focused pane.
* __PageScrollDown__ - scrolls down 1 page in the focused pane.
* __ToggleFocusFullscreen__ - toggles between fullscreen focus pane and normal
  layout.
* __NewPane: <Direction\>__ - opens a new pane in the specified direction (Left,
  Right, Up, Down) relative to focus. 
* __CloseFocus__ - closes focused pane.
* __NewTab__ - creates a new tab.
* __GoToNextTab__ - goes to the next tab.
* __GoToPreviousTab__ - goes to previous tab.
* __CloseTab__ - closes current tab.
* __GoToTab: <Index\>__ - goes to the tab with the specified index number.
* __Detach__ - detach session and exit.
* __ToggleActiveSyncTab__ - toggle between sending text commands to all panes
  on the current tab and normal mode.
* __UndoRenameTab__ - undoes the changed tab name and reverts to the previous name.
* __UndoRenamePane__ - undoes the changed pane name and reverts to the previous name.


KEYS
----

* __Char: <character\>__ - a single character with no modifier.
* __Alt: <character\>__ - a single character with `Alt` key as modifier.
* __Ctrl: <character\>__ - a single character with `Ctrl` key as modifier.
* __F: <1-12\>__ - one of `F` keys (usually at the top of the keyboard).
* __Backspace__
* __Left / Right / Up / Down__ - arrow keys on the keyboard.
* __Home__
* __End__
* __PageUp / PageDown__
* __BackTab__ - a backward Tab key.
* __Delete__
* __Insert__
* __Esc__


MODES
-----

* __normal__ - the default startup mode of Zellij. Provides the ability to
  switch to different modes, as well as some quick navigation shortcuts.
* __locked__ - disables all keybindings except the one that would switch the
  mode to normal (_ctrl-g_ by default). Useful when Zellij's keybindings
  conflict with those of a chosen terminal app. 
* __tmux__ - provides convenience keybindings emulating simple tmux behaviour
* __pane__ - includes instructions that manipulate the panes (adding new panes,
  moving, closing).
* __tab__ - includes instructions that manipulate the tabs (adding new tabs,
  moving, closing).
* __resize__ - allows resizing of the focused pane.
* __scroll__ - allows scrolling within the focused pane.
* __renametab__ - is a "hidden" mode that can be passed to _SwitchToMode_
  action. It will trigger renaming of a tab.
* __renamepane__ - is a "hidden" mode that can be passed to _SwitchToMode_
  action. It will trigger renaming of a pane.
* __session__ - allows detaching from a session.


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

https://zellij.dev/documentation
