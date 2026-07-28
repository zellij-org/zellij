# Mobile plugin render-offset investigation

A facts-only handoff. Every statement below is backed by a code reference or a log
observation from this session. Items not yet determined are listed separately and explicitly.

## Problem

When a web client (browser) attaches to a Zellij session that routes to the `zellij:mobile`
plugin, the mobile plugin renders its UI at a width larger than the client's actual terminal
width for roughly 2–4 seconds after attach, then settles to the correct width. The mobile
plugin's Sessions/session-manager screen horizontally centers its content on the width it is
given, so while the width is too large the content is positioned starting past the middle of
the screen and "snaps" to the correct position once the width settles.

## Code changes already made in this session (present in the working tree)

These are modifications made during the current investigation. They are NOT part of the
original codebase and are relevant context for anything observed.

1. Per-client render gate in `zellij-server/src/screen.rs`:
   - New `Screen` field `clients_awaiting_first_mobile_render: HashSet<ClientId>` and helpers
     `gate_client_until_mobile`, `is_client_gated`, `ungate_client`,
     `ungate_clients_for_mobile_plugin`.
   - New instruction `ScreenInstruction::SuppressRenderUntilMobile(ClientId)` (variant +
     `ScreenContext` variant in `zellij-utils/src/errors.rs`) whose handler sets the gate.
   - `render_to_clients` removes gated client IDs from the serialized output before sending
     `ServerInstruction::Render` (skips send entirely if the map becomes empty).
   - The gate is lifted in the `PluginBytes` handler when a non-empty paint arrives for the
     client's mobile tab; also lifted in `remove_client`, `exit_mobile_mode`,
     `EnterMobileMode` error path, and `UpdatePluginLoadingStage` on `is_error()`.
2. `zellij-server/src/lib.rs` `AttachClient` and `FirstClientConnected`:
   - `should_enter_mobile` is computed before the first render-producing instruction.
   - For web clients it is `mobile_layout.may_route_web_client_to_mobile()`; for terminal
     clients it is `mobile_layout.should_route_to_mobile(...)`.
   - When true, `SuppressRenderUntilMobile` is sent, and `EnterMobileMode` is sent (i.e. web
     clients now enter mobile at attach time).
3. `zellij-utils/src/input/options.rs`: added
   `MobileLayoutConfiguration::may_route_web_client_to_mobile()` (`Web | Always`).
4. `default-plugins/mobile/src/main.rs`: `update()` returns `should_render &&
   is_ready_to_render()`; `render()` early-returns unless `is_ready_to_render()`.
   `is_ready_to_render()` requires workspace tabs/panes to be non-empty and, on the Viewport
   screen, the selected pane's content to be present in `latest_pane_contents`. The
   `render_stub` ("mobile plugin loaded — RxC") function was removed from
   `default-plugins/mobile/src/render.rs`.
5. Debug logging tagged `[mobile-settle]` is currently present (see "Debug logging" below).

The WASM plugin asset must be rebuilt and re-embedded for plugin-side changes to take effect.

## Confirmed facts — server code paths

- The mobile Sessions screen centers content on the given width:
  `default-plugins/mobile/src/screens/sessions.rs:326` (`card_x = cols.saturating_sub(card_w) / 2`)
  and `:348-349` (`centered_x = self.cols.saturating_sub(label_w) / 2`).
- `enter_mobile_mode` (`zellij-server/src/screen.rs:1716`) creates the mobile tab via
  `new_tab`; the mobile tab contains a single borderless plugin pane (`zellij:mobile`).
- A client `TerminalResize { new_size, cause }` is handled in `zellij-server/src/route.rs:2370`:
  it calls `set_client_size(client_id, new_size)` and sends
  `ScreenInstruction::RecomputeTabSize(client_id, new_size)`. If `cause` is
  `ResizeCause::Viewport` it additionally sends `ScreenInstruction::ReevaluateMobileMode`.
- `RecomputeTabSize` handler (`zellij-server/src/screen.rs:7735`) calls
  `recompute_tab_size(active_tab_id_of_client)`.
- `recompute_tab_size` (`zellij-server/src/screen.rs:2329`) computes the new tab size as the
  minimum `client_sizes` among clients whose active tab is this tab, and calls
  `tab.resize_whole_tab(new_size)` only when `tab.size != new_size`.
- `resize_whole_tab` (`zellij-server/src/tab/mod.rs:3550`) sets `self.size`, calls
  `tiled_panes.resize(new_size)`, then (auto_layout) `relayout_tiled_panes`, then
  `LayoutApplier::offset_viewport(...)`.
- `tiled_panes.resize` (`zellij-server/src/panes/tiled_panes/mod.rs:1274`) updates the shared
  `display_area` (`:1341-1342`).
- `offset_viewport` (`zellij-server/src/tab/layout_applier.rs:1032`) resets the viewport to
  `display_area` (`:1041`) and then calls `tiled_panes.set_pane_frames` (`:1074`).
- `set_pane_frames` (`zellij-server/src/panes/tiled_panes/mod.rs:556`) iterates all panes and
  calls the `resize_pty!` macro for each (`:611`).
- `resize_pty!` (`zellij-server/src/tab/mod.rs:72-138`) for a `PaneId::Plugin` sends
  `PluginInstruction::Resize(pid, pane.get_content_columns(), pane.get_content_rows())`.
- `PluginInstruction::Resize` is handled in `zellij-server/src/plugins/mod.rs:504`, calling
  `wasm_bridge.resize_plugin`.
- `resize_plugin` (`zellij-server/src/plugins/wasm_bridge.rs:794`):
  - Filters out (skips) any plugin whose id is in `cached_resizes_for_pending_plugins`
    (`:810-814`); for such a plugin it only updates the cached size (`:879-883`).
  - For a non-skipped plugin it sets `running_plugin.rows/columns = new` and, if the size
    changed, calls the WASM `render(new_rows, new_columns)` and sends the output as
    `ScreenInstruction::PluginBytes` (`:838-869`).
- `cached_resizes_for_pending_plugins` is populated at load start
  (`zellij-server/src/plugins/wasm_bridge.rs:331`) and drained in
  `apply_cached_events_and_resizes_for_plugin` (`:1507`, drain at `:1613-1614`), which then
  calls `resize_plugin` with the cached size.
- `ServerInstruction::Render(Some(map))` (`zellij-server/src/lib.rs:~1505`) sends each client
  its bytes; `Render(None)` is the session-exit sentinel that disconnects all clients.

## Confirmed facts — browser code (`zellij-client/assets/`)

- The control WebSocket (`wsControl`), which carries all size reports, is created inside
  `wsTerminal.onmessage` on the FIRST terminal-channel message
  (`zellij-client/assets/websockets.js:95-101`). Until a first terminal message is received,
  `wsControl` does not exist and no size is reported.
- Size is reported over the control channel:
  - On `wsControl.onopen`: `fitAddon.proposeDimensions()` → `sendSizeUpdate(...)` with default
    cause `TerminalResize` (`websockets.js:188-191`).
  - On `SetConfig`: the browser sets font, computes `isMobileViewport`, applies a font size
    (explicit, else 24 for mobile, else 12), and for a mobile viewport without an explicit
    font size runs an adaptive loop that shrinks the font size and re-fits until
    `term.rows >= NATURAL_MIN_TOTAL_ROWS` (`websockets.js:225-259`); then sends the resulting
    size with cause `"RenderingPreference"` (`websockets.js:271-278`).
  - On `QueryTerminalSize`: `proposeDimensions()` → resize term → `sendSizeUpdate` (default
    cause) (`websockets.js:279-285`).
- `sendSizeUpdate` maps cause `"RenderingPreference"` → `TerminalResizeRendering`, otherwise
  `TerminalResize` (`websockets.js:36-39`), and also sends a `TerminalMetrics` message with
  cell pixel dimensions (`websockets.js:50-65`).
- `fitAddon.proposeDimensions()` (`zellij-client/assets/addon-fit.js`) computes cols/rows from
  `_renderService.dimensions.css.cell.width/height` and returns `undefined` when either cell
  dimension is `0`.
- Server side, these map: `TerminalResize` payload → `ClientToServerMsg::TerminalResize {
  cause: Viewport }`; `TerminalResizeRendering` → `{ cause: RenderingPreference }`;
  `TerminalMetrics` → `terminal_metrics_to_ipc(...)`
  (`zellij-client/src/web_client/websocket_handlers.rs:78-96`).

## Confirmed facts — observed timeline (log: `/tmp/zellij-1000/zellij-log/zellij.log`)

All `[mobile-settle]` lines below are from the debug logging added this session.

Run at 16:48 (web client, `client=1`, mobile plugin `pid=2`):
- `resize_plugin pid=2 to 80x24 pending=true` (repeated) — resize skipped while pending.
- `apply_cached_events_and_resizes_for_plugin pid=2 cached_resize=Some((24, 80))` — load
  completion drains the cached resize.
- `resize_plugin pid=2 to 80x24 pending=false` immediately after.
- `resize_plugin pid=2 to 46x38 pending=false` ~1.4 s later, repeated ~every 300 ms.
- `resize_plugin pid=2 to 31x25 pending=false` ~2 s after that; then steady at 31x25.

Run at 15:41 (web client, `client=1`, mobile plugin `pid=2`):
- `TerminalResize client=1 new_size=Size{rows:38,cols:46} cause=Viewport` at 15.548.
- `recompute_tab_size tab=1 old=80 new=46 will_resize=true` and `after resize_whole_tab tab=1
  tab_size=46 plugin_panes=[(Plugin(2), 46, 38)]`.
- `TerminalResize client=1 new_size=Size{rows:25,cols:31} cause=RenderingPreference` at 15.570.
- `recompute_tab_size tab=1 old=46 new=31 will_resize=true` and `after resize_whole_tab tab=1
  tab_size=31 plugin_panes=[(Plugin(2), 31, 25)]` — i.e. the plugin pane geometry was 31
  immediately after `resize_whole_tab`.
- `set_pane_frames plugin pid=2 pane_cols=31 viewport_cols=31 display_cols=31` at 15.570-571.
- Despite the above, the mobile plugin's own render logged `picker cols=80` continuously from
  ~15.34 through ~18.5 (the plugin kept rendering at 80 while the geometry was 31).

In all observed mobile attaches:
- `enter_mobile_mode client=1 tab=1 active_now=None client_size=None` — the mobile tab is
  created before the client's size is known, at the default 80x24.
- The mobile plugin's `render`/`picker` logs show `cols=80` for ~2-4 s after attach, then the
  value changes to the reported width (e.g. 46 then 31) and stays.

## Two distinct timing components observed (both contribute to the offset window)

1. Pending-skip: while the mobile plugin is in `cached_resizes_for_pending_plugins`,
   `resize_plugin` logs `pending=true` and skips the resize; the plugin continues rendering at
   its load-time size (80). This window ends at `apply_cached_events_and_resizes_for_plugin`.
2. Post-load churn: after `pending=false`, `resize_plugin` is invoked repeatedly (~300 ms
   cadence) and the size value it carries walks `80 → 46 → 31` over ~2 s (observed in the
   16:48 run).

## Not yet determined (open questions — no conclusion reached)

- Why, in the 16:48 run, the post-load `resize_plugin` size walked `80 → 46 → 31` over ~2 s
  rather than reaching 31 promptly, while in the 15:41 run `set_pane_frames`/`recompute`
  showed the plugin-pane geometry at 31 within ~20 ms of the `RenderingPreference` resize.
  The cause of this run-to-run difference has not been determined.
- In the 15:41 run, the plugin pane geometry was 31 (per `after resize_whole_tab` and
  `set_pane_frames` logs) while the plugin's own `render` continued logging `cols=80`. Whether
  this is solely the pending-skip (resizes dropped) or also involves the plugin's WASM
  `running_plugin.columns` not being updated for another reason has not been isolated for that
  run.
- The exact contribution of browser-side timing (the adaptive font-size loop / `SetConfig`
  arrival producing the `RenderingPreference` size) versus server-side delay to the total
  offset duration has not been measured.
- Whether relaxing the `resize_plugin` pending-skip (`wasm_bridge.rs:810-814`) for plugins
  that already have a running instance is safe, and whether it eliminates the offset, has not
  been tested.

## Constraints established this session

- The browser requires at least one terminal-channel message to create its control WebSocket
  (`websockets.js:95-101`); therefore suppressing all render output to a web client prevents
  it from ever reporting its size. (A web client that was gated with no terminal message and
  no attach-time mobile entry was observed to never enter mobile mode and never report a
  size.)
- Mobile-mode size reporting otherwise flows over the control channel, which is separate from
  the terminal/render channel (`zellij-client/src/web_client/websocket_handlers.rs`,
  `control_message.rs`).

## Debug logging currently in the tree (tag `[mobile-settle]`)

- `zellij-server/src/route.rs` `TerminalResize` handler: client, new_size, cause, is_watcher.
- `zellij-server/src/screen.rs`: `RecomputeTabSize` handler; `recompute_tab_size`
  (old/new/will_resize) and the plugin-pane sizes after `resize_whole_tab`; `enter_mobile_mode`
  (client, tab, active_now, client_size); `PluginInstruction::Resize handled` is in
  `plugins/mod.rs`.
- `zellij-server/src/plugins/mod.rs`: `Resize handled` (pid, cols, rows).
- `zellij-server/src/plugins/wasm_bridge.rs`: `resize_plugin` (pid, target size, pending flag);
  `apply_cached_events_and_resizes_for_plugin` (pid, cached_resize).
- `zellij-server/src/panes/tiled_panes/mod.rs` `set_pane_frames`: plugin pid, pane cols/rows,
  viewport cols, display cols.
- `default-plugins/mobile/src/main.rs` `render`: rows, cols, active screen, ready flag.
- `default-plugins/mobile/src/screens/sessions.rs` `PickerLayout::compute`: cols, body bounds,
  card_w, card_x, new_session_x.

Log file used throughout: `/tmp/zellij-1000/zellij-log/zellij.log` (grep for `mobile-settle`).
