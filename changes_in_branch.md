# Mobile web-client startup render — changes in branch

Functional summary of the uncommitted work that removes the startup render
"flash"/"pan" when a web (browser) client connects with no session name and is
routed into the `zellij:mobile` plugin.

## Problem

On a `--release` web connection the mobile UI was briefly visible in a wrong
state before settling: content panned to the right, then re-centered; the mobile
plugin's main chrome flashed before the welcome screen; and the welcome session
list flashed empty before it populated. Root causes were a render-size race and
transient plugin state, not bad geometry.

## Server-side mobile render gate

`zellij-server/src/mobile_mode.rs` — `MobileRenderGate`: gated clients receive
only a `CLEAR_SCREEN` frame (`blank_gated_clients`) until they are revealed.
A client is revealed once it has reported a settled viewport size and a matching
paint has arrived (`record_settled_size` / `record_paint_size` / `try_reveal`).

`zellij-server/src/screen.rs` — `MobileRenderGate` field on `Screen`;
`ScreenInstruction::SuppressRenderUntilMobile` / `MobileSizeSettled` /
`ForceMobileUngate`; `render_to_clients` blanks gated clients; `PluginBytes`
records paint size and calls `try_lift_mobile_gate`; a background
`MobileGateTimeout` force-reveals after a fallback delay.

## Defer plugin render until the viewport size settles (core fix)

The gate keyed reveal on *pane geometry*, but the plugin's bytes are laid out at
the width the plugin used to render, which lags the pane resize — so revealed
content walked `80 → 46 → 31` columns ("pan"). Fixed by not rendering the mobile
plugin at all until the size settles, then rendering once at the final width:

`zellij-server/src/plugins/wasm_bridge.rs` — `held_mobile_render_clients`.
While a client is held, `apply_cached_events` skips draining its plugins, so they
stay "pending" and every resize/event is cached instead of rendered (reusing the
load-pending machinery). `hold_mobile_render` / `release_mobile_render`; release
applies the final cached **resize before** the cached events, so the single
render — and the replayed events — use the settled width (not the stale load
width).

`zellij-server/src/plugins/mod.rs` + `zellij-utils/src/errors.rs` —
`PluginInstruction::HoldMobileRender` / `ReleaseMobileRender` + contexts +
handlers.

`zellij-server/src/screen.rs` — sends `HoldMobileRender` on gated mobile entry
(guarded on `is_gated`; `is_web` reads false at that instant). Sends
`ReleaseMobileRender` on `MobileSizeSettled`, on the timeout `ForceMobileUngate`,
and in `exit_mobile_mode`.

## Settled-size signal

The browser tells the server when its viewport size is final so the gate can
reveal:

- `zellij-client/assets/websockets.js` — on `SetConfig`, after applying the
  final font, loads the WebGL renderer and sends a size update with cause
  `"Settled"`.
- `zellij-client/src/web_client/{control_message.rs,websocket_handlers.rs}` and
  `zellij-utils` IPC — `TerminalSizeSettled` → `ResizeCause::SizeSettled`, routed
  to `ScreenInstruction::MobileSizeSettled`.

## Mobile plugin — welcome-screen render gating

`default-plugins/mobile/src/{main.rs,pane_sync.rs,state.rs}`:

- `is_ready_to_render` no longer paints the empty Viewport "(no pane)" chrome
  (returns not-ready when no pane is selected), so the plugin's main chrome does
  not flash before `maybe_take_over_welcome` switches to the welcome screen.
- On the welcome screen, an **empty** session list is suppressed until a 400 ms
  grace elapses (armed via `set_timeout` on welcome entry, cleared by a `Timer`
  event → `welcome_grace_elapsed`). A non-empty list renders immediately. This
  hides the transient empty `SessionUpdate` events (`raw=1 filtered=0`) the
  session manager emits at startup, which would otherwise clobber the list to
  empty for a frame.

## Browser startup rendering

`zellij-client/assets/terminal.js` — the WebGL addon is created but its load is
deferred until `SetConfig` has applied the final font (the DOM renderer reflows
the startup font resizes cleanly; WebGL takes over once the size is stable).

`zellij-client/assets/style.css` — dark page background so the pre-paint gap is
not a white flash.
