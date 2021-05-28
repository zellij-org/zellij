# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]
* Fix crash when padding before widechar (https://github.com/zellij-org/zellij/pull/540)
* Do not lag when reading input too fast (https://github.com/zellij-org/zellij/pull/536)

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
