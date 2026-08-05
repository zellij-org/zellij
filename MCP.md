# Zellij Remote Control — MCP Server

`zellij-mcp` exposes the [WebSocket remote-control API](./REMOTE_API.md) as MCP
tools, so an LLM agent (Claude, or anything else that speaks
[MCP](https://modelcontextprotocol.io)) can list, create, and drive Zellij
sessions directly.

It is a **thin client** of the WebSocket API, not a reimplementation of it —
every tool call becomes one JSON command sent to an already-running
`zellij api-server`, the same protocol `zellij-api/examples/drive.rs` speaks.
All session/tab/pane/input logic lives in `zellij-api` and is tested there;
this crate's own job is the MCP wiring, and that is what its own tests focus
on.

```
  MCP client (an LLM agent)
        │ Streamable HTTP, JSON-RPC, Authorization: Bearer <mcp token>
        ▼
┌──────────────────┐
│    zellij-mcp     │   rmcp server, 21 tools, stateless
└─────────┬─────────┘
          │ WebSocket, ?token=<api token>          (one connection per call)
          ▼
┌──────────────────┐
│  zellij api-server │  ← unchanged; see REMOTE_API.md
└──────────────────┘
```

## Why "attach" needs its own tool

A human running `zellij attach` gets oriented by *looking at the screen*. An
MCP agent has no screen to look at — so `attach_session` is the composite tool
that gives it the same orientation in one call: every tab, every pane, and the
current on-screen content of each terminal pane, in one JSON response. Call it
first when picking up a session. `read_screen` is the single-pane version, for
checking on one pane after driving it.

## Tools

| Tool | Wraps |
| --- | --- |
| `list_sessions` | `session.list` |
| `create_session` | `session.create` |
| `kill_session` | `session.kill` |
| `rename_session` | `session.rename` |
| `attach_session` | `tab.list` + `pane.list` + `screen.subscribe` + `screen.snapshot` per pane |
| `list_tabs` / `create_tab` / `focus_tab` / `close_tab` / `rename_tab` | `tab.*` |
| `list_panes` / `create_pane` / `focus_pane` / `close_pane` / `rename_pane` / `resize_pane` | `pane.*` |
| `send_text` / `send_keys` / `send_mouse` | `input.*` |
| `read_screen` | `screen.snapshot` (auto-subscribes if not already followed) |
| `screen_history` | `screen.history` |

Every tool's parameters and behavior match the underlying command exactly —
see [REMOTE_API.md](./REMOTE_API.md#protocol) for the full semantics (what
`pane_id` omitted means, why a directional split needs focus, and so on).
Nothing here changes any of that; it only translates.

**Writing and focusing are separate, deliberately.** `send_text`/
`send_keys`/`send_mouse` only write bytes — they never move focus, even
when addressed to a `pane_id` in a different tab than the one currently
focused. Only `focus_pane`/`focus_tab` actually change what's focused. This
matters in practice when a person is also attached to the same session
live: writing to an explicit pane never yanks their view somewhere else,
only an explicit focus call does. Each tool's own description repeats this
so it's visible without cross-referencing this doc.

## Running it

```sh
zellij-mcp --port 8788 --token <mcp-secret> \
  --api-url ws://127.0.0.1:8787/api --api-token <api-secret>
```

Two separate tokens, two separate trust boundaries: `--token` is what an MCP
client must present to *this* server; `--api-token` is what this server
presents to the WebSocket API it drives. Reusing one token for both would
conflate "who can ask an agent to drive Zellij" with "what the WS API itself
accepts" — keeping them apart means either can be rotated independently.

Like the WS API, this refuses to start quietly unauthenticated — without
`--token` it prints a warning naming exactly what that means.

### As a service

Deployed the same way as the WS API — a systemd **user** service, alongside
it:

```ini
# ~/.config/systemd/user/zellij-mcp.service
[Unit]
After=zellij-api-server.service
Wants=zellij-api-server.service

[Service]
ExecStart=%h/.local/bin/zellij-mcp --bind 127.0.0.1 --port 8788
EnvironmentFile=%h/.config/zellij-api/env   # ZELLIJ_MCP_TOKEN, ZELLIJ_API_TOKEN
Restart=always
RestartSec=2
NoNewPrivileges=true

[Install]
WantedBy=default.target
```

**No `KillMode=process` here, unlike the WS API's own unit.** That setting
exists because the WS API daemonizes session servers that must outlive it —
this process spawns nothing; it is only a WebSocket client. A cgroup-wide kill
on restart takes down nothing that needs to survive it.

```sh
systemctl --user daemon-reload
systemctl --user enable --now zellij-mcp
```

## Auth

Bearer token via the `Authorization: Bearer <token>` header, checked with a
constant-time comparison (see `zellij-api`'s own `tokens_match` for why a
plain `==` would leak timing information). `GET /health` is deliberately
unauthenticated, for liveness checks.

## Testing

```sh
cargo xtask build                      # target/dev-opt/zellij
cargo build -p zellij-mcp              # target/debug/zellij-mcp
ZELLIJ_API_E2E=target/dev-opt/zellij ZELLIJ_MCP_E2E=target/debug/zellij-mcp \
  cargo test -p zellij-mcp --test e2e -- --nocapture
```

The e2e suite drives a real MCP client (`rmcp`'s own) against a real
`zellij-mcp` server against a real `zellij api-server` against a real Zellij
session: `create_session` → `attach_session` → `send_text` → `read_screen` →
`kill_session`, plus a check that every tool is discoverable and that the
wrong bearer token is refused. It does not re-test the underlying command
semantics — `zellij-api`'s own suite already does that exhaustively; this
suite exists to catch protocol-wiring and schema mistakes specific to the MCP
layer.

## Defects found by testing

Five, so far — each reproduced against a live deployment, not reasoned about
in the abstract, and each guarded by a regression test proven to actually
catch the bug (run against the pre-fix binary first, confirmed it failed
the same way). The most severe — `send_text`/`send_keys` writing every
character twice — turned out to be caused by `TERM` being unset when this
service runs as a systemd daemon (no controlling terminal to inherit it
from), breaking the spawned shell's own redraw logic. See
[REMOTE_API.md](./REMOTE_API.md#defects-found-by-adversarial-testing) for
the full write-up and fix (`start_api_server` now defaults `TERM` if empty,
the way tmux/screen do for their own panes).

### `attach_session`/`read_screen` came back empty on a race

`attach_session`'s snapshot loop first came back with `"screens": []` — every
pane correctly listed, none snapshotted. Root cause: `screen.subscribe`
returns as soon as the *subscription request* is acknowledged, not once the
canvas actually has content — the first render event that populates it arrives
asynchronously afterward. Calling `screen.snapshot` immediately after
subscribing can race that and fail with "subscribe first," and the original
code was silently swallowing that failure (`if let Ok(snapshot) = ...`) rather
than surfacing or retrying it. Fixed with a short bounded retry (up to 8
attempts, 100ms apart) in both `attach_session` and `read_screen`'s
auto-subscribe path. Caught by the e2e test, not by inspection — the JSON
Value the test asserted on made the empty array impossible to miss.

### Business failures were protocol-level errors, not tool-level ones

Every tool used to report failures — "session not running," "unknown key
name," "give `command` or `plugin`, not both" — by returning
`Err(McpError::internal_error(...))`. In MCP terms that's a JSON-RPC
`error` object: it says "I couldn't even attempt this," the same category as
a malformed request or an unknown tool name. An agent calling `list_tabs` on
a typo'd session name doesn't get a `CallToolResult` back to reason about —
it gets a client-side call failure, indistinguishable from the tool not
existing at all.

Per rmcp's own documented convention: a protocol-level error means the
server couldn't run anything; a tool-level error (`Ok(CallToolResult::error(
vec![ContentBlock::text(msg)]))`, `isError: true` in the result) means the
tool ran, produced no useful result, and the *caller* should see why and
react. All 21 tools' business-logic failures now go through a shared
`err_result()` helper that does the latter. Two composite tools needed
restructuring beyond a search-and-replace: `attach_session`'s per-pane
snapshot loop used to drop a failed pane silently (only a `log::warn!`) —
it now reports `{"pane_id": ..., "error": ...}` inline instead of a gap
in the array; `read_screen`'s `screen.subscribe` failure is now surfaced
directly rather than falling through to a less specific error from the
snapshot call that follows it.

Verified live (curl against the deployed service) and with a regression
test, `business_failures_are_tool_errors_not_protocol_errors` in
`zellij-mcp/tests/e2e.rs`: a nonexistent session returns `isError: true`
with `"...not running"` in the content, while a genuinely nonexistent tool
name still comes back as a real `Err` from `call_tool` — the protocol-level
case is still a protocol-level error, only the business-logic case moved.

### Inconsistent parameter naming (`kill_session` / `rename_session`)

19 of the 21 tools name their "which session" parameter `session`.
`kill_session` and `rename_session` were the odd ones out — they took
`name` instead, a leftover from mirroring the underlying wire command's
own field name too literally. In practice this meant an agent that had
correctly learned the convention from any of the other 19 tools got a
confusing `"missing field \`session\`"` on exactly these two, with no way
to guess the right name short of reading that tool's schema individually.
Fixed by renaming the parameter (the underlying `zellij-api` wire call
still sends `name`, unaffected — this was purely the MCP-facing schema).
`create_session` is the one deliberate exception: its `name` names the
session being *created*, not one being looked up, so it stays as-is.

Caught by re-reading the tool schemas end to end, not by inspection of any
one tool in isolation — the inconsistency was only visible across all 21 at
once. Guarded going forward by
`every_tool_names_its_session_parameter_consistently` in
`zellij-mcp/tests/e2e.rs`, which asserts every tool but `create_session`/
`list_sessions` declares a `session` property in its schema. Proved the
test actually catches the bug via the same revert-then-retest the rest of
this PR's fixes went through.

### `read_screen`/`screen_history` couldn't default to the focused pane

`send_text` and `send_keys` both make `pane_id` optional and default to
whatever pane the session currently has focused — the same thing a human
typing at a terminal would mean by "wherever I am." `read_screen` and
`screen_history` required `pane_id` outright, so an agent could blindly
type into "the pane, whichever it is" but couldn't read the result back
without already knowing that pane's exact id — found via a holistic smoke
test (create a session, split panes, switch tabs, drive it without ever
naming a pane), not isolated per-tool fuzzing.

Fixed symmetrically with the input tools: `pane_id` is optional on both,
defaulting to the focused pane via `zellij-api`'s existing `resolve_pane`
helper. `read_screen` got its own dedicated params type rather than reusing
the `pane_id`-required one shared by `close_pane`/`focus_pane`/
`rename_pane` — those three stay required on purpose, since defaulting a
*mutation* to "whichever pane happens to be focused" is a meaningfully
riskier default than defaulting a *read*. Guarded by
`read_screen_and_screen_history_default_to_the_focused_pane` in
`zellij-mcp/tests/e2e.rs`.
