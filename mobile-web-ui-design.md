# Web-Native Mobile Interface — Design Document

## Status

Design approved. This document is a specification intended to be handed to an
implementing agent to produce an implementation plan. It describes the desired
behavior, the architecture it replaces, the relevant existing code seams, and
the design decisions that have been made. It does not prescribe a step-by-step
code change list; that is the implementer's task.

## Motivation

Zellij currently provides a "mobile interface" for small viewports (notably
phones attaching over the web client). The current implementation is an
in-terminal WASM plugin (`default-plugins/mobile/`) that mirrors a selected
pane's rendered ANSI, is hosted inside a per-client hidden ("secret") tab, and
is triggered by server-side size-threshold detection.

This approach is brittle and invasive:

- It re-serializes and reprints pane ANSI, disables autowrap, and manually pans
  a mirrored viewport. This is fragile for wide content, mouse-heavy TUIs, and
  alternate-screen applications, and duplicates rendering work.
- It required invasive core coupling: a hidden-tab mechanism (`Tab::visible_to`),
  phantom-tab accounting (`mobile_tab_count`) threaded through tab numbering,
  `TabUpdate` filtering, shadow focus, and mirror render routing.
- Its touch targets are hard to hit. The hamburger is a single-cell glyph,
  compensated for with a two-tier tight/slop click-distance heuristic
  (`default-plugins/mobile/src/frame.rs`, `click.rs`).
- It can pop up unexpectedly on startup when the terminal is briefly small.

The web client already renders panes in the browser with xterm.js. Therefore a
web-native mobile UI can render navigation chrome as DOM elements around the
existing terminal grid, eliminating the mirroring, hidden-tab, and slop-targeting
machinery for web clients entirely.

This document specifies that web-native mobile interface. The scope is web
clients only.

The original plan retained the plugin for native terminal clients behind a new
config value. That has since been superseded: the plugin is removed entirely in
Phase 5. It never shipped in a tagged release, and web-client routing into it is
already unconditionally disabled, so the web-native UI becomes the sole mobile
interface. See "Plugin Bypass — Superseded by Plugin Removal" and "Phase 5 —
Plugin Removal and Activation Policy" below.

## Existing Architecture Reference

The implementing agent should treat the following as the ground-truth seams. All
file:line references were accurate at the time of writing and should be
re-verified.

### Web client / web server

- The web server is not a separate crate; it lives inside `zellij-client`, under
  `zellij-client/src/web_client/`.
- Frontend assets live in `zellij-client/assets/` and are compiled into the
  binary via `include_dir` (`zellij-client/src/web_client/http_handlers.rs:29`)
  and served from memory. There is no frontend build step.
- The frontend is framework-less ES modules plus xterm.js (with WebGL, fit,
  clipboard, and web-links addons). Entry: `zellij-client/assets/index.html`,
  JS entry `zellij-client/assets/index.js`.
- The only content DOM element is `<div id="terminal">`
  (`zellij-client/assets/index.html:27`), which hosts the xterm.js canvas.
- The only existing DOM overlay chrome is `zellij-client/assets/modals.js` (auth,
  error, and reconnection modals), injected as a non-module script. It
  establishes the injection pattern (inject `<style>` + elements into
  `document.body`, no framework) that new chrome should follow.

### Rendering and transport

- The browser is a dumb terminal emulator. Zellij composites all panes, the tab
  bar, the status bar, and frames server-side into a single ANSI stream. The
  browser has no structured tab/pane model; it only receives ANSI.
- Two WebSockets per client:
  - Terminal WS (`/ws/terminal`) — raw ANSI server→browser; raw input bytes
    browser→server. Server handler `handle_ws_terminal`
    (`zellij-client/src/web_client/websocket_handlers.rs:159-341`). Browser
    render path `zellij-client/assets/websockets.js` (`term.write(data)`).
  - Control WS (`/ws/control`) — JSON control messages. Wire enums in
    `zellij-client/src/web_client/control_message.rs`:
    - Browser→server: `WebClientToWebServerControlMessage` (payload enum:
      `TerminalResize`, `TerminalResizeRendering`, `TerminalSizeSettled`,
      `TerminalMetrics`, `SoftKeyboardVisibilityChanged`).
    - Server→browser: `WebServerToWebClientControlMessage` (`SetConfig`,
      `QueryTerminalSize`, `Log`, `LogError`, `SwitchedSession`,
      `SetSoftKeyboard`).
  - Control-message translation to internal IPC:
    `zellij-client/src/web_client/websocket_handlers.rs:77-104`. Browser-side
    control handling: `zellij-client/assets/websockets.js` (`startWsControl`).
- `zellij-client/src/web_client/server_listener.rs` is the per-client bridge to
  the internal Zellij IPC server (Unix socket).

### Input

- Wired in `zellij-client/assets/input.js`. Keyboard via xterm.js
  `term.onData`/`onBinary`; mouse via `mouse.js` (synthesizes SGR mouse-motion
  reports); touch gestures via `touch.js` (tap→click, long-press→right-click,
  swipe→wheel, two-finger tap→toggle soft keyboard, pinch→font zoom via
  `pinch.js`); soft keyboard capture via `soft-keyboard.js` (hidden input in a
  closed shadow root; server can force it via `SetSoftKeyboard`).
- The key point for this design: xterm.js already handles pane display,
  scrolling, mouse, touch, and soft keyboard natively. The DOM mobile UI does not
  need to reproduce any of the plugin's mirroring, panning, SGR-translation, or
  cursor-mirroring logic.

### Viewport size / resize

- The browser is authoritative about size. It measures via xterm.js FitAddon
  (`fitAddon.proposeDimensions()` → `{rows, cols}`) and reports over the control
  WS via `sendSizeUpdate` (`zellij-client/assets/websockets.js:31-67`), which
  sends a resize message (typed `TerminalResize` / `TerminalResizeRendering` /
  `TerminalSizeSettled`) and a `TerminalMetrics` message (cell/text-area pixel
  dimensions).
- Resize triggers listen on `window resize`, `visualViewport resize`, and a
  custom `zellij:rendering-resize` event; debounced via `requestAnimationFrame`.
  Soft-keyboard show/hide is tracked via `visualViewport.height` deltas and emits
  `SoftKeyboardVisibilityChanged`.
- Server ingestion: resize → `ClientToServerMsg::TerminalResize { new_size, cause }`
  with `ResizeCause::Viewport|RenderingPreference|SizeSettled`; metrics →
  `ClientToServerMsg::TerminalPixelDimensions` consumed by
  `Screen::update_pixel_dimensions` (`zellij-server/src/screen.rs`).

### Web-client identification and current mobile routing

- `is_web_client` is set at attach time by the web server bridge
  (`zellij-client/src/web_client/session_management.rs`) and carried through IPC.
- Server stores it in `Screen::connected_clients: HashMap<ClientId, bool>`
  (`zellij-server/src/screen.rs`), queried via `client_is_web`
  (`zellij-server/src/screen.rs:1653`).
- `mobile_layout` option (`zellij-utils/src/input/options.rs:19-76`), values
  `Web` (default), `Always`, `Never`; thresholds `mobile_threshold_cols`
  (default 60) and `mobile_threshold_rows` (default 30). `should_route_to_mobile`
  (`zellij-utils/src/input/options.rs:48-68`) decides routing; `Web` routes only
  web clients under threshold into the in-terminal mobile plugin.
- The plugin tab is created by `Screen::enter_mobile_mode`
  (`zellij-server/src/screen.rs`), orchestrated by
  `zellij-server/src/mobile_mode.rs` (`MOBILE_PLUGIN_URL = "zellij:mobile"`), with
  a `MobileRenderGate` blanking output during transition. Reevaluation on resize
  in `zellij-server/src/route.rs` and `Screen::reevaluate_mobile_mode`.

## Functional Parity Baseline (from the existing plugin)

The DOM UI must reproduce the plugin's navigation chrome and its actions. It must
NOT reproduce the plugin's pane-mirroring subsystems (xterm.js supersedes them).
The plugin's full behavior is inventoried here as the parity baseline. Sources
are in `default-plugins/mobile/`.

What is dropped (superseded by xterm.js in the browser):

- Viewport mirroring / ANSI reprint (`screens/viewport.rs`, `ansi.rs`).
- Pan/scroll bridge into pane scrollback (`mouse.rs`).
- Click→SGR translation (`mouse.rs`).
- Fat-finger tight/slop targeting (`frame.rs`, `click.rs`) — replaced by
  real pixel-sized CSS touch targets.
- Cursor mirroring and autowrap discipline (`render.rs`, `viewport.rs`).

What must be reproduced as DOM chrome and actions:

1. Top bar (`components/top_bar.rs`): session name (tap → session switcher),
   active pane name/title (tap → pane switcher), hamburger menu button.
2. Hamburger menu (`screens/menu.rs`): entries adapted to this design — see the
   menu spec below.
3. Pane switcher (`screens/panes.rs`): fuzzy search; cards showing pane title (or
   `#id`), tab name, and activity (`[CURRENT PANE]` or `Active <time> ago`, with
   the plugin's time buckets: `just now` under 5s, then `Ns/Nm/Nh/Nd ago`);
   `+ New Tab`, `+ New Pane` (current tab), BACK. Selecting a card focuses that
   pane for this client only (shadow focus, not global focus).
4. Session switcher (`screens/sessions.rs`): fuzzy search; cards showing session
   name and `N tabs, N panes, N clients`; current session excluded; for web
   clients, sessions with `web_clients_allowed == false` are excluded;
   `+ New Session`, BACK. Welcome-takeover mode (title `Hi from Zellij!`, no BACK)
   when the mirrored context is the welcome screen.
5. New session prompt (`screens/new_session.rs`): name input, Cancel / Accept.
6. Modifier / keyboard bar (`components/modifier_bar.rs`): cells ESC, TAB, CTRL,
   ALT, arrows, `-`. CTRL/ALT are one-shot armed modifiers that merge into the
   next key send and then clear. Shown when the soft keyboard is visible.
7. Fuzzy matching, card ordering, and activity formatting run browser-side in JS
   from the state feed (replacing the plugin's skim/`ansi.rs`/`navigation.rs`
   logic).

## Target Design

### Two Orthogonal Controls

The mobile UI is governed by two independent controls.

**Render mode** (what is rendered):

- **Single pane** (default): only the active/focused pane is rendered. The active
  pane is user-selectable via the pane-switcher chrome. This matches the plugin's
  per-client selection model (`default-plugins/mobile/src/workspace.rs:97-106`),
  which deliberately uses per-client selection rather than following other
  clients' global focus.
- **Full render**: everything is rendered exactly as for a normal desktop client
  (all tabs and panes, server-side tab bar and status bar), with a reduced chrome
  escape hatch overlaid so the user can navigate back out.

**Fit toggle** (how the mobile client's own size relates to the render):

- **Enabled**: the render fits the mobile screen — the browser viewport minus
  chrome and minus the soft keyboard. The client's measured `cols`/`rows` drive
  the server layout via the existing size-report path (`sendSizeUpdate`,
  `zellij-client/assets/websockets.js:31-67`). Re-applied on resize and on
  soft-keyboard show/hide.
- **Disabled**: the mobile client's own size is ignored for server layout.
  Content renders at the **connected desktop client's size**, and the mobile
  client pans across that larger render. This mode requires a connected desktop
  (non-web / full-size) client as a reference size. When no desktop client is
  connected, fit-disabled is unavailable (the toggle is disabled in the UI, and
  defaults must not resolve to it).

### Defaults

- Single pane → fit enabled.
- Full render → fit disabled, falling back to enabled when no desktop client is
  connected.

### Behavior Matrix

| Render mode        | Fit enabled                                              | Fit disabled                                                             |
| ------------------ | -------------------------------------------------------- | ----------------------------------------------------------------------- |
| Single pane (dflt) | Active pane fills mobile screen minus chrome (default)   | Active pane at desktop size; mobile pans. Requires a desktop client.     |
| Full render        | Full desktop layout scaled to mobile size minus chrome   | Full desktop layout at desktop size; mobile pans (default). Requires a desktop client. |

### Chrome per Mode

- **Single pane**: full chrome — top bar (session + pane name), hamburger menu,
  modifier/keyboard bar.
- **Full render (escape hatch)**: hamburger menu + modifier/keyboard bar only.
  The top bar is omitted so the full render is unobstructed; navigation is via
  the hamburger. (Decision: full-render escape-hatch chrome = hamburger menu +
  modifier bar; no top bar.)

### Hamburger Menu Spec

The menu contents adapt to context and include:

- Render mode toggle: Single pane ⇄ Full render.
- Fit toggle: enabled/disabled. Disabled (greyed / non-actionable) when no
  desktop client is connected.
- Change Pane (relevant in single-pane mode) → pane switcher.
- Change Session → session switcher.
- Switch to Desktop → client-local: hide the DOM mobile chrome and resume the
  full terminal experience for this browser client.

### Touch Targets

All chrome elements are DOM with CSS pixel dimensions. This removes the plugin's
tight/slop click-distance heuristic entirely; touch targets are real rectangles
sized for fingers and scale correctly across font sizes.

## Sizing / Fit Mechanics

- **Fit enabled**: the browser measures its available terminal area — the
  viewport minus DOM chrome height and minus the soft keyboard — extending the
  current fit computation. It reports `cols`/`rows` to the server through the
  existing `sendSizeUpdate` path. The server lays out to that size. Recompute and
  re-report whenever the chrome layout changes (mode change, menu affecting
  reserved space) or the soft keyboard shows/hides.
- **Fit disabled**: the mobile client stops driving server layout with its own
  size. The server renders at the desktop client's size; the mobile client
  receives the larger render and pans in the browser (see below). Entering
  fit-disabled requires a connected desktop client; if the desktop client
  disconnects while fit-disabled is active, the UI must fall back to fit-enabled.

## Panning (Fit Disabled)

First implementation approach: browser-side panning over xterm.js — the larger
terminal renders in xterm.js and the browser pans across it within a smaller
container (via scroll/transform of the terminal element).

This is explicitly flagged as uncertain. xterm.js sizes its canvas to the
terminal dimensions, so rendering a grid larger than the visible container and
panning it may or may not behave correctly. The implementer must prototype and
validate this before committing.

Fallback if browser-side panning does not work: a server-side windowed pan — the
client reports a pan offset and the server sends only the visible slice. This is
more server work and should only be pursued if the browser-side approach fails.

## Server → Browser State Feed

The browser currently has no structured tab/pane/session state (only ANSI). The
DOM navigation UI requires structured state, delivered as new JSON control-WS
messages (not by scraping ANSI, and not by keeping the plugin alive as a headless
state provider).

Add a `MobileState` variant to `WebServerToWebClientControlMessage`
(`zellij-client/src/web_client/control_message.rs`). It carries, per web client:

- Session name.
- Session list, each with: name, `web_clients_allowed`, tab count, pane count,
  client count. (For filtering and card rendering; the browser excludes the
  current session and any `web_clients_allowed == false`.)
- Tab list with names.
- Panes per tab: id, title, last-activity timestamp.
- Current mode / `is_web_client`.
- Active (shadow-focused) pane for this client.
- Whether a desktop (non-web / full-size) client is connected — gates the fit
  toggle.
- The desktop reference size (cols/rows) — used for fit-disabled rendering and
  panning bounds.

This data maps directly to the events the plugin subscribed to (`ModeUpdate`,
`TabUpdate`, `PaneUpdate`, `SessionUpdate`, and render-report activity/titles);
it is the same data delivered as JSON instead of plugin events. Populate it from
existing server state (`ModeInfo`, tab/session state in
`zellij-server/src/screen.rs`). The desktop-connected flag and reference size
derive from `connected_clients` / `client_is_web`
(`zellij-server/src/screen.rs:1653`). Push it on the same triggers the plugin
reacted to (tab change, pane change, mode change, session change,
activity/title updates).

Handle the message browser-side in the control handler
(`zellij-client/assets/websockets.js`, `startWsControl`) and render/update the
DOM chrome from it.

## Browser → Server Actions

Add a typed control message that maps to `ClientToServerMsg::Action`
(cf. `zellij-client/src/web_client/websocket_handlers.rs:77-104`) for navigation.
The action surface (derived from the plugin's complete click-action enum,
`default-plugins/mobile/src/click.rs`) reduces to:

- Set render mode (single pane vs full render).
- Set fit (enabled / disabled) — gated on desktop-client presence.
- Focus pane (shadow focus for this client).
- New pane in tab; new tab.
- Switch session; new session (name or none).
- Switch to Desktop — client-local (hide DOM chrome); no server action required
  beyond ceasing mobile-specific behavior.
- Key and modifier sends — reuse the existing terminal WS path; not part of the
  new control message.

Fit toggling and single-pane focus map to server-side layout operations: fit
enabled/disabled changes which size drives layout; single-pane mode corresponds
to rendering only the active pane. The implementer should determine the precise
server-side mechanism (e.g. existing focus/fullscreen/single-pane operations)
consistent with the plugin's shadow-focus approach, ensuring the mobile client
does not steal global focus from other clients.

## Plugin Bypass — Superseded by Plugin Removal

This section originally specified a `WebNative` value for
`MobileLayoutConfiguration` that would suppress plugin routing for web clients
while leaving native-terminal mobile intact. That approach has been dropped. See
"Phase 5 — Plugin Removal and Activation Policy" below for the replacement.

Two findings made it obsolete:

1. Plugin routing for web clients is **already** unconditionally suppressed. The
   Phase 2 commit changed `should_route_to_mobile`
   (`zellij-utils/src/input/options.rs:47-71`) to `return false` for any web
   client regardless of variant, and `may_route_web_client_to_mobile` (`:73-75`)
   to a hardcoded `false`. `MobileLayoutConfiguration::Web` is therefore
   behaviorally identical to `Never`, and the plugin is reachable only under
   `Always` for non-web terminal clients. `MobileRenderGate`, `Tab::visible_to`,
   and `mobile_tab_count` are never exercised for web clients today, and the
   startup pop-up race is already gone for them.
2. The plugin is **unreleased**. PR #5241 sits under `## [Unreleased]` in
   `CHANGELOG.md`, and no tag contains its commit. `mobile_layout`,
   `mobile_threshold_cols/rows`, `Action::ToggleMobileMode`, and the plugin
   commands `SetTabFit` / `SetShadowFocus` / `ExitMobileMode` /
   `NewTabUnfocused` / `NewTiledPaneInTab` have never shipped. Removal requires
   no deprecation cycle, config migration, or ABI compatibility shim.

Adding `WebNative` would consequently extend an enum slated for deletion — an
extra variant, `FromStr` arm, KDL parse/serialize pair, round-trip test, and 13
snapshot re-acceptances — in order to gate behavior that is already
unconditional. The in-terminal plugin is removed outright instead.

The remaining live question from this section — how the DOM chrome decides to
activate — is answered client-locally rather than by config; see Phase 5,
Step 10.

## Scope and Non-Goals

- Scope: web clients only.
- The in-terminal mobile plugin is removed entirely (Phase 5). Native-terminal
  mobile behavior is not preserved; it never shipped in a release, and the
  web-native UI is the sole mobile interface going forward.
- Non-goal: reproducing the plugin's pane-mirroring, panning-into-scrollback,
  SGR-click-translation, or slop-targeting logic. xterm.js supersedes these.

## Recommended Build Order

This ordering lets the DOM UI be built and validated alongside the existing
plugin, deferring removal of server-side machinery until parity is confirmed.

1. DOM chrome scaffold wrapping `#terminal` (top bar, hamburger, modifier bar),
   following the `modals.js` injection pattern, added to the module list in
   `zellij-client/assets/index.html` (auto-compiled via `include_dir`, no build
   tooling). Include fit-enabled size reconciliation (measure viewport minus
   chrome/keyboard, feed through `sendSizeUpdate`). This validates single-pane +
   fit-enabled (the default path) with static or stubbed data.
2. State feed: add and populate the `MobileState` control message end-to-end,
   including desktop-client presence and reference size; make the chrome live.
3. Actions: render-mode toggle, fit toggle (gated on desktop-client presence),
   and pane/session navigation (pane switcher, session switcher, new
   session/pane/tab).
4. Fit-disabled panning: prototype browser-side panning over xterm.js; if it
   does not work, fall back to a server-side windowed pan.
5. Plugin removal: delete the in-terminal mobile plugin and its entire
   server-side coupling surface, and settle DOM chrome activation as a
   client-local persisted preference.

## Open Risks

- Browser-side panning over xterm.js (fit-disabled) is unproven and may require a
  server-side windowed-pan fallback.
- Fit-disabled depends on a connected desktop client for its reference size;
  desktop disconnect while fit-disabled must fall back to fit-enabled.
- The precise server-side operations for "single pane render" and "fit
  enabled/disabled size authority" must be chosen to avoid stealing global focus
  from other clients, consistent with the plugin's shadow-focus model.
- All file:line references in this document are point-in-time and must be
  re-verified against the current tree during implementation.

## Implementation Plan

This section is the concrete implementation plan derived from the design above,
following codebase research. All file:line references were accurate at research
time and must be re-verified during implementation.

### Confirmed Design Decisions

| Decision | Resolution | Rationale / precedent |
| --- | --- | --- |
| Single-pane render | Global fullscreen of the active pane | Matches existing plugin `ToggleFit` parity (`zellij-server/src/mobile_mode.rs:229-260`), which already fullscreens the real pane tab-globally. |
| Focus model | Real per-client focus via `Action::FocusPaneByPaneId` | Per-client focus is already per-client (`zellij-server/src/panes/active_panes.rs:6-12`); the shadow-focus constraint existed only because the plugin's real focus was pinned to the plugin pane. |
| Fit enabled | Mobile size participates in the standard per-tab min computation (`zellij-server/src/screen.rs:2408-2426`) | Existing behavior for any small client; no new machinery. |
| Fit disabled | Mobile client excluded from the min-size loop (`zellij-server/src/screen.rs:2410-2417`); browser pans | Localized change; the fit-override branch (`zellij-server/src/screen.rs:2388-2406`) is precedent for bypassing client sizes. |
| Session switching | Browser URL navigation (`${baseUrl}/${name}`) | Existing `SwitchedSession` handler already navigates (`zellij-client/assets/websockets.js:285-288`); no server action needed. |

### Phase 1 — DOM Chrome Scaffold (frontend only, stubbed data) - IMPLEMENTED

New file `zellij-client/assets/mobile-ui.js` (ES module) plus registration in the
module list (`zellij-client/assets/index.html:30-44`, before `websockets.js`).
Assets are auto-embedded via `include_dir` (`zellij-client/src/web_client/http_handlers.rs:29`);
no build tooling.

Structure (following the `modals.js` injection pattern — idempotent
`<style id="...">` into head, elements into `document.body`;
`zellij-client/assets/modals.js:1-338`):

- Top bar: session-name button, pane-name button (fallback `#<id>` / `—`),
  hamburger button.
- Hamburger menu panel: Render mode toggle, Fit toggle (with disabled state),
  Change Pane, Change Session, Switch to Desktop.
- Modifier bar: ESC, TAB, CTRL, ALT, ←, ↓, ↑, →, `-`. CTRL/ALT one-shot arming
  (armed visual state; merged into the next send, then cleared) reproducing
  `default-plugins/mobile/src/components/modifier_bar.rs:79-114`. Bytes are sent
  over the terminal WS send function (the same channel as xterm `onData`);
  serialization: ESC `\x1b`, TAB `\x09`, arrows CSI `A-D`, ctrl→control byte,
  alt→ESC prefix, kitty encoding reused from `zellij-client/assets/keyboard.js`
  where the negotiated protocol requires it (armed modifiers must also merge into
  hardware keys, cf. plugin `default-plugins/mobile/src/input.rs:13-22`).
- Full-screen overlays: pane switcher, session switcher (cards, fuzzy search,
  footer buttons, BACK), new-session prompt (name input, Cancel/Accept). Fuzzy
  matching is implemented in JS (subsequence scoring approximating SkimMatcherV2;
  score-desc, name-asc tiebreak; matched-index highlighting). Activity buckets
  are ported from `default-plugins/mobile/src/ansi.rs:9-26` (`just now` <5s, then
  `Ns/Nm/Nh/Nd ago`).

Chrome-per-mode: single-pane → top bar + hamburger + modifier bar; full render →
hamburger + modifier bar only.

Activation: mobile viewport detection extracted from
`zellij-client/assets/websockets.js:226-230` into `utils.js`. As implemented, a
temporary `?mobile=1|0|auto` query-param / localStorage override enables
development, and a `setServerFlag` hook was left in place for an intended
server-side gate.

Superseded by Phase 5 Step 10: there is no server flag. `setServerFlag` is dead
code and is deleted, the query-param override is removed, and activation becomes
viewport detection plus a sticky `localStorage["zellij:mobile-ui"]` user
preference.

Size reconciliation: chrome reserves height by sizing `#terminal` via CSS custom
properties (extending the `--dynamic-vh` scheme,
`zellij-client/assets/style.css:17-40`, `zellij-client/assets/websockets.js:314-321`);
after any chrome layout change, a `zellij:rendering-resize`-style event dispatch
drives the existing `scheduleResize` → `fitAddon.proposeDimensions()` →
`sendSizeUpdate` path (`zellij-client/assets/websockets.js:31-67, 351-387`).
Modifier-bar visibility is bound to soft-keyboard visibility: a
`zellij:soft-keyboard-visibility` CustomEvent is emitted from
`setupSoftKeyboardVisibilityTracker` (`zellij-client/assets/websockets.js:389-438`).

**Deliverable / how to test (operator):**

1. Build and run a session with the web server: `zellij web` (or start Zellij and
   enable the web server), then open the served URL in a desktop browser.
2. Append the temporary activation override to the URL (e.g. `?mobile=1`) or set
   the documented localStorage key, and reduce the browser window to a
   phone-sized viewport (or use the browser dev-tools device emulator).
3. Confirm the DOM chrome appears around the terminal: a top bar (session name,
   pane name, hamburger), and a modifier bar.
4. Tap the hamburger; confirm the menu opens with Render mode toggle, Fit toggle,
   Change Pane, Change Session, and Switch to Desktop entries. Tap Change Pane and
   Change Session to confirm the switcher overlays render (populated with stubbed
   data) with working fuzzy-search input, card layout, footer buttons, and BACK.
5. Confirm the terminal grid resizes to fit the area not covered by chrome (no
   content is hidden behind the bars). Show the soft keyboard (focus input); the
   modifier bar appears and the grid reflows; hide it and confirm it reverts.
6. Tap modifier-bar cells: ESC/TAB/arrows/`-` produce the corresponding input in
   the focused shell; tap CTRL then a letter to confirm one-shot arming sends the
   control combination and then clears.

### Phase 2 — `MobileState` Feed (server → browser) - IMPLEMENTED

Wire type: `MobileState { payload }` added to
`WebServerToWebClientControlMessage` (`zellij-client/src/web_client/control_message.rs:29-38`,
internally tagged `type`). The `MobileStatePayload` carries: `session_name`;
`sessions: Vec<{name, web_clients_allowed, tab_count, pane_count, connected_clients}>`;
`tabs: Vec<{position, name, active}>`;
`panes: Vec<{tab_position, pane_id, is_plugin, title, is_floating, geometry, last_activity_secs_ago}>`;
`active_pane` (this client); `desktop_client_connected: bool`;
`desktop_size: Option<{cols, rows}>`.

IPC plumbing (established pattern, `SetSoftKeyboard` reference):

1. `ServerToClientMsg::MobileState { payload }` in `zellij-utils/src/ipc.rs:183-230`.
2. Proto message + oneof entry in
   `zellij-utils/src/client_server_contract/server_to_client.proto`; regenerate
   `zellij-utils/assets/prost_ipc/client_server_contract.rs`; conversions in
   `zellij-utils/src/ipc/protobuf_conversion.rs`.
3. Native client: ignore arm in `zellij-client/src/lib.rs` (~:217).
4. Web bridge: match arm in the `server_listener.rs` receive loop (:145-284) →
   `client_connection_bus.send_control(...)`.
5. Browser: dispatch case in `startWsControl`
   (`zellij-client/assets/websockets.js:195-293`) → `mobile-ui.js` update entry
   point.

Population: `Screen::report_mobile_state()` invoked from
`log_and_report_session_state` (`zellij-server/src/screen.rs:3860-3953`),
inheriting the identical trigger set as `TabUpdate`/`PaneUpdate`; sent only to
connected web clients (from `connected_clients`, `zellij-server/src/screen.rs:1424`)
when web-native is active, with change-diffing to suppress redundant pushes.
Sources:

- Tabs/panes: same data as `generate_and_report_tab_state` / `pane_infos()`
  (`zellij-server/src/screen.rs:3643-3787`).
- Sessions: `peer_sessions_cache` (`zellij-server/src/screen.rs:1443`). Cache
  freshness: a browser→server `RequestSessionList` control action triggers
  `scan_session_list_default_dirs` + `ScreenInstruction::UpdateSessionInfos`,
  mirroring `zellij-server/src/plugins/zellij_exports.rs:4352-4388` (sent when the
  session switcher opens). Filtering (current session,
  `web_clients_allowed == false`, welcome sessions) is done browser-side per the
  design.
- Desktop presence/size: `connected_clients` (is-web flags) + `client_sizes`
  (`zellij-server/src/screen.rs:1427`).
- Pane activity: new `Screen.pane_last_activity: HashMap<PaneId, Instant>`,
  stamped from the drained pane-render-report diff in `Screen::render`
  (`zellij-server/src/screen.rs:2915-2925`; drain gating extended to run when
  web-native mobile clients are connected). Fallback if the drain cost is
  prohibitive: pane `active_at` (focus recency) as a degraded approximation.

**Deliverable / how to test (operator):**

1. With the Phase 1 chrome active, create several tabs and panes in the session
   and give panes distinct titles.
2. Confirm the top bar shows the real session name and the real active pane title
   (or `#<id>`), updating live as focus/titles change.
3. Open the pane switcher; confirm real panes are listed with real tab names and
   live activity labels (`just now`, `Ns/Nm/Nh/Nd ago`) that advance over time as
   panes are used.
4. Start a second Zellij session on the same machine; open the session switcher
   and confirm the other session appears with correct `N tabs, N panes, N clients`
   counts, that the current session is excluded, and that sessions with web
   clients disallowed are excluded.
5. Attach a desktop (non-web) client; confirm the Fit toggle becomes actionable
   (no longer greyed). Detach it; confirm the Fit toggle reverts to disabled/greyed.

### Phase 3 — Browser → Server Actions - IMPLEMENTED

Wire type: new `WebClientToWebServerControlMessagePayload` variants
(`zellij-client/src/web_client/control_message.rs:11-19`), translated in
`send_message_to_server` (`zellij-client/src/web_client/websocket_handlers.rs:66-105`):

| Browser payload | Translation |
| --- | --- |
| `FocusPane { pane_id, is_plugin }` | `ClientToServerMsg::Action(Action::FocusPaneByPaneId)` (`zellij-utils/src/input/actions.rs:642-644`; routed at `zellij-server/src/route.rs:388-398`) |
| `NewPaneInTab { tab_position }` | `Action::NewTiledPane { tab_id: Some(..), .. }` (`zellij-utils/src/input/actions.rs:267-275`) |
| `NewTab` | `Action::NewTab` (`zellij-utils/src/input/actions.rs:318-328`) |
| `RequestSessionList` | new `ClientToServerMsg` (Phase 2) |
| `SetMobileRenderPreferences { single_pane, fit }` | new `ClientToServerMsg` variant (proto + conversions) → new `ScreenInstruction` |

Session switch / new session: the browser navigates to
`${baseUrl}/${encodeURIComponent(name)}` (a new unnamed session → base URL); no
server action. Switch to Desktop: client-local chrome teardown + re-fit; no server
message.

Server handling of `SetMobileRenderPreferences` (new `Screen` state
`mobile_web_prefs: HashMap<ClientId, {single_pane, fit}>`):

- Fit disabled: insert into a `size_excluded_clients` set consulted in the
  `recompute_tab_size` min loop (`zellij-server/src/screen.rs:2410-2417`);
  reject/ignore when no desktop client is connected; on desktop disconnect
  (`remove_client`, `zellij-server/src/screen.rs:3561-3606`) auto-revert and push
  an updated `MobileState` so the UI falls back to fit-enabled.
- Single-pane enabled: fullscreen the client's active pane via the existing
  `ToggleFullscreenWithPaneId` machinery (`zellij-server/src/screen.rs:10447`,
  `zellij-server/src/tab/tiled_panes/mod.rs:2706-2761`); on focus change while
  single-pane, move the fullscreen; on disable, unset fullscreen. Reconciliation
  with desktop-initiated fullscreen changes treats fullscreen state as tab-global
  truth and reflects it in `MobileState`.

**Deliverable / how to test (operator):**

1. With the Phase 2 feed live, open the pane switcher and select a pane; confirm
   that pane becomes the active/rendered pane for the mobile client without moving
   focus for any attached desktop client.
2. Use `+ New Pane` and `+ New Tab` from the switcher; confirm a new pane appears
   in the chosen tab and a new tab is created, respectively, and the mobile client
   focuses the newly created pane.
3. Open the session switcher and select another session; confirm the browser
   navigates to that session. Use `+ New Session` with and without a name; confirm
   the appropriate session is created/attached.
4. In the hamburger menu, toggle Render mode to Single pane; confirm only the
   active pane is rendered (fullscreen). Toggle Fit; with a desktop client present,
   confirm fit-disabled ignores the mobile size (content stays at desktop size) and
   fit-enabled makes content fit the mobile screen.
5. With fit-disabled active, disconnect the desktop client; confirm the UI
   automatically falls back to fit-enabled.

### Phase 4 — Fit-Disabled Panning (prototype-first) - IMPLEMENTED

Browser-side panning over xterm.js has been implemented. New module
`zellij-client/assets/mobile-pan.js` exposes `window.__zjMobilePan`
(`setActive`, `panBy`, `recompute`, `isActive`, `getOffset`) and pans by applying
a clamped `translate3d(-panX, -panY, 0)` CSS transform to `term.element`; bounds
are derived from the terminal canvas pixel size (cols/rows × cell dims) minus the
`#terminal` container size. It is registered in `zellij-client/assets/index.html`
before `touch.js` and initialized (idempotently) from `input.js`
`setupInputHandlers`.

Activation is bound to the existing pinned state: `mobile-ui.js` `syncPan()` sets
`__zjMobilePan.setActive(getRenderSizing().pinned)` and is invoked from
`reconcileSize()` (both active and inactive branches) and `setData()`, so pan
engages exactly when fit is disabled with a valid desktop reference size and
disengages (resetting offset to zero) otherwise, including on
`switchToDesktop` and desktop-disconnect fallback.

Touch integration (`touch.js`): when `__zjMobilePan.isActive()`, single-finger
`touchmove` pans via `panBy(-delta_x, -delta_y)` (content follows finger) instead
of synthesizing wheel events; `reportCoords` offsets `clientX/clientY` by the pan
offset before `getMouseReportCoords`, so tap/long-press SGR targeting is corrected
for the pan. `mouse.js` applies the same offset to synthesized mousemove reports.
`#terminal` `touch-action` is switched to `none` via a `body.zj-mobile-panning`
rule while panning so two-axis drag is not consumed by the browser.

Size-report suppression gaps closed in `websockets.js`: the `wsControl.onopen`
report and the `SetConfig` "Settled" report are now guarded by the pinned check
(resizing the terminal to the desktop reference size without reporting the mobile
size); the pinned `QueryTerminalSize` branch now reports the desktop reference
size. The pinned `resizeTerminal` branch triggers `__zjMobilePan.recompute()`.

The server-side windowed-pan fallback (item 3 below) was not required.

#### Original plan (retained for reference)

1. Prototype (throwaway validation): with size reporting suppressed (guard in
   `sendSizeUpdate` / `scheduleResize`,
   `zellij-client/assets/websockets.js:31-67, 351-371`; `QueryTerminalSize`
   responses report the desktop reference size), call
   `term.resize(desktopCols, desktopRows)` from `MobileState.desktop_size`, clip
   the xterm element inside `#terminal`, and pan via CSS transform. Validate:
   canvas rendering beyond the container, WebGL addon behavior, cursor/selection
   correctness, and scrollback.
2. Touch integration in `touch.js` (`zellij-client/assets/touch.js:5-266`): in
   fit-disabled, single-finger drag pans the viewport; tap/long-press still
   forward (coordinates offset by the pan before SGR synthesis in
   `zellij-client/assets/touch.js:38-43` and `zellij-client/assets/mouse.js:8`).
3. Fallback (only if the prototype fails): server-side windowed pan — the browser
   reports a pan offset via a new control payload; the server serializes a
   per-client character-chunk window. Scoping is deferred until the prototype
   outcome.

**Deliverable / how to test (operator):**

1. Attach a desktop client to establish a large reference size; on the mobile
   client set Render mode to Full render with Fit disabled.
2. Confirm the full desktop layout renders at the desktop size and the mobile
   client can pan across it via single-finger drag (or drag/scroll on desktop
   emulation), reaching all four edges.
3. Confirm a tap at a panned location still lands on the correct cell (input
   targeting accounts for the pan offset) and that the cursor and any text
   selection render at the correct positions.
4. If the browser-side prototype is abandoned, repeat the above against the
   server-side windowed-pan fallback and confirm equivalent behavior.

### Phase 5 — Plugin Removal and Activation Policy

Supersedes the original "Plugin Bypass (`WebNative`)" phase; the rationale for
the change is recorded in the "Plugin Bypass — Superseded by Plugin Removal"
section above.

Nothing is added to `MobileLayoutConfiguration`. The in-terminal mobile plugin
and its entire server-side coupling surface are deleted, and DOM chrome
activation becomes a client-local persisted preference with no server surface at
all.

#### Settled Decisions

| Question | Resolution | Rationale |
| --- | --- | --- |
| Chrome activation | No config. Browser viewport detection plus a sticky, persisted user toggle. | The `isMobileViewport()` heuristic already requires a coarse pointer, so a narrow desktop window does not trigger it. "Switch to Desktop" supplies the escape hatch. Config surface is not added before a demonstrated need. |
| Sequencing | Single combined phase (removal + activation). | The activation policy is only meaningful once `mobile_layout` is gone; splitting would leave dead config in the tree between phases. |
| Plugin commands | Retain `NewTabUnfocused` and `NewTiledPaneInTab`; drop `SetTabFit`, `SetShadowFocus`, `ExitMobileMode`, `EventType::PaneRenderReportWithAnsi`, and `Action::ToggleMobileMode`. | The two retained commands are generically useful plugin APIs incidentally consumed only by the mobile plugin. The rest are backed by plugin-only server machinery that is being deleted. |

#### Step 1 — Relocate web-native state out of `mobile_mode.rs`

`MobileWebPrefs` (`zellij-server/src/mobile_mode.rs:31-46`) belongs to the web UI
and is read at 11 sites in `screen.rs`. Move it to a new
`zellij-server/src/mobile_web.rs`, declared in `zellij-server/src/lib.rs:17`.
`FIT_RESIZE_MAX_ITERS` (`:11`) is consumed only by the plugin fit branch and is
deleted in Step 2.

This is the sole blocker to deleting `mobile_mode.rs` wholesale, so it is
performed first; every later step is then pure deletion.

#### Step 2 — Delete the plugin fit path

Distinct from the web UI's fit mechanism, which stays.

- Delete the `recompute_tab_size` fit-override branch (`screen.rs:2407-2425`) and
  `compute_fit_size` (`:2504`), `set_tab_fit` (`:2508`), `exit_fit_mode`
  (`:2520`), `clear_fit_for_closed_pane` (`:2533`) with their callers.
- Delete `FitOverride` and the `MobileState` fit API (`mobile_mode.rs:215-339`);
  `ScreenInstruction::SetTabFit` (`screen.rs:794-798`, `errors.rs:407`, handler
  `:10189`).
- Delete `PluginCommand::SetTabFit` (`data.rs:3642`, `shim.rs:1718`,
  `zellij_exports.rs:467, 4055-4060, 5610`, proto id 216).
- Delete the ~14 `screen_tests.rs` fit tests and the `setup_mobile_fit` helper
  (`:9998`).

Retained: `recompute_tab_size` lines 2427-2469, `recompute_fit_disabled_tabs`,
`reference_size_for_client`, `has_reference_client`.

#### Step 3 — Delete the render gate

`MobileRenderGate` (`mobile_mode.rs:48-118`);
`ScreenInstruction::{SuppressRenderUntilMobile, MobileSizeSettled, ForceMobileUngate}`
(`screen.rs:577-579`, `errors.rs:336-338`, handlers `:8546-8575`);
`BackgroundJob::MobileGateTimeout` (`background_jobs.rs:77, 106, 113, 627-637`,
`errors.rs:656`); `PluginInstruction::{Hold,Release}MobileRender`
(`plugins/mod.rs:116-117, 244-245, 739-744`, `errors.rs:533-534`,
`wasm_bridge.rs:180, 246, 1231, 1236-1256, 1318`);
`Screen::{try_lift_mobile_gate, force_render_mobile_tab, ungate_clients_for_mobile_plugin}`
(`:1680-1718`) and the gate checks at `:1702, 1713, 1824, 1939, 3221, 6782`.

#### Step 4 — Delete mode entry/exit and routing

`Screen::{enter,exit,toggle,reevaluate}_mobile_mode` (`:1787-1944`),
`is_in_mobile_mode`, `visible_tab_positions_for_client`;
`ScreenInstruction::{Enter,Exit,Toggle}MobileMode` and `ReevaluateMobileMode`
(`:901-910`, handlers `:11084-11115`); `Action::ToggleMobileMode`
(`actions.rs:426`, proto field 140, `protobuf_conversion.rs:1668-1670, 2558`,
KDL `kdl/mod.rs:85, 1267, 1616`, `route.rs:1282-1287`); connect hooks
(`lib.rs:1002-1016, 1115-1123, 1169-1180, 1208-1212, 2276-2289`); the resize hook
(`route.rs:2432-2476`), retaining the `SetMobileRenderPreferences` arm at
`:2738-2744`; `PluginCommand::ExitMobileMode` (proto id 222).

#### Step 5 — Delete shadow focus

`active_panes.rs:6-12, 25, 30-37, 52, 59, 62, 73, 92`;
`tiled_panes/mod.rs:1110, 2838-2852`; `floating_panes/mod.rs:261-274, 475`;
`tab/mod.rs:3755-3784`; `mobile_mode.rs:177-213, 341-345`;
`screen.rs:1776-1785, 1798, 1895, 3829, 11122-11124`;
`PluginCommand::SetShadowFocus` (proto id 219); the ~10 shadow-focus tests at
`tab_tests.rs:16632-16835`.

`TabInfo` carries no shadow field (`data.rs:2250-2280`); shadow clients merge
into `other_focused_clients` at `screen.rs:3899-3903, 3954-3958, 6156-6160`.
Removal is behavioral only and changes no plugin ABI.

#### Step 6 — Delete `visible_to` and `mobile_tab_count`

`Tab::visible_to` (`tab/mod.rs:191, 897`) and its 10 `screen.rs` sites, including
the `TabUpdate` filter (`:3934-3941`), the `remove_client` tab-GC
(`:3820-3825`), `switch_active_tab_name` (`:2190`), and `go_to_tab` (`:2284`).
The `mobile_tab_count` parameter is dropped from `Tab::new` and the default-name
formula (`tab/mod.rs:817-820`) simplifies to `format!("Tab #{}", id + 1)`;
12 test call sites drop the argument.

#### Step 7 — Delete the config surface

`MobileLayoutConfiguration` (`options.rs:19-76`);
`Options::{mobile_layout, mobile_threshold_cols, mobile_threshold_rows}`
(`:404-417`) with merge sites (`:527-529, 582-584, 674-676, 729-731`), the derived
CLI flags, and tests (`:775-950`); KDL parse (`kdl/mod.rs:2896-2928, 2981-2983`)
and serialize (`:4420-4507, 4695-4703`) with test `:7558`; proto
`common_types.proto:1199-1201, 1231-1235` and
`protobuf_conversion.rs:909-919, 937, 1030-1046`.

#### Step 8 — Delete the crate and asset

`default-plugins/mobile/` (22 `.rs` files, 5,159 LOC); the workspace member
(`Cargo.toml:61`); the xtask member (`xtask/src/main.rs:90-92`); `Cargo.lock`;
`zellij-utils/assets/plugins/mobile.wasm` (1.58 MB); `consts.rs:174`;
`plugins.rs:72`; the aliases at
`zellij-utils/assets/config/default.kdl:245` and `example/default.kdl:247`.
Delete `EventType::PaneRenderReportWithAnsi`
(`screen.rs:870, 1222, 3096, 3152-3162, 6361-6400, 10595`;
`wasm_bridge.rs:512, 1333-1430, 2177`; `zellij_exports.rs:869`).

Retained by decision: `PluginCommand::NewTabUnfocused` (220) and
`NewTiledPaneInTab` (221). Retained by hard dependency: `PaneRenderReport`,
`PaneContents`, and `SubscribeToPaneRenders` — consumed by
`zellij-client/src/cli_client.rs:289`.

#### Step 9 — Regenerate and re-accept

Run `cargo x proto` (`xtask/src/flags.rs:45`, `xtask/src/build.rs:183`) to
regenerate `assets/prost`, `assets/prost_ipc`, and `assets/prost_web_server`.
Mark freed plugin-command ids 216, 219, 222, Action field 140, and Options
fields 48-50 as `reserved`. Re-accept the 4 KDL snapshots and the 9
`zellij-utils/src/snapshots/setup_test__*.snap` files.

#### Step 10 — Frontend activation policy

No config; browser detection plus a sticky, persisted user toggle.

- Replace `activationOverride()` (`mobile-ui.js:55-72`): drop the `?mobile=`
  query parameter entirely and read a single key `localStorage["zellij:mobile-ui"]`
  with values `"on"` / `"off"` / absent (auto).
- Delete `serverFlag` and `setServerFlag` (`mobile-ui.js:74-79`) and their entry
  in the `window.__zjMobileUi` export (`:46`). Both are already dead code — no
  caller exists anywhere in `zellij-client/assets/`.
- Fix `switchToDesktop()` (`:527-543`): persist `"off"` rather than calling
  `localStorage.removeItem`. This alone resolves the re-activation bounce, since
  `evaluateActivation()`'s early return (`:90-92`) then holds. Today the key is
  merely removed, so the next `resize`, orientation change, or soft-keyboard
  toggle re-runs `shouldActivate()`, `isMobileViewport()` returns true, and the
  chrome reappears — "Switch to Desktop" is not sticky.
- Add the return affordance: a small fixed-position pill rendered only when the
  stored preference is `"off"` **and** `isMobileViewport()` (`utils.js:37-44`) is
  true. Tapping it restores the preference and calls `evaluateActivation()`. It
  must not appear on genuine desktop browsers.
- Retain `switchToDesktop`'s `SetMobileRenderPreferences { single_pane: false, fit: true }`
  send (`:530-534`); the server must exit single-pane fullscreen.

#### Step 11 — Documentation

Replace `CHANGELOG.md:10` (`feat: mobile UI (#5241)`, unreleased) with an entry
describing the web-native mobile UI.

**Deliverable / how to test (operator):**

1. Attach a phone-sized web client. Confirm the DOM chrome activates from
   viewport detection alone, with no `?mobile=1` query parameter and no config
   setting.
2. Tap Switch to Desktop. Confirm the chrome disappears and **stays** hidden
   across an orientation change, a window resize, and a soft-keyboard show/hide
   cycle. Confirm the return pill is visible and restores the chrome.
3. Open the same URL in a desktop browser. Confirm neither the chrome nor the
   return pill appears.
4. Attach a native terminal client at a small size (with any former
   `mobile_layout` value removed from the config). Confirm no mobile plugin tab
   is created, tab numbering is contiguous, and no startup pop-up occurs.
5. Confirm a config containing `mobile_layout`, `mobile_threshold_cols`, or
   `mobile_threshold_rows` is rejected or ignored per the codebase's handling of
   unknown options, and that no `zellij:mobile` alias remains in the shipped
   default config.

### Validation

- Serde round-trip tests for the new control messages (pattern:
  `zellij-client/src/web_client/control_message.rs:393-402`); protobuf conversion
  tests alongside the existing ones.
- Unit tests: `recompute_tab_size` exclusion semantics (cf.
  `zellij-server/src/unit/screen_tests.rs:9857`) and the fit-disabled
  desktop-disconnect fallback. The `should_route_to_mobile` tests are deleted
  with the function in Phase 5 Step 7.
- Unit runs: `cargo test -p zellij-server -p zellij-utils -p zellij-client`.
- Integration tests are executed exclusively via `cargo x integration-test`.
- `cargo x build` — confirms the wasm plugin set builds without `mobile`.
- Approximately 60 tests are deleted and 13 snapshots re-accepted in Phase 5. A
  clean `cargo test` before and after is required to distinguish intended
  deletions from regressions.
- Manual browser validation per phase (mobile viewport emulation + a real device):
  chrome reflow, soft-keyboard cycles, switcher parity against the plugin, and the
  panning prototype.

### Implementation Risks

- Phase 4 browser-side panning over xterm.js is unproven; the prototype gates the
  approach before commitment (server-side windowed-pan fallback).
- Phase 5 Steps 5-6 touch shared pane and tab containers (`active_panes.rs`, the
  tiled and floating render filters). The `other_focused_clients` merge sites
  must be reduced to real focus without altering desktop multi-client behavior.
- `reference_size_for_client` (`screen.rs:2490-2497`) and `has_reference_client`
  (`:2499`) currently accept *any* other connected client rather than
  specifically a desktop one, contradicting this document's "desktop client"
  language. Pre-existing; flagged, not in scope.
- Single-pane fullscreen is tab-global: desktop viewers of the same tab see the
  fullscreen (accepted; matches plugin `ToggleFit` parity).
- Fit-enabled on a shared tab shrinks it for desktop viewers via min-size
  semantics (standard Zellij multi-client behavior; surfaced in the UI via the fit
  toggle).
- Proto regeneration is performed with `cargo x proto` (`xtask/src/flags.rs:45`,
  `xtask/src/build.rs:183`), which regenerates `assets/prost`,
  `assets/prost_ipc`, and `assets/prost_web_server`.

Recommended execution order is Phases 1→5 as listed. Phases 1–4 are implemented.
Phase 5 is a single combined change (plugin removal plus activation policy) and
is the only remaining work; until it lands, the DOM chrome is reachable only via
the temporary `?mobile=1` override.
