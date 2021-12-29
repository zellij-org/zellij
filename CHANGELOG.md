# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]
* Terminal compatibility: properly handle insertion of characters in a line with wide characters (https://github.com/zellij-org/zellij/pull/964)
* Terminal compatibility: properly handle deletion of characters in a line with wide characters (https://github.com/zellij-org/zellij/pull/965)
* Fix: properly remove clients when detaching from a session (https://github.com/zellij-org/zellij/pull/966)
* Fix: plugin theme coloring (https://github.com/zellij-org/zellij/pull/975)
* Fix: prevent unhandled mouse events escape to terminal (https://github.com/zellij-org/zellij/pull/976)
* Fix: ensure clippy runs on all targets (https://github.com/zellij-org/zellij/pull/972) 

## [0.23.0] - 2021-12-20
* Feature: add collaboration support - multiple users using multiple cursors (https://github.com/zellij-org/zellij/pull/957)

## [0.22.1] - 2021-12-14
* Hotfix: Focus fullscreen pane when switching tab focus (https://github.com/zellij-org/zellij/pull/941)

## [0.22.0] - 2021-12-13
* Fix: missing themes in configuration merge (https://github.com/zellij-org/zellij/pull/913)
* Fix: add `gray` to theme section (https://github.com/zellij-org/zellij/pull/914)
* Fix: prevent zellij session from attaching to itself (https://github.com/zellij-org/zellij/pull/911)
* Terminal compatibility: fix flaky scrolling issue (https://github.com/zellij-org/zellij/pull/915)
* Fix: handle pasted text properly in windows terminal (https://github.com/zellij-org/zellij/pull/917)
* Fix: update example config options (https://github.com/zellij-org/zellij/pull/920)
* Fix: correct handling of unbinds (https://github.com/zellij-org/zellij/issues/923)
* Fix: improve perfomance when resizing window with a large scrollback buffer (https://github.com/zellij-org/zellij/pull/895)
* Fix: support multiple users in plugins (https://github.com/zellij-org/zellij/pull/930)
* Fix: update default layouts (https://github.com/zellij-org/zellij/pull/926)
* Add: infrastructure to show distinct tips in the `status-bar` plugin (https://github.com/zellij-org/zellij/pull/926)
* Feature: Allow naming panes (https://github.com/zellij-org/zellij/pull/928)

## [0.21.0] - 2021-11-29
* Add: initial preparations for overlay's (https://github.com/zellij-org/zellij/pull/871)
* Add: initial `zellij.desktop` file (https://github.com/zellij-org/zellij/pull/870)
* Add: section for third party repositiories `THIRD_PARTY_INSTALL.md` (https://github.com/zellij-org/zellij/pull/857)
* Add: suggestion for similar session name, on attach (https://github.com/zellij-org/zellij/pull/843)
* Fix: handling and overwriting options through the cli (https://github.com/zellij-org/zellij/pull/859)

  THIS IS A BREAKING CHANGE:
  Previously it was only possible to turn off certain features through the cli,
  now it also is possible to overwrite this behavior - for that the following changed:

  - renamed and inverted:
  ```
  disable_mouse_mode -> mouse_mode
  no_pane_frames -> pane_frames
  ```
  - cli options added:
  ```
  mouse-mode [bool]
  pane-frames [bool]
  simplified-ui [bool]
  ```
  - cli flag removed:
  ```
  simplified-ui
  ```

  Now the cli options can optionally be toggled on, even if the config
  turns it off, example:
  ```
  zellij options --mouse-mode true
  ```
* Fix: fix CSI cursor next line not moving cursor to beginning of line after moving it down (https://github.com/zellij-org/zellij/pull/863)
* Refactor: Support multiple users in `Tab`s (https://github.com/zellij-org/zellij/pull/864)
* Refactor: close_pane returns closed pane (https://github.com/zellij-org/zellij/pull/853)
* Add: ability to configure zellij through layouts (https://github.com/zellij-org/zellij/pull/866)
* Refactor: simplify terminal character style diff (https://github.com/zellij-org/zellij/pull/839)
* Fix: improve performance with large scrollback buffer (https://github.com/zellij-org/zellij/pull/881)
* Add: support osc8 escape code (https://github.com/zellij-org/zellij/pull/822)
* Add: optionally leave ephemeral modes by pressing the `esc` key to default config (https://github.com/zellij-org/zellij/pull/889)
* Feature: Multiple users UI for panes behind a turned-off feature flag (https://github.com/zellij-org/zellij/pull/897)
* Add: plugin api, to provide version information to plugins (https://github.com/zellij-org/zellij/pull/894)


## [0.20.1] - 2021-11-10
* Add: initial session name to layout template (https://github.com/zellij-org/zellij/pull/789)
* Fix: simplify matches (https://github.com/zellij-org/zellij/pull/844)
* Add: support darwin builds on ci (https://github.com/zellij-org/zellij/pull/846)
* Add: e2e instructions for x86 and arm darwin systems (https://github.com/zellij-org/zellij/pull/846)
* Fix: use key-value style for `docker-compose` (https://github.com/zellij-org/zellij/issues/338)
* Fix: unify zellij environment variable handling (https://github.com/zellij-org/zellij/pull/842)
* Add: toggle boolean options with cli flags (https://github.com/zellij-org/zellij/pull/855)

* HOTFIX: fix pasting regression (https://github.com/zellij-org/zellij/pull/858)

## [0.20.0] - 2021-11-08
* Fix: improve performance of echoed keystrokes (https://github.com/zellij-org/zellij/pull/798)
* Add: Use hyperlinks for the setup information (https://github.com/zellij-org/zellij/pull/768)
* Feature: Rotate Pane location (https://github.com/zellij-org/zellij/pull/802)
* Terminal compatibility: improve handling of wide-characters when inserted mid-line (https://github.com/zellij-org/zellij/pull/806)
* Fix: plugins are now only compiled once and cached on disk (https://github.com/zellij-org/zellij/pull/807)
* Fix: pasted text performs much faster and doesn't kill Termion (https://github.com/zellij-org/zellij/pull/810)
* Fix: resizing/scrolling through heavily wrapped panes no longer hangs (https://github.com/zellij-org/zellij/pull/814)
* Terminal compatibility: properly handle HOME/END keys in eg. vim/zsh (https://github.com/zellij-org/zellij/pull/815)
* Fix: Typo (https://github.com/zellij-org/zellij/pull/821)
* Fix: Update `cargo-make` instructions post `v0.35.3` (https://github.com/zellij-org/zellij/pull/819)
* Fix: Unused import for darwin systems (https://github.com/zellij-org/zellij/pull/820)
* Add: `WriteChars` action (https://github.com/zellij-org/zellij/pull/825)
* Fix: typo and grammar (https://github.com/zellij-org/zellij/pull/826)
* Add: `rust-version` - msrv field to `Cargo.toml` (https://github.com/zellij-org/zellij/pull/828)
* Fix: improve memory utilization, reap both sides of pty properly and do not expose open FDs to child processes (https://github.com/zellij-org/zellij/pull/830)
* Fix: move from the deprecated `colors_transform` to `colorsys` (https://github.com/zellij-org/zellij/pull/832)
* Feature: plugins can now detect right mouse clicks (https://github.com/zellij-org/zellij/pull/801)
* Fix: open pane in cwd even when explicitly specifying shell (https://github.com/zellij-org/zellij/pull/834)
* Fix: do not resize panes below minimum (https://github.com/zellij-org/zellij/pull/838)
* Feature: Non directional resize of panes (https://github.com/zellij-org/zellij/pull/520)
* Add: `colored` crate to replace manual color formatting (https://github.com/zellij-org/zellij/pull/837)
* Add: introduce `thiserrror` to simplify error types (https://github.com/zellij-org/zellij/pull/836)
* Add: support `--index` option for the `attach` subcommand in order to
  choose the session indexed by the provided creation date (https://github.com/zellij-org/zellij/pull/824)
* Fix: simplify the main function significantly (https://github.com/zellij-org/zellij/pull/829)
* Feature: half page scrolling actions (https://github.com/zellij-org/zellij/pull/813)

## [0.19.0] - 2021-10-20
* Fix: Prevent text overwrite when scrolled up (https://github.com/zellij-org/zellij/pull/655)
* Add: Treat empty config files as empty yaml documents (https://github.com/zellij-org/zellij/pull/720)
* Fix: Commands that don't interact with the config file don't throw errors on malformed config files (https://github.com/zellij-org/zellij/pull/765)
* Add: Add config options to default config file (https://github.com/zellij-org/zellij/pull/766)
* Fix: Properly clear "FULLSCREEN" status when a pane exits on its own (https://github.com/zellij-org/zellij/pull/757)
* Refactor: handle clients in tabs/screen (https://github.com/zellij-org/zellij/pull/770)
* Feature: kill-session and kill-all-sessions cli commands (https://github.com/zellij-org/zellij/pull/745)
* Fix: Keep default file permissions for new files (https://github.com/zellij-org/zellij/pull/777)
* Feature: Add mouse events to plugins – including strider and the tab-bar (https://github.com/zellij-org/zellij/pull/629)
* Feature: Directional movement of panes (https://github.com/zellij-org/zellij/pull/762)
* Refactor: More groundwork to support multiple-clients in tabs (https://github.com/zellij-org/zellij/pull/788)

## [0.18.1] - 2021-09-30

* HOTFIX: mouse selection now working (https://github.com/zellij-org/zellij/pull/752)
* HOTFIX: prevent strider from descending into /host folder (https://github.com/zellij-org/zellij/pull/753)

## [0.18.0] - 2021-09-29
* Fix: Properly open new pane with CWD also when switching to a new tab (https://github.com/zellij-org/zellij/pull/729)
* Feature: Option to create a new session if attach fails (`zellij attach --create`) (https://github.com/zellij-org/zellij/pull/731)
* Feature: Added the new `Visible` event, allowing plugins to detect if they are visible in the current tab (https://github.com/zellij-org/zellij/pull/717)
* Feature: Plugins now have access to a data directory at `/data` – the working directory is now mounted at `/host` instead of `.` (https://github.com/zellij-org/zellij/pull/723)
* Feature: Add ability to solely specify the tab name in the `tabs` section (https://github.com/zellij-org/zellij/pull/722)
* Feature: Plugins can be configured and the groundwork for "Headless" plugins has been laid (https://github.com/zellij-org/zellij/pull/660)
* Automatically update `example/default.yaml` on release (https://github.com/zellij-org/zellij/pull/736)
* Feature: allow mirroring sessions in multiple terminal windows (https://github.com/zellij-org/zellij/pull/740)
* Feature: display a message when the current pane is in full-screen (https://github.com/zellij-org/zellij/pull/450)
* Terminal compatibility: handle cursor movements outside scroll region (https://github.com/zellij-org/zellij/pull/746)
* Terminal compatibility: scroll lines into scrollback when clearing viewport (https://github.com/zellij-org/zellij/pull/747)

## [0.17.0] - 2021-09-15
* New panes/tabs now open in CWD of focused pane (https://github.com/zellij-org/zellij/pull/691)
* Fix bug when opening new tab the new pane's viewport would sometimes be calculated incorrectly (https://github.com/zellij-org/zellij/pull/683)
* Fix bug when in some cases closing a tab would not clear the previous pane's contents (https://github.com/zellij-org/zellij/pull/684)
* Fix bug where tabs would sometimes be created with the wrong index in their name (https://github.com/zellij-org/zellij/pull/686)
* Fix bug where wide chars would mess up pane titles (https://github.com/zellij-org/zellij/pull/698)
* Fix various borderless-frame in viewport bugs (https://github.com/zellij-org/zellij/pull/697)
* Fix example configuration file (https://github.com/zellij-org/zellij/pull/693)
* Fix various tab bar responsiveness issues (https://github.com/zellij-org/zellij/pull/703)
* Allow plugins to run system commands (https://github.com/zellij-org/zellij/pull/666)
  * This has also added a temporary new permission flag that needs to be specified in the layout. This is a breaking change:
    ```yaml
    ...
    plugin: strider
    ...
    ```
    has become:
    ```yaml
    plugin:
      path: strider
    ```
    A plugin can be given command executing permission with:
    ```yaml
    plugin:
      path: strider
      _allow_exec_host_cmd: true
    ```
* Use the unicode width in tab-bar plugin, for tab names (https://github.com/zellij-org/zellij/pull/709)
* Fix automated builds that make use of the `setup` subcommand (https://github.com/zellij-org/zellij/pull/711)
* Add option to specify a tabs name in the tab `layout` file (https://github.com/zellij-org/zellij/pull/715)
* Improve handling of empty valid `yaml` files (https://github.com/zellij-org/zellij/pull/716)
* Add options subcommand to attach (https://github.com/zellij-org/zellij/pull/718)
* Fix: do not pad empty pane frame title (https://github.com/zellij-org/zellij/pull/724)
* Fix: Do not overflow empty lines when resizing panes (https://github.com/zellij-org/zellij/pull/725)


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
