# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [Unreleased]

## [0.7.0] - 2021-04-29
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
