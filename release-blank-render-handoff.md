# Mobile web-client release-only blank-render — handoff

Facts-only handoff. Statements are confirmed facts, direct user observations, or
current code state. No cause is asserted.

## Goal
Investigate a rendering problem in the Zellij `zellij:mobile` plugin that
reproduces **only in `--release` builds** of the web (browser) client. Repo: a
Zellij fork with a mobile-web feature branch.

## Symptom (direct user observations — not yet root-caused)
- When a web client loads or attaches to a session, the mobile plugin shows its
  chrome and the cursor in roughly the right place, but the **embedded pane
  contents are blank**, and the **session name + pane name on the top mobile bar
  are blank**.
- **Tapping once** on the screen makes the content appear.
- Reproduces only in `--release`; the user is **unable to reproduce it in a
  non-`--release` build**. (This makes it timing-sensitive.)

Note: some of the above was observed while an experimental change to
`is_ready_to_render` (see below) was in the tree; that change has since been
reverted, so re-confirm the exact current symptom against current code.

## Reproduction conditions
- Build: `cargo x build --release` (debug builds do not reproduce).
- Connect/attach a web client (small/mobile viewport) so it routes into the
  `zellij:mobile` plugin (a full-screen plugin pane in a per-client private
  "Mobile" tab).
- Logs: `/tmp/zellij-1000/zellij-log/zellij.log`. Browser-side logging requires a
  temporary control-channel `DebugLog` mechanism (was added and removed this
  session; see git history if you want to re-add it).

## How mobile rendering works (confirmed facts, with code locations)
1. The plugin (`default-plugins/mobile/src/`) renders an embedded view of the
   user's focused real pane. It gets that pane's content from
   `Event::PaneRenderReportWithAnsi`, stored in `workspace.latest_pane_contents`
   (`default-plugins/mobile/src/main.rs`, `Event::PaneRenderReportWithAnsi` arm).
2. The plugin gates its own rendering via `State::is_ready_to_render()`
   (`default-plugins/mobile/src/main.rs`). **Current committed behavior:** on the
   Viewport screen it returns `viewport_pane_has_content()`, i.e. it renders only
   when the current pane exists AND its `latest_pane_contents` viewport is
   non-empty. (An experimental variant `current_pane().is_some()` was tried and
   reverted.)
3. Server → plugin pane reports: `Screen` drains a per-render `PaneRenderReport`
   (`zellij-server/src/screen.rs`, `output.drain_pane_render_report()` →
   `PluginInstruction::PaneRenderReport`), then
   `WasmBridge::handle_pane_render_report`
   (`zellij-server/src/plugins/wasm_bridge.rs`) calls
   `get_changed_panes_per_client`, which **filters out panes whose content is
   unchanged vs the previous report** before emitting
   `Event::PaneRenderReportWithAnsi` to the plugin. `PaneContents` fields:
   `viewport: Vec<String>`, `selected_text`, `cursor: Option<(usize,usize)>`
   (`zellij-utils/src/data.rs`).
4. Server-side blank gate: `MobileRenderGate`
   (`zellij-server/src/mobile_mode.rs`) sends gated clients only `CLEAR_SCREEN`
   (`\u{1b}[2J\u{1b}[H`) until "revealed". Reveal requires the client reported a
   settled viewport size AND a paint whose size matches it.
5. Hold-until-settled: when a gated client enters mobile, the plugin's renders
   are held (kept "pending" in `cached_events_for_pending_plugins` /
   `cached_resizes_for_pending_plugins`) via
   `clients_holding_mobile_render_until_size_settled`
   (`zellij-server/src/plugins/wasm_bridge.rs`), and drained once when the
   browser reports `SizeSettled`. Browser sends the settled size from
   `zellij-client/assets/websockets.js` (`SetConfig` handler, cause `"Settled"` →
   `TerminalSizeSettled` → `ResizeCause::SizeSettled` →
   `ScreenInstruction::MobileSizeSettled`).

## What is already fixed (committed; don't re-investigate these)
- The earlier **startup "pan"/flash** (content briefly rendered centered for a
  stale width 80→46→31) is fixed by the hold/release mechanism above. Commits
  `d515c891` and `d22f388b "fix: various mobile startup issues"`.
- Proven during that work (facts): the server only ever sent correct-size
  (31-col) bytes to the browser by reveal time; WebGL was exonerated (disabling
  it did not remove the flash); xterm cell-pixel metrics and container geometry
  were stable from the first measurement.
- The welcome screen has its own gating in `is_ready_to_render`: empty Viewport
  "(no pane)" is suppressed, and an empty welcome session list is suppressed for
  a 400 ms grace (`set_timeout`, `Event::Timer`, `empty_welcome_list_grace_elapsed`)
  to ride out transient empty `SessionUpdate`s.

## Relevant uncommitted change in the render-report path
- `zellij-server/src/plugins/wasm_bridge.rs`: `get_changed_panes_per_client` now
  also reports when `cursor` changes. Before this change, a cursor-only move (no
  `viewport`/`selected_text` change) produced **no**
  `Event::PaneRenderReportWithAnsi`, so embedded-pane viewers like the mobile
  plugin did not repaint. This is in the same change-detection code the
  blank-render investigation will examine, so note which state the tree is in.

## Diagnostic facts gathered (from `/tmp/zellij-1000/zellij-log/zellij.log`)
- `get_changed_panes_per_client` was observed emitting **no** report for a pane
  when only the cursor moved (e.g. `viewport_changed=false … cursor_changed=true
  -> reported=false`) prior to the cursor fix. Implication to verify: an attach
  where only the cursor/position differs from the previous report could be
  filtered out, leaving the plugin with no content event until something else
  changes (e.g. a tap).
- A "tap" on the embedded viewport goes through `Mouse::LeftClick` →
  `mouse::handle_left_click` in the mobile plugin (`default-plugins/mobile/src/`),
  which selects a pane / sets shadow focus. (Worth instrumenting what a tap
  changes that makes content appear.)

## Suggested starting points (not conclusions)
- Confirm the current symptom against the current committed `is_ready_to_render`
  (it requires non-empty pane content on Viewport).
- Instrument the full path with timestamps and reproduce: (a) does an
  `Event::PaneRenderReportWithAnsi` for the focused real pane arrive at the plugin
  on attach, before the tap? (b) does `get_changed_panes_per_client` filter it
  (`reported=false`)? (c) what does a tap change (focus / shadow-focus / a forced
  render) that produces the content?
- Establish whether the blank is because no report is generated, the report is
  filtered as "unchanged", or the plugin's `is_ready_to_render` gate is false at
  that moment.

## Key files
- Plugin: `default-plugins/mobile/src/main.rs` (`is_ready_to_render`, event
  handling), `default-plugins/mobile/src/screens/viewport.rs`,
  `default-plugins/mobile/src/pane_sync.rs`,
  `default-plugins/mobile/src/workspace.rs` (`current_pane`,
  `latest_pane_contents`).
- Server: `zellij-server/src/plugins/wasm_bridge.rs`
  (`handle_pane_render_report`, `get_changed_panes_per_client`, hold/release),
  `zellij-server/src/mobile_mode.rs` (`MobileRenderGate`),
  `zellij-server/src/screen.rs` (render path, `drain_pane_render_report`),
  `zellij-server/src/route.rs`.
- Browser: `zellij-client/assets/websockets.js` (settled-size signal),
  `zellij-client/src/web_client/{message_handlers.rs,websocket_handlers.rs}`.
- Data types: `zellij-utils/src/data.rs` (`PaneContents`, `PaneRenderReport`).
- Prior facts-only handoffs in repo root: `mobile-render-flash-handoff.md`,
  `mobile-render-offset-investigation.md`.
