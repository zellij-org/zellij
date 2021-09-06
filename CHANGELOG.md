# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]
* Fix bug when opening new tab the new pane's viewport would sometimes be calculated incorrectly (https://github.com/zellij-org/zellij/pull/683)
* Fix bug when in some cases closing a tab would not clear the previous pane's contents (https://github.com/zellij-org/zellij/pull/684)
* Fix bug where tabs would sometimes be created with the wrong index in their name (https://github.com/zellij-org/zellij/pull/686)
* Fix bug where wide chars would mess up pane titles (https://github.com/zellij-org/zellij/pull/698)
* Fix various borderless-frame in viewport bugs (https://github.com/zellij-org/zellij/pull/697)
* Fix example configuration file (https://github.com/zellij-org/zellij/pull/693)

## [0.16.0] - 2021-08-31
* Plugins don't crash zellij anymore on receiving mouse events (https://github.com/zellij-org/zellij/pull/620)
* A universal logging system has been implemented (https://github.com/zellij-org/zellij/pull/592)
  * Added [`log`](https://docs.rs/log/0.4.14/log/#macros) crate support for logging within Zellij
  * Messages sent over the `stderr` of plugins are now logged as well, bringing back `dbg!` support!
* Add displaying of the `session-name` to the `tab-bar` (https://github.com/zellij-org/zellij/pull/608)
* Add command to dump `layouts` to stdout (https://github.com/zellij-org/zellij/pull/623)
  * `zellij setup --dump-layout [LAYOUT]` [default, strider, disable-status]
* Add `action`: `ScrollToBottom` (https://github.com/zellij-org/zellij/pull/626)
  * Bound by default to `^c` in `scroll` mode, scrolls to bottom and exists the scroll mode
* Simplify deserialization slightly (https://github.com/zellij-org/zellij/pull/633)
* Fix update plugin attributes on inactive tab (https://github.com/zellij-org/zellij/pull/634)
* New pane UI: draw pane frames - can be disabled with ctrl-p + z, or through configuration (https://github.com/zellij-org/zellij/pull/643)
* Terminal compatibility: support changing index colors through OSC 4 and similar (https://github.com/zellij-org/zellij/pull/646)
* Fix various shells (eg. nushell) unexpectedly exiting when the user presses ctrl-c (https://github.com/zellij-org/zellij/pull/648)
* Fix line wrapping while scrolling (https://github.com/zellij-org/zellij/pull/650)
* Indicate to the user when text is copied to the clipboard with the mouse (https://github.com/zellij-org/zellij/pull/642)
* Terminal compatibility: properly paste multilines (https://github.com/zellij-org/zellij/pull/653 + https://github.com/zellij-org/zellij/pull/658)
* Terminal compatibility: fix progress bar line overflow (http://github.com/zellij-org/zellij/pull/656)
* Add action to toggle between tabs `ToggleTab`, bound by default to [TAB] in tab mode (https://github.com/zellij-org/zellij/pull/622)
* Terminal compatibility: properly handle cursor shape changes in eg. Neovim (https://github.com/zellij-org/zellij/pull/659)
* The resize and layout systems have been overhauled (https://github.com/zellij-org/zellij/pull/568)
  * Resizing a terminal then returning it to its original size will now always return panes to their original sizes and positions
  * Resize mode resizes panes by 5% of the space on screen, not some fixed number
  * Panes on-screen keep their ratios – a screen split 50/50 between two panes will remain 50/50 even as the terminal is resized (https://github.com/zellij-org/zellij/issues/406)
  * The terminal can now be resized without leaving fullscreen mode
  * Layout parts are split into equal percentages if no explicit split-size is given (https://github.com/zellij-org/zellij/issues/619)
  * Fixed display of the tab bar at small terminal widths
* Add `tabs` to `layouts` (https://github.com/zellij-org/zellij/pull/625)

  The layout has now a template, and tabs section.
  The template specifies the location a tab is inserted in with `body: true`.

  Eg:
  ```
  ---
  template:
    direction: Horizontal
    parts:
      - direction: Vertical
        borderless: true
        split_size:
          Fixed: 1
        run:
          plugin: tab-bar
      - direction: Vertical # <= The location of
        body: true          # <= the inserted tab.
      - direction: Vertical
        borderless: true
        split_size:
          Fixed: 2
        run:
          plugin: status-bar
  tabs:
    - direction: Vertical # <= Multiple tabs can be
    - direction: Vertical # <= specified in the layout.
    - direction: Vertical
  ```

  The `NewTab` action can optionally be bound to open
  a layout that is assumed to be in the new `tabs` section

  This is a BREAKING CHANGE for people that have the
  `NewTab` action already bound in the config file:
  ```
  - action: [NewTab, ]
    key: [F: 5,]
  ```
  must now be specified as:
  ```
  - action: [NewTab: ,]
    key: [F: 5,]
  ```

  Optionally a layout that should be opened on the new tab can be
  specified:
  ```
  - action: [NewTab: {
    direction: Vertical,
    parts: [ {direction: Horizontal, split_size: {Percent: 50}},
    {direction: Horizontal, run: {command: {cmd: "htop"}}},],
    key: [F: 6,]
  ```


## [0.15.0] - 2021-07-19
* Kill children properly (https://github.com/zellij-org/zellij/pull/601)
* Change name of `Run` binding for actions (https://github.com/zellij-org/zellij/pull/602)
* Add running commands to `layouts` (https://github.com/zellij-org/zellij/pull/600)

  POSSIBLE BREAKING CHANGE for custom layouts:
  Plugins are under the run category now, that means:
  ```
  plugin: status-bar
  ```
  is now:
  ```
  run:
      plugin: status-bar
  ```
* Add `on_force_close` config option (https://github.com/zellij-org/zellij/pull/609)


## [0.14.0] - 2021-07-05
* Add improved error handling for layouts (https://github.com/zellij-org/zellij/pull/576)
* Change layout directory from data to config (https://github.com/zellij-org/zellij/pull/577)
  POSSIBLE BREAKING CHANGE:
  In case of having custom layouts in the previous
  `layout-dir` one can switch either the layouts to
  the new dir, or set the `layout-dir` to be the current
  `layout-dir`
* Fix `Makefile.toml` because of missing directory (https://github.com/zellij-org/zellij/pull/580)
* Autodetach on force close (https://github.com/zellij-org/zellij/pull/581)
* Add option to specify a default shell (https://github.com/zellij-org/zellij/pull/594)
* Add action to run bound commands in a pane (https://github.com/zellij-org/zellij/pull/596)
* Initial mouse support (https://github.com/zellij-org/zellij/pull/448)
* Add `layout-dir` to `setup --check` subcommand (https://github.com/zellij-org/zellij/pull/599)

## [0.13.0] - 2021-06-04
* Fix crash when padding before widechar (https://github.com/zellij-org/zellij/pull/540)
* Do not lag when reading input too fast (https://github.com/zellij-org/zellij/pull/536)
* Session name optional in attach command (https://github.com/zellij-org/zellij/pull/542)
* Fix build on platforms with TIOCGWINSZ / ioctl() integer type mismatch (https://github.com/zellij-org/zellij/pull/547)
* Fix(ui): session mode should be disabled in locked mode (https://github.com/zellij-org/zellij/pull/548)
* Add option to start in arbitrary modes (https://github.com/zellij-org/zellij/pull/513)
* Attaching to a session respects the `default_mode` setting of the client (https://github.com/zellij-org/zellij/pull/549)
* Add option to specify a color theme in the config (https://github.com/zellij-org/zellij/pull/550)
* Fix config options to not depend on `simplified_ui` (https://github.com/zellij-org/zellij/pull/556)
* Don't rename `unnamed` tabs upon deletion of other tabs (https://github.com/zellij-org/zellij/pull/554)
* Add layout to disable the status bar (https://github.com/zellij-org/zellij/pull/555)
* Significantly improve terminal pane performance (https://github.com/zellij-org/zellij/pull/567)

## [0.12.1] - 2021-05-28
* HOTFIX: fix Zellij not responding to input on certain terminals (https://github.com/zellij-org/zellij/issues/538)

## [0.12.0] - 2021-05-27
* Remove unused imports (https://github.com/zellij-org/zellij/pull/504)
* More Infrastructure changes for the upcoming session detach feature: run server and client in separate processes (https://github.com/zellij-org/zellij/pull/499)
* Restructuring cargo workspace: Separate client, server and utils into separate crates (https://github.com/zellij-org/zellij/pull/515)
* Terminal compatibility: handle most OSC sequences (https://github.com/zellij-org/zellij/pull/517)
* Split `layout` flag into `layout` and `layout-path` (https://github.com/zellij-org/zellij/pull/514)
* Fix behaviour of the `clean` flag (https://github.com/zellij-org/zellij/pull/519)
* Make distinction clearer between certain configuration flags (https://github.com/zellij-org/zellij/pull/529)
* Resource usage and performance improvements (https://github.com/zellij-org/zellij/pull/523)
* Feature: Detachable/Persistent sessions (https://github.com/zellij-org/zellij/pull/531)
* Terminal compatibility: Support wide characters (https://github.com/zellij-org/zellij/pull/535)

## [0.11.0] - 2021-05-15

This version is mostly an installation hotfix.

* Add `check` flag to `setup` subcommand, move `generate-completions` subcommand to `setup` flag (https://github.com/zellij-org/zellij/pull/503)
* Change the asset installation from an opt-in to an opt-out (https://github.com/zellij-org/zellij/pull/512)

## [0.10.0] - 2021-05-14
* Change Switch default config loading order of `HOME` and system (https://github.com/zellij-org/zellij/pull/488)
* Add support for requesting a simpler layout from plugins, move `clean` flag from `options` to `setup` (https://github.com/zellij-org/zellij/pull/479)
* Improve config loading slightly (https://github.com/zellij-org/zellij/pull/492)
* Terminal compatibility: preserve current style when clearing viewport (https://github.com/zellij-org/zellij/pull/493)
* Fix propagation of plugin ui request (https://github.com/zellij-org/zellij/pull/495)
* Handle pasted text properly (https://github.com/zellij-org/zellij/pull/494)
* Fix default keybinds for tab -> resize mode (https://github.com/zellij-org/zellij/pull/497)
* Terminal compatibility: device reports (https://github.com/zellij-org/zellij/pull/500)
* Forward unknown keys to the active terminal (https://github.com/zellij-org/zellij/pull/501)

## [0.9.0] - 2021-05-11
* Add more functionality to unbinding the default keybindings (https://github.com/zellij-org/zellij/pull/468)
* Terminal compatibility: fix support for CSI subparameters (https://github.com/zellij-org/zellij/pull/469)
* Move the sync command to tab mode (https://github.com/zellij-org/zellij/pull/412)
* Fix exit code of `dump-default-config` (https://github.com/zellij-org/zellij/pull/480)
* Feature: Switch tabs using `Alt + h/l` in normal mode if there are no panes in the direction (https://github.com/zellij-org/zellij/pull/471) 
* Terminal Compatibility: various behaviour fixes (https://github.com/zellij-org/zellij/pull/486)
* Fix handling of `$HOME` `config` directory, especially relevant for darwin systems (https://github.com/zellij-org/zellij/pull/487)

## [0.8.0] - 2021-05-07
* Terminal compatibility: pass vttest 8 (https://github.com/zellij-org/zellij/pull/461)
* Add a Manpage (https://github.com/zellij-org/zellij/pull/455)
* Code infrastructure changes to support the upcoming session detach (https://github.com/zellij-org/zellij/pull/223)

## [0.7.0] - 2021-05-04
* Fix the tab '(Sync)' suffix in named tabs (https://github.com/zellij-org/zellij/pull/410)
* Improve performance when multiple panes are open (https://github.com/zellij-org/zellij/pull/318)
* Improve error reporting and tests of configuration (https://github.com/zellij-org/zellij/pull/423)
* Refactor install module to setup module (https://github.com/zellij-org/zellij/pull/431)
* Add theme support through xrdb (https://github.com/zellij-org/zellij/pull/239)
* Fix default keybindings in resize mode and add arrow parity in tab and scroll mode (https://github.com/zellij-org/zellij/pull/441)
* Terminal compatibility: pass vttest 2 and 3 (https://github.com/zellij-org/zellij/pull/447)
* Stabilize colors (https://github.com/zellij-org/zellij/pull/453)

## [0.6.0] - 2021-04-29
* Doesn't quit anymore on single `q` press while in tab mode  (https://github.com/zellij-org/zellij/pull/342)
* Completions are not assets anymore, but commands `option --generate-completion [shell]` (https://github.com/zellij-org/zellij/pull/369)
* Fixes in the default configuration `default.yaml` file. Adds initial tmux-compat keybindings `tmux.yaml` (https://github.com/zellij-org/zellij/pull/362)
* Added the `get_plugin_ids()` query function to the plugin API (https://github.com/zellij-org/zellij/pull/392)
* Implemented simple plugin timers via the `set_timeout()` call (https://github.com/zellij-org/zellij/pull/394)
* Added more configuration locations, changed `ZELLIJ_CONFIG` to `ZELLIJ_CONFIG_FILE` (https://github.com/zellij-org/zellij/pull/391)
* Improved keybind handling (https://github.com/zellij-org/zellij/pull/400)
* Added initial screen-compat keybinds `screen.yaml` (https://github.com/zellij-org/zellij/pull/399)
* Added the ability to synchronize input sent to panes (https://github.com/zellij-org/zellij/pull/395)
* Terminal fix: pass vttest 1 (https://github.com/zellij-org/zellij/pull/408)

## [0.5.1] - 2021-04-23
* Change config to flag (https://github.com/zellij-org/zellij/pull/300)
* Add ZELLIJ environment variable on startup (https://github.com/zellij-org/zellij/pull/305)
* Terminal fix: do not clear line if it's not there (https://github.com/zellij-org/zellij/pull/289)
* Do not allow opening new pane on the status bar (https://github.com/zellij-org/zellij/pull/314)
* Allow scrolling by full pages (https://github.com/zellij-org/zellij/pull/298)
* Reduce crate size by 4.8MB using `cargo diet`, to 77kB (https://github.com/zellij-org/zellij/pull/293)
* Draw UI properly when instantiated as the default terminal command (https://github.com/zellij-org/zellij/pull/323)
* Resolve ambiguous pane movements by their activity history (https://github.com/zellij-org/zellij/pull/294)

## [0.5.0] - 2021-04-20
Beta release with all the things
