# Mobile web-client startup render flash — handoff

Facts-only handoff for a clean session. Every statement is backed by a code
reference, a log observation from `/tmp/zellij-1000/zellij-log/zellij.log`, or a
testing observation reported during the session. No conclusions about root cause
are asserted; tried changes are listed with their observed result.

## 1. Symptom (as reported during testing)

- Context: a web (browser) client connects with no session name, which starts
  the built-in `welcome` layout; that flow routes the client into the
  `zellij:mobile` plugin (a full-screen plugin pane).
- After the mobile UI settles to its final size, a transient is visible for
  roughly under one second before settling. Reported variants across separate
  reproductions:
  - Content positioned horizontally to the right ("as if I need to pan right to
    see it"), then it snaps/settles to the correct position.
  - A blank screen that only appeared ("popped in") after tapping the screen.
  - A blank white screen for an instant immediately before the off-to-the-right
    frame, then it settled.
- The transient was reported to reproduce **only in `--release` builds**; it was
  **not** reproduced in debug builds.
- The mobile plugin's Sessions screen horizontally centers its content on the
  width it is given: `default-plugins/mobile/src/screens/sessions.rs` (search
  `card_x` / `cols.saturating_sub`).

## 2. Worktree state

- Branch: `mobile-layout`. HEAD = `d515c891` "remove mobile flash on
  startup/attach" (a local, unpushed commit made during the session; the session
  started at `82d01b79`). The relevant code is split between this commit and the
  uncommitted working tree — the effective current state is HEAD + the
  uncommitted diff. Sections §3–§5 describe that effective state.
- Committed in `d515c891` (source files; `git show --stat d515c891`):
  - `default-plugins/mobile/src/main.rs`, `default-plugins/mobile/src/render.rs`
  - `zellij-client/assets/websockets.js`
  - `zellij-client/src/web_client/control_message.rs`,
    `zellij-client/src/web_client/websocket_handlers.rs`
  - `zellij-server/src/background_jobs.rs`, `lib.rs`, `mobile_mode.rs`,
    `route.rs`, `screen.rs`, `unit/screen_tests.rs`
  - `zellij-utils/assets/prost_ipc/client_server_contract.rs`,
    `zellij-utils/src/client_server_contract/client_to_server.proto`,
    `zellij-utils/src/errors.rs`, `zellij-utils/src/input/options.rs`,
    `zellij-utils/src/ipc.rs`, `zellij-utils/src/ipc/protobuf_conversion.rs`
  - (plus rebuilt `.wasm` plugin assets)
- Uncommitted working-tree changes (`git status --porcelain`), source files:
  - `zellij-client/assets/style.css`
  - `zellij-client/assets/terminal.js`
  - `zellij-client/assets/websockets.js`
  - `zellij-client/src/web_client/control_message.rs`
  - `zellij-client/src/web_client/websocket_handlers.rs`
  - `zellij-server/src/mobile_mode.rs`
  - `zellij-server/src/screen.rs`
  - `zellij-utils/assets/plugins/*.wasm` (13 Bin files)
- Untracked: `mobile-render-flash-handoff.md` (this file),
  `mobile-render-offset-investigation.md` (earlier facts handoff), `to_review.md`.
- Build command used: `cargo x build` (builds the wasm plugins, then the
  workspace with profile `dev-opt`). Browser assets are embedded into the
  `zellij-client` binary at compile time via `include_dir!` / `include_str!`
  (`zellij-client/src/web_client/http_handlers.rs`), so asset edits require
  rebuilding/relaunching the web server binary.

## 3. Server-side mobile render gate (current state, in worktree)

Implemented to prevent the server from sending wrong-size content to a gated
client during mobile entry.

- `zellij-server/src/mobile_mode.rs`: `struct MobileRenderGate` with fields
  `awaiting_first_render: HashSet<ClientId>`, `clients_with_settled_size:
  HashSet<ClientId>`, `last_paint_size: HashMap<ClientId, Size>`. Methods:
  `gate`, `is_gated`, `is_empty`, `gated_clients`, `ungate(client, reason)`,
  `record_settled_size`, `record_paint_size`, `try_reveal(client, is_web,
  reported_size)`, `blank_gated_clients(serialized_output)`. `const CLEAR_SCREEN:
  &str = "\u{1b}[2J\u{1b}[H"` (7 bytes).
  - `try_reveal` returns true (and ungates) when: web client AND
    `clients_with_settled_size` contains the client AND `reported_size ==
    last_paint_size`; non-web clients reveal on any recorded paint.
- `zellij-server/src/screen.rs`:
  - `Screen` field `mobile_render_gate: MobileRenderGate`.
  - `ScreenInstruction` variants `SuppressRenderUntilMobile(ClientId)`,
    `MobileSizeSettled(ClientId)`, `ForceMobileUngate(ClientId)` (+ matching
    `ScreenContext` variants in `zellij-utils/src/errors.rs`).
  - `render_to_clients`: after `output.serialize()`, calls
    `self.mobile_render_gate.blank_gated_clients(&mut serialized_output)` which
    replaces each gated client's bytes with `CLEAR_SCREEN`.
  - `PluginBytes` handler: for a non-empty paint on the client's mobile tab,
    records the plugin pane's content size via `record_paint_size`, then calls
    `try_lift_mobile_gate(client_id)`.
  - `try_lift_mobile_gate`: gathers `client_is_web` + `client_sizes`, calls
    `mobile_render_gate.try_reveal(...)`, and on success calls
    `force_render_mobile_tab` (sets `set_should_clear_display_before_rendering`
    + `set_force_render` on the mobile tab).
  - `SuppressRenderUntilMobile` handler: `mobile_render_gate.gate(client_id)` and
    sends `BackgroundJob::MobileGateTimeout(client_id)`.
  - `MobileSizeSettled` handler: `record_settled_size` then `try_lift_mobile_gate`
    + render.
  - `ForceMobileUngate` handler: if gated, ungate + force render + render.
  - `enter_mobile_mode`: creates the mobile tab via `new_tab(... self.size ...)`
    (`Tab::new(..., self.size, ...)`).
- `zellij-server/src/route.rs`: client `TerminalResize { new_size, cause }`
  handler sends `ScreenInstruction::RecomputeTabSize`; on `cause ==
  ResizeCause::Viewport` sends `ReevaluateMobileMode`; on `cause ==
  ResizeCause::SizeSettled` sends `ScreenInstruction::MobileSizeSettled`.
- `zellij-server/src/lib.rs`: `FirstClientConnected` and `AttachClient` compute
  `should_enter_mobile_on_connect(&options, is_web_client, viewport)` (free fn
  near `should_show_startup_tip`); when true, send `SuppressRenderUntilMobile`
  then `EnterMobileMode`.
- `zellij-server/src/background_jobs.rs`: `BackgroundJob::MobileGateTimeout(ClientId)`
  (+ `BackgroundJobContext` variant in errors.rs); handler sleeps
  `MOBILE_GATE_FALLBACK_LIFT_TIMEOUT_MS = 5000` then sends
  `ScreenInstruction::ForceMobileUngate`.
- `self.size` (Screen) note: assigned only at `Screen::new`; the only mutator
  `resize_to_screen` is reached only by `ScreenInstruction::TerminalResize`,
  which is not constructed anywhere in the server (verified by grep in an earlier
  session). Per-client web resizes route through `RecomputeTabSize`.

## 4. Explicit settled-size IPC signal (current state)

- `.proto`: `zellij-utils/src/client_server_contract/client_to_server.proto`
  enum `ResizeCause` has `RESIZE_CAUSE_SIZE_SETTLED = 2` (in addition to
  `RESIZE_CAUSE_VIEWPORT = 0`, `RESIZE_CAUSE_RENDERING_PREFERENCE = 1`).
- Generated prost asset `zellij-utils/assets/prost_ipc/client_server_contract.rs`
  regenerated via `cargo x build` (contains `SizeSettled = 2` and string-name
  mappings). Generated file is not hand-edited.
- `zellij-utils/src/ipc.rs`: `enum ResizeCause { Viewport, RenderingPreference,
  SizeSettled }`. `zellij-utils/src/ipc/protobuf_conversion.rs`: both From
  directions handle `SizeSettled`.
- Browser→web-client-process JSON control message
  (`zellij-client/src/web_client/control_message.rs`):
  `WebClientToWebServerControlMessagePayload::TerminalSizeSettled(Size)` and
  `DebugLog { message: String }`.
- `zellij-client/src/web_client/websocket_handlers.rs`: `TerminalSizeSettled(size)`
  → `ClientToServerMsg::TerminalResize { cause: ResizeCause::SizeSettled }`;
  `DebugLog { message }` → `log::info!("[mobile-gate-client] {message}")` and is
  not forwarded.
- Browser (`zellij-client/assets/websockets.js`): `SetConfig` sends a size update
  with cause `"Settled"`, which `sendSizeUpdate` maps to the
  `TerminalSizeSettled` message type. `RenderingPreference` is still produced by
  the runtime resize path (`scheduleRenderingResize`, `zellij:rendering-resize`
  event).

## 5. Browser startup font-sizing and renderer (current state)

- `zellij-client/assets/terminal.js` `initTerminal`: `new Terminal({ fontFamily:
  "Monospace", allowProposedApi: true, scrollback: 0 })`. Addons loaded at open:
  fit, clipboard, web-links. The **WebGL addon is created but NOT loaded at
  startup**; instead `window.__zjLoadWebglRenderer` is defined to load it once.
  `term.open(...)` then runs on the default DOM renderer.
- `ensurePreserveDrawingBuffer()` overrides `HTMLCanvasElement.prototype.getContext`
  to set `preserveDrawingBuffer: true` for webgl/webgl2 (used by the pinch
  overlay snapshot). Comment references xterm.js issue #5164 re: font preload /
  first-load rendering (`zellij-client/assets/index.html` head).
- `zellij-client/assets/websockets.js` `SetConfig` handler:
  - Sets `term.options.fontFamily`, theme, cursor options.
  - `initialCandidate` = explicit font size, else 24 (mobile viewport), else 12.
  - `applyFontSize(term, fitAddon, initialCandidate)` (one fit).
  - If mobile viewport && no explicit font && `term.rows < NATURAL_MIN_TOTAL_ROWS`
    (25): single computed downscale `finalPx = max(floor(initialCandidate *
    term.rows / 25), MOBILE_LEGIBLE_FLOOR_PX=16)`, applied once if smaller. (This
    replaced the previous iterative loop, which called fit() up to
    `MOBILE_ADAPTIVE_MAX_ITERATIONS = 4` times.)
  - Sets body + `#terminal` background to theme background.
  - Calls `window.__zjLoadWebglRenderer()` (loads WebGL on the now-final,
    still-blank terminal) before sending the settled size.
  - Sends size update with cause `"Settled"`.
- `zellij-client/assets/style.css`: `html, body { background: #000; }` added.
  `#terminal` rule has no opacity/visibility hiding (reverted).
- `zellij-client/assets/index.html`: `<div id="terminal" tabindex="0"></div>`
  (no inline style; reverted).

## 6. Changes tried, in order, and observed result

1. Server render gate (§3): blanks gated clients, reveals on settled+paint-match.
   Result: server logs confirm only `CLEAR_SCREEN` (7-byte) frames are sent to
   the gated client until reveal, then content at the final size. Flash still
   reported.
2. Explicit `TerminalSizeSettled` / `ResizeCause::SizeSettled` (§4). Flash still
   reported.
3. Hide `#terminal` with `visibility: hidden` (CSS) until first real frame, then
   reveal on rAF. One build logged `termVisibility=visible` during SetConfig
   (hide not applied); after also setting it from JS, logged
   `termVisibility=hidden`. Flash still reported.
4. Switch hide to `opacity: 0` + `fitAddon.fit()` + `term.refresh()` on reveal.
   Flash still reported.
5. Reveal only after `REVEAL_STABILIZE_MS = 250` ms with repeated `fit()`/
   `refresh()` before and after setting opacity to 1, plus a post-visible
   refresh. Result: reported as removing the visible flash ("now it works").
   This was treated as scaffolding and subsequently removed (per §6.6/§6.7).
6. De-churn font loop to a single computed resize (§5) + `term.clearTextureAtlas()`
   after; removed the opacity hide and the 250 ms delay/refresh scaffolding.
   Flash still reported. Logs: browser font settles `46x38 → 31x25` in one step
   (no intermediate `29x23`).
7. Defer the WebGL addon load until after `SetConfig` settle so startup resizes
   run on the DOM renderer (§5); removed `clearTextureAtlas`. Flash still
   reported (latest run, 16:46).

Current tree contains: §3, §4, §5 (de-churn + deferred WebGL), and the page
background. The hide/opacity/delay scaffolding from steps 3–5 has been removed.

## 7. Canonical log timeline (latest run, 16:46, post-deferred-WebGL)

All `[mobile-gate]` lines are server-side; `[mobile-gate-client]` lines are
browser-side, delivered via the `DebugLog` control message.

```
[mobile-gate] enter_mobile_mode client=1 is_web=false tab=1 screen_size=80x24 created_tab_size=80x24 client_size=None gated=true
[mobile-gate-client] control-open proposeDimensions=46x38 term=46x38 domTermOpacity=1
[mobile-gate] mobile-tab resize tab=1 24x80 -> 38x46
[mobile-gate-client] SetConfig enter term=46x38 mobileViewport=true explicitFont=false candidate=24 font=monospace fontsLoaded=loaded termOpacity=1
[mobile-gate-client] SetConfig settled term=31x25
[mobile-gate] mobile-tab resize tab=1 38x46 -> 25x31
[mobile-gate] settled-size client=1 gated=true
[mobile-gate-client] term-recv bytes=7 term=31x25   (repeated; 7 = CLEAR_SCREEN)
[mobile-gate-client] term-recv bytes=1601 term=31x25   (first content frame)
[mobile-gate-client] term-recv bytes=1597/1888/1948/2063 term=31x25   (subsequent)
```

Consistent facts across all captured runs:
- At `enter_mobile_mode`: `is_web=false` at that instant, `screen_size=80x24`,
  `created_tab_size=80x24`, `client_size=None`. (`is_web` reads `true` later, at
  `try_reveal` time.)
- Server `mobile-tab resize` sequence: `80x24 → 46x38 → 31x25`.
- Every `render_to_clients` line for the gated client shows the real frame size
  pre-blank (e.g. 2449/2412/3036) followed by `blanked clients=[1]`; the browser
  receives only 7-byte frames until the first content frame.
- Browser receives the first content frame (~1601 bytes) and all subsequent
  frames at `term=31x25`; no `term-recv` line shows a non-31x25 size.
- Visual probe (present in step 6/7 builds before removal; values from a 15:40 /
  15:58 run): `cont=414 contScroll=414 screen=404 screenScroll=404
  canvasCss=404px canvasPx=1054 dpr≈2.609 body=414/414`, identical at `reveal`,
  `reveal+700`, and `reveal+1500` — i.e. no horizontal overflow in DOM/canvas
  geometry at any sampled time.
- Renderer probe (step 6 build): `renderer=canvas canvasCount=2` (the probe
  matched `.xterm-screen canvas`; xterm is configured with `WebglAddon`).
- `reveal-apply` (step 6 build): `fitBefore=31x25 fitAfter=31x25` (the reveal-time
  `fit()` did not change the grid).

## 8. Debug instrumentation currently in the tree (to remove once resolved)

- Server, tag `[mobile-gate]` (`log::info!`):
  - `zellij-server/src/mobile_mode.rs`: `gate`, `ungate` (with reason),
    `settled-size`, `paint-size`, `try_reveal` (client, is_web, reported, paint,
    settled, reveal), `blanked clients`.
  - `zellij-server/src/screen.rs`: `enter_mobile_mode` (client, is_web, tab,
    screen_size, created_tab_size, client_size, gated); `mobile-tab resize`
    (in `recompute_tab_size`, mobile tabs only); `render_to_clients`
    (per-client gated + byte length while gate non-empty); `mobile-paint`
    (client, plugin, painted_size, bytes_len, while gated); `timeout fired`
    (in `ForceMobileUngate`).
- Browser, tag `[mobile-gate-client]`, via `DebugLog`:
  - `zellij-client/src/web_client/websocket_handlers.rs:~79`: logs the message.
  - `zellij-client/assets/websockets.js`: `sendDebugLog` helper; logs at
    `control-open` (proposeDimensions, term, domTermOpacity), `fonts-ready`,
    `SetConfig enter` (term, mobileViewport, explicitFont, candidate, font,
    fontsLoaded, termOpacity), `SetConfig settled`, and `term-recv` (first 60
    frames: bytes + term size).
- Temporary `DebugLog` control-message variant
  (`control_message.rs` + `websocket_handlers.rs`) exists only to route browser
  logs into the server log; remove when instrumentation is removed.

## 9. Reproduction

1. `cargo x build`.
2. Launch the web server and open the web client with no session name (starts the
   `welcome` layout → mobile plugin). Reproduces only in `--release`.
3. `grep mobile-gate /tmp/zellij-1000/zellij-log/zellij.log` shows both
   `[mobile-gate]` (server) and `[mobile-gate-client]` (browser) lines.
