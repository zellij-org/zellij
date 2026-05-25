# zellij-tappable-status-bar

A fork of zellij v0.44.3 (commit 55a2121b) that makes the default
`status-bar` plugin's one-line UI **mouse-tappable**. Click on a mode
ribbon (`<z> LOCK`, `<p> PANE`, `<t> TAB`, `<n> RESIZE`, `<h> MOVE`,
`<s> SEARCH`, `<o> SESSION`, `<q> QUIT`) or on the right-hand-side
quick-actions (`Alt + <n> New Pane`, `<f> Floating`) and they fire
the same action as the keyboard shortcut.

Only changes are in `default-plugins/status-bar/`.

## Build

```
cargo build --release -p status-bar --target wasm32-wasip1
```

Output: `target/wasm32-wasip1/release/status-bar.wasm`.

## Install (Zellij config)

Copy the wasm somewhere and point `status-bar` at it in `~/.config/zellij/config.kdl`:

```kdl
plugins {
    status-bar location="file:~/.config/zellij/plugins/tappable-status-bar.wasm"
    // ...
}
```

## How it works

- `LinePart` got a `regions: Vec<HitRegion>` field. Each ribbon-emitting
  helper (`add_shortcut`, `add_shortcut_with_inline_key`,
  `add_shortcut_with_key_only`) optionally records a `HitRegion` (a
  column range tagged with a `ClickAction`).
- `LinePart::append` merges sub-regions and shifts them by the current
  column offset, so the final rendered line carries an accurate
  left-to-right hit map.
- The plugin subscribes to `EventType::Mouse`. On
  `Mouse::LeftClick(line, col)` with `line == 0`, it looks up the
  matching region and dispatches its `ClickAction` via
  `switch_to_input_mode`, `quit_zellij`, or `run_action` (for
  `NewPane` / `ToggleFloatingPanes`).
- The two-row (classic) status bar is unaffected and not tappable.
- The plugin now requests `ChangeApplicationState` so it can
  actually run those actions.

## Why a whole-repo fork

The plugin depends on path crates (`zellij-tile`, `zellij-tile-utils`)
that live in the zellij monorepo. Cloning the whole repo and tweaking
just `default-plugins/status-bar/` is the path of least resistance.
