# Vendored copy of `termwiz`

## Why do we need this?
We need the fix for parsing partial SGR mouse sequences from https://github.com/wezterm/wezterm/pull/7504, which is not yet part of a release.

## Origin
This module is a copy of [`termwiz`](https://crates.io/crates/termwiz) 0.23.3. The source code was copied from the directory `termwiz/src` in the [wezterm](https://github.com/wezterm/wezterm) repository (tag `termwiz-0.23.3` / commit `4bf28e253c0167102f07bfc7e7199c13eed98012`).

## Changes applied
* All code was formatted with this repo's `rustfmt` settings
* `lib.rs` was renamed to `mod.rs`
* all `crate::` references were updated to `crate::vendored::termwiz::`
* all top-level exported macros were prefixed with `vendored_termwiz_`
* `escape/apc.rs` was updated to work with the same version of `nix` as `zellij-utils`
* references to the deprecated `clippy::cyclomatic_complexity` rule were removed from `escape/osc.rs` and `render/terminfo.rs`
* `tmux_cc/mod.rs` was updated to use a prefixed path to the `pest` grammar and to not use `env_logger`
* `termwiz/data/xterm-256color` was copied under `caps/` and all references to it were updated
