# Branch Review — Features

The 56 commits / ~14.3k insertions on this branch group into eight user-facing features. Each entry lists the user-visible behaviour first, then the code that implements it.

---

## Feature 1 — Auto-detected Private Mobile UI ("secret tab") - DONE

**What the user sees.** A client connecting from a phone is automatically dropped into a dedicated "Mobile" tab that runs the mobile UI plugin. The tab is invisible to every other client on the session. Rotating the phone or resizing the browser past the threshold demotes the client back to the regular UI; an explicit `Action::ToggleMobileMode` keybinding (and an in-plugin "Exit Mobile" button) flips it manually. Manual entries are *sticky* — a later resize never demotes them. The threshold (`mobile_threshold_cols` × `mobile_threshold_rows`, defaults 60 × 30) and policy (`web` / `always` / `never`) are configurable.

**Implementation.**
- Visibility filter: `Tab::visible_to: Option<HashSet<ClientId>>` at `zellij-server/src/tab/mod.rs:171`. Honoured by every navigation/render path through `Screen::tab_visible_to` at `zellij-server/src/screen.rs:1795` and `visible_tab_positions_for_client` at `screen.rs:2108`. Tab numbering compensates via `mobile_tab_count` (`tab/mod.rs:778-782`).
- Bookkeeping on `Screen`: `mobile_tabs`, `mobile_previous_tab_ids`, `mobile_auto_entered` (`screen.rs:1542-1555`).
- Lifecycle helpers: `enter_mobile_mode` (`screen.rs:1899`), `exit_mobile_mode` (`screen.rs:1987`), `toggle_mobile_mode` (`screen.rs:2051`), `reevaluate_mobile_mode` (`screen.rs:2066`).
- Auto-route decision: `MobileLayout::should_route_to_mobile` at `zellij-utils/src/input/options.rs`. Invoked at attach in `zellij-server/src/lib.rs` (both `FirstClientConnected` and `AttachClient` arms; web clients deliberately deferred to first resize) and on every `ResizeCause::Viewport` in `zellij-server/src/route.rs`.
- Client-side `ResizeCause::Viewport` tagging on all `TerminalResize` emissions in `zellij-client/src/lib.rs`.
- Runtime actions: `Action::ToggleMobileMode` (`input/actions.rs`), `PluginCommand::ExitMobileMode` (`data.rs` + shim + handler).
- Config: `MobileLayout` enum + three `Options` fields, KDL parsing (`kdl/mod.rs`), protobuf (`common_types.proto`), `example/default.kdl` + `zellij-utils/assets/config/default.kdl` entries.
- `ScreenContext` enum entries for the new instructions: `EnterMobileMode`, `ExitMobileMode`, `ToggleMobileMode`, `ReevaluateMobileMode` (`zellij-utils/src/errors.rs`).
- Detach cleanup: `Screen::remove_client` at `screen.rs:3854-3898` (visibility-set empties → tab garbage collected).

---

## Feature 2 — The Mobile UI Itself - DONE

**What the user sees.** The mobile tab paints a custom interface: a top tab strip, a live viewport showing the user's currently-selected pane (cursor and all), a pane-picker for the current tab, and action buttons for "+ New Pane", "+ New Tab", Fit, Exit Mobile, etc. The user navigates by tapping tabs/panes; new tabs and panes open in the *session*, not in the user's own mobile tab.

**Implementation.**
- The plugin itself: `default-plugins/mobile/{main.rs, state.rs, render.rs}` (~6k lines), `Cargo.toml`, `.cargo/config.toml`, `.gitignore`.
- Bundled wasm at `zellij-utils/assets/plugins/mobile.wasm`; registered in the bundled-plugin asset list at `zellij-utils/src/consts.rs`.
- Alias resolution: `input/plugins.rs` (`zellij:mobile` tag); URL constant resolved by `Screen::mobile_plugin_url`.
- Workspace wiring: `Cargo.toml`, `Cargo.lock`, `xtask/src/main.rs`.
- Two plugin commands the mobile plugin needs so it does not yank itself off its own tab:
  - `PluginCommand::NewTabUnfocused` — dispatches `should_change_focus_to_new_tab=false` (`data.rs`, `zellij_exports.rs::new_tab_unfocused`, shim `new_tab_unfocused`).
  - `PluginCommand::NewTiledPaneInTab { tab_position }` — opens the pane in an explicitly addressed tab (`zellij_exports.rs::new_tiled_pane_in_tab`, shim `new_tiled_pane_in_tab`).
- Cursor passthrough on embedded viewports: `PaneContents.cursor` (`data.rs`) populated by `Grid::visible_cursor_in_viewport` (`panes/grid.rs`). Both ANSI and plain `pane_contents` paths fill it.

---

## Feature 3 — Mobile Welcome / Session Picker - DONE

**What the user sees.** When the welcome screen comes up on a small viewport, the desktop layout (banner, decorative boundaries, 90-col centred block) is replaced with a vertically stacked, tap-friendly version: search/name input, unified active+resurrectable session list, rename/error/kill-all overlays — all addressable by tap on the row.

**Implementation.**
- `default-plugins/session-manager/src/ui/mobile_welcome.rs` (421 lines): `MobileClickTarget` table + the alternate render path.
- `default-plugins/session-manager/src/ui/mod.rs` — module wiring.
- `default-plugins/session-manager/src/main.rs`: branch at top of `render`, `Event::Mouse` subscription, `mobile_viewport_click` pipe-message handler (so taps inside the embedded session-manager viewport reach the welcome screen), web-client re-refresh on `ModeUpdate` so web-forbidden sessions drop out immediately.

---

## Feature 4 — Native OS Soft Keyboard - DONE

**What the user sees.** Tapping the terminal pops up the OS keyboard. Every keystroke arrives correctly — including on Android (where xterm.js's textarea normally returns `keyCode 229` / "Unidentified") and through autocorrect, predictive text, GBoard, SwiftKey, Samsung Keyboard, iOS Safari, Firefox Android, and in-app WebViews. A one-row "modifier bar" (Ctrl/Alt/Esc/Tab/`~` etc.) appears in lockstep with the OS keyboard and disappears when the keyboard is dismissed — including external dismissals like the Android back button. The mobile plugin can also show/hide the keyboard programmatically (e.g. on first load, or in response to its own ⌨ button).

**Implementation.**
- Browser-side capture: `installSoftKeyboardCapture` in `zellij-client/assets/input.js` — a hidden `<input type="password">` inside a closed shadow root, value-diff to a sentinel-backed capture path. Plus `suppressSoftKeyboardOnTouch` (sets `inputmode="none"` on xterm.js's textarea so taps don't summon the keyboard there).
- Window-level focus listeners (click / touchend / pointerdown) re-focus the capture on every gesture so the OS keyboard re-appears.
- Programmatic show/hide: `PluginCommand::SetSoftKeyboard(bool)` → `ScreenInstruction::SetSoftKeyboard` → `ServerToClientMsg::SetSoftKeyboard` → `WebServerToWebClientControlMessage::SetSoftKeyboard` → focus/blur the capture (`shim.rs`, `zellij_exports.rs::set_soft_keyboard`, `screen.rs` handler, `server_listener.rs`, `websockets.js`).
- Terminal-client safety: `ClientInstruction::from(ServerToClientMsg)` swallows `SetSoftKeyboard` on terminal clients in `zellij-client/src/lib.rs`; the remote-CLI control-channel branch in the same file ignores `WebServerToWebClientControlMessage::SetSoftKeyboard`.
- Visibility observation: `visualViewport.height` watcher in `input.js` → `WebClientToWebServerControlMessage::SoftKeyboardVisibilityChanged` → `ClientToServerMsg::SoftKeyboardVisibilityChanged` → `Event::SoftKeyboardVisibilityChanged(bool)` to subscribed plugins (`route.rs`).
- Plugin permission classification for the new event in `zellij-server/src/plugins/wasm_bridge.rs::check_event_permission`.
- `ScreenContext::SetSoftKeyboard` entry in `zellij-utils/src/errors.rs`.
- Modifier bar UI: `default-plugins/mobile/src/modifier_bar/{mod,controller,layout,modifiers,render}.rs` plus the key-serialization helpers in `default-plugins/mobile/src/keys.rs`.

---

## Feature 5 — Touch Gestures - DONE

**What the user sees.**
- Swipe vertically to scroll the focused pane.
- Swipe horizontally to pan the embedded mobile viewport.
- Tap to left-click.
- Long-press (~500 ms, no motion) to right-click.
- Two-finger tap to toggle the OS keyboard.

**Implementation.**
- Browser-side gesture engine: the touchstart/touchmove/touchend block in `zellij-client/assets/input.js` (~600 lines) — tracks origin, motion threshold (16 px slop), long-press timer (500 ms), two-finger-tap window (600 ms). Emits SGR mouse sequences with 1-based coords.
- Horizontal pan plumbed end-to-end as SGR buttons 66/67:
  - Wire format: `MouseEvent.wheel_left` / `wheel_right` (`input/mouse.rs` — touches every constructor), protobuf field (`common_types.proto:1017`).
  - Termwiz decode: `zellij-client/src/input_handler.rs` (`HORZ_WHEEL` bit mapping).
  - Server dispatch: new `MouseAction::ScrollLeft`/`ScrollRight` + `handle_scrollwheel_horizontal` in `zellij-server/src/tab/mouse_handler.rs` — restricted to plugin panes (terminal panes have no horizontal scrollback).
  - Pane trait: default no-op `Pane::scroll_left`/`scroll_right` (`tab/mod.rs`); `PluginPane` forwards as `Event::Mouse(Mouse::ScrollLeft/Right)` (`plugin_pane.rs`); new `Mouse::ScrollLeft/Right` variants (`data.rs`).
  - Only consumer today is the mobile plugin's pan handler.

---

## Feature 6 — Pinch-to-Zoom & Mobile-Tuned Font Sizing - DONE

**What the user sees.** On first connection from a phone the terminal font is automatically sized so a sensible number of rows (~25) is visible. The user can pinch to zoom in or out on the terminal at any time — a smooth snapshot overlays the canvas during the gesture, then the grid re-flows at the new font size when fingers lift. Pinch-zoom never accidentally evicts the user from mobile mode (or pulls them into it), even though the cell grid changes. The pinched font is ephemeral — a reload restores the default. A `font_size` config knob is available for users who want a fixed size regardless of viewport.

**Implementation.**
- Config: `WebClientConfig::font_size` (`input/web_client.rs`) + KDL parser/serializer addition in `zellij-utils/src/kdl/mod.rs`, `SetConfigPayload.font_size` (`control_message.rs`), Options protobuf field (`common_types.proto`).
- Adaptive default walk: `zellij-client/assets/websockets.js` — `NATURAL_MIN_TOTAL_ROWS`, `MOBILE_LEGIBLE_FLOOR_PX`, `MOBILE_ADAPTIVE_MAX_ITERATIONS` constants and the iterative font-size search.
- Pinch gesture engine and snapshot overlay: `input.js` (pinch_initial_distance, pinch_active, the canvas snapshot path) + `preserveDrawingBuffer` shim in `zellij-client/assets/terminal.js` (xterm.js's WebGL canvas otherwise returns blank pixels under `drawImage`).
- Apply/clamp helpers: `applyFontSize` / `clampFontSize` / `MIN_FONT_SIZE_PX` / `MAX_FONT_SIZE_PX` (`terminal.js`); `zellij-client/assets/index.js` threads `fitAddon` into `setupInputHandlers` so the pinch handler can re-flow the grid.
- Mobile-mode safety: `ResizeCause::{Viewport, RenderingPreference}` discriminator (`ipc.rs`, `client_to_server.proto`). Pinch dispatches `TerminalResizeRendering` → server still re-lays the grid but skips `ReevaluateMobileMode` (`route.rs` arm) — without this, every pinch would risk flipping the user between mobile and desktop.
- iOS Safari URL-bar fix: `100dvh`/`100dvw` upgrade with `100vh` fallback (`assets/style.css`).
- Cursor-inactive-style sync (the soft-keyboard capture takes focus on every tap, so xterm.js renders the inactive cursor style; the sync makes that look right post-`SetConfig`).

---

## Feature 7 — Fit Mode - DONE

**What the user sees.** Tapping the "Fit" button in the mobile UI fullscreens the currently-focused pane and resizes its tab so the pane exactly fills the area the mobile plugin reserves for its embedded viewport. The Fit state is per-client: another desktop user on the same session still sees their normal sized tabs. Disconnecting or rotating tears the fit down cleanly — fullscreen is reverted only if Fit was the thing that turned it on.

**Implementation.**
- Plugin commands: `EnterFitMode { tab_id, pane_id, size }`, `ExitFitMode`, `UpdateFitSize { tab_id, size }` (`data.rs`, shim, protobuf). Sent by the mobile plugin on tap, on subsequent rotations, and on Fit-off respectively.
- Server-side state: `FitState { owning_client, pane_id, size, was_fullscreen_before }` and `Screen::fit_states: HashMap<tab_id, FitState>` (`screen.rs:1556-1583`). Last-writer-wins on collision; `UpdateFitSize` reclaims a displaced entry.
- Screen handlers: `EnterFitMode` / `ExitFitMode` / `UpdateFitSize` instructions, `recompute_tab_size` consults `fit_states` for the override.
- `ScreenContext` enum entries: `EnterFitMode`, `ExitFitMode`, `UpdateFitSize` (`zellij-utils/src/errors.rs`).
- `Size` now derives `Eq` (`zellij-utils/src/pane_size.rs`) — consumed by `FitState`'s derive chain.
- Disconnect cleanup: in `remove_client` (`screen.rs:3864-3880`) — every entry owned by the leaving client reverts fullscreen (if Fit toggled it on) and recomputes the tab.
- Tab API: `Tab::fullscreen_pane_id()` accessor (`tab/mod.rs`) so the handler can capture pre-fit state.
- UI: the Fit button in `default-plugins/mobile/src/render.rs` + the fit-update flush in `default-plugins/mobile/src/main.rs::update`.

---

## Feature 8 — Co-Presence via Shadow Focus

**What the user sees.** A desktop colleague attached to the same session sees a focus marker on whichever pane the mobile user is currently looking at — even though the mobile user is technically alone in their private mobile tab. Real input (the mobile user's typing) still goes to the right pane, and the desktop user's keystrokes are unaffected. No CSI focus-tracking sequences are written to the affected pane's terminal (so its program does not falsely believe it has been foregrounded twice).

**Implementation.**
- Core data structure: `ActivePanes::shadow_clients: HashSet<ClientId>` (`panes/active_panes.rs`) with `insert_silent` / `remove_silent` / `is_shadow_client` / `has_shadow_focus_on` / `iter_shadow_clients`.
- Render filters widened in both pane containers so shadow-marker clients render their focus indicator even though they are not in the tab's `connected_clients` (`tiled_panes/mod.rs:1030-1046`, `floating_panes/mod.rs:472-484`). The marker distinguishes intentional shadow focus from incidental `active_panes` entries (e.g. fake CLI client ids).
- Tab-level helpers: `Tab::set_shadow_focus` / `clear_shadow_focus` / `shadow_focus_clients` / `has_shadow_focus_on` (`tab/mod.rs:2704-2754`). Close-pane and move-pane code paths handle shadow clients silently (no CSI writes).
- Plugin entry point: `PluginCommand::SetMobileFocusedPane(PaneId)` → `ScreenInstruction::SetMobileFocusedPane` → `Screen::set_mobile_focused_pane`. The handler resolves the tab containing `pane_id`, clears any prior shadow entry the client had elsewhere, and applies the new marker. Idempotent against `TabUpdate` → sync → `TabUpdate` loops via `has_shadow_focus_on`.
- `ScreenContext::SetMobileFocusedPane` entry in `zellij-utils/src/errors.rs`.
- Cleanup on detach / mode-exit: `clear_mobile_shadow_focus` invoked in `remove_client` and `exit_mobile_mode`.

---

## Cross-Cutting

- **Tests** (`zellij-server/src/unit/screen_tests.rs` +958 lines, `zellij-server/src/tab/unit/tab_tests.rs` +473 lines, `tab_integration_tests.rs`, IPC roundtrip + socket tests, client-side `terminal_loop_tests.rs` + `web_client_tests.rs`) cover features 1, 7, 8 in particular.
- **Generated protobuf** in `zellij-utils/assets/prost/` and `zellij-utils/assets/prost_ipc/` plus `zellij-utils/src/ipc/protobuf_conversion.rs` mirror the proto additions across features 1, 4, 5, 6, 7, 8.
- **Snapshot files** under `zellij-utils/src/.../snapshots/` track the KDL config additions (feature 1 + feature 6).
- **`ResizeCause`** is shared infrastructure between feature 1 (must re-evaluate on real viewport changes) and feature 6 (must *not* re-evaluate on pinch). It is plumbing, not a feature.
- **Shadow focus** (feature 8) and **NewTabUnfocused / NewTiledPaneInTab** (feature 2) are both consequences of one architectural decision: the mobile client lives in its own per-client plugin tab and must not be moved off it. Without that decision, neither would be needed.

---

## Suggested Review Order

Feature 1 (private tab + auto-routing) is the structural keystone — every other feature presupposes it. A natural order:

1. **Feature 1** — secret-tab mechanic + auto-routing.
2. **Feature 8** — shadow focus (foundational for the mobile plugin's view of other panes).
3. **Feature 2** — the mobile UI plugin itself.
4. **Feature 7** — fit mode (depends on 1 + 2).
5. **Feature 4** — soft keyboard integration.
6. **Feature 5** — touch gestures.
7. **Feature 6** — pinch-zoom + font sizing.
8. **Feature 3** — mobile welcome screen.
