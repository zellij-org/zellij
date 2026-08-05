# Zellij Remote Control API

Adds a **WebSocket control plane** on top of the existing multiplexer.
Instead of a human driving the terminal, a remote program drives it: it manages
sessions and tabs, injects keyboard/mouse input, and receives a **git-diff-style
history of every change that appears on screen**.

Want an LLM agent driving it instead of writing WebSocket JSON by hand? See
**[MCP.md](./MCP.md)** — an MCP server that exposes this same API as tools.

## Design goals

1. **Keep the multiplexer.** No changes to how Zellij multiplexes. The API is an
   additional front-end, not a replacement. A session driven over the API is an
   ordinary Zellij session under the hood — discovered by `zellij list-sessions`
   and attachable with `zellij attach` exactly like one started interactively.
2. **Remote control instead of direct user control.** One WebSocket endpoint
   speaks a JSON command/event protocol.
3. **Session and tab management through the API.**
4. **Input through the API** — type text, send named keys, click with the mouse,
   addressed to a specific pane in a specific tab in a specific session.
5. **Screen history as diffs.** A pane's canvas is treated as a text file. Every
   change to that file produces a unified diff (`+sudo` / `-sudr`), stored in a
   per-pane history and streamed live.

## Why this shape

Zellij is already a client/server system: `zellij-server` owns a session and
speaks a protobuf IPC protocol over a unix socket per session
(`ClientToServerMsg` / `ServerToClientMsg`). Three existing primitives make a
control plane possible without forking the core:

| Need | Existing primitive |
| --- | --- |
| Drive the session | `ClientToServerMsg::Action { action, is_cli_client: true }` — the `Action` enum already covers tabs, panes, focus, resize, writes and mouse events |
| Query state | `Action::ListTabs/ListPanes/ListClients { output_json: true }` → replies as `ServerToClientMsg::Log { lines }` |
| Observe the screen | `ClientToServerMsg::SubscribeToPaneRenders { pane_ids, scrollback, ansi }` → `ServerToClientMsg::PaneRenderUpdate { pane_id, viewport: Vec<String>, .. }` pushed on every change |
| Create a session headlessly | `spawn_server(socket_path)` + `ClientToServerMsg::FirstClientConnected` (the same path the bundled web client uses — no TTY required) |

So the API server is a **process that acts as a headless multi-session client**.
It is not a patch to the render loop, so it composes with any future change to
that loop rather than fighting it.

```
   remote program
        │  JSON over WebSocket
        ▼
┌──────────────────────┐
│  zellij api-server   │   axum WS + session registry + canvas/diff engine
└─────────┬────────────┘
          │ protobuf IPC (one control conn + one observer conn per session)
   ┌──────┴───────┬──────────────┐
   ▼              ▼              ▼
zellij-server  zellij-server  zellij-server      (unchanged multiplexer)
 session A      session B      session C
```

### Three IPC connections per session

- **Control connection** — a CLI-style client used for queries and commands
  that need a reply. Serialized request/reply: one `Action` at a time.
- **Observer connection** — dedicated to `SubscribeToPaneRenders`, so the
  unsolicited render stream never interleaves with command replies.
- **Focus connection** — a *real attached client*, so the session has somewhere
  to hold focus (see below). Focus-dependent actions are sent here with
  `is_cli_client: false`, which makes the server attribute them to that client.
  A thread drains this socket: an attached client that stops reading its render
  stream would eventually block the server and stall the session.

The set of live panes is refreshed from `ListPanes` and the subscription is
renewed when panes appear or disappear, so newly created panes are tracked
automatically.

#### Matching replies to requests

The IPC protocol carries no request ids, so a reply can only be matched to its
request by position — and two server behaviours make naive position-matching
wrong:

- `UnblockInputThread` is **not** one-per-action. The server emits extra ones
  (a broadcast unblock reaches every connected client, ours included), so
  treating it as a terminator shifts every later reply by one.
- Some actions log more than once. `NewTab` reports both the new tab's id and
  the new pane's id, leaving a spare `Log` in the stream.

So each action declares the *shape* of its answer (`session_link::ReplyShape`)
and the reader skips anything that cannot be it. Queries ask for a JSON array or
object, which no stray line from another action parses as — which makes them
self-resynchronising rather than merely lucky.

## Protocol

Transport: WebSocket, text frames, one JSON object per frame.
Endpoint: `ws://<bind>:<port>/api?token=<token>`.

**Command** — `{"id": "<caller id>", "cmd": "<name>", ...params}`
**Reply** — `{"id": "<caller id>", "ok": true, "result": {...}}` or
`{"id": "<caller id>", "ok": false, "error": "..."}`
**Event** — `{"event": "<name>", ...}` (unsolicited, no `id`)

### Commands

| Command | Params | Result |
| --- | --- | --- |
| `session.list` | — | sessions: name, age_secs, resurrectable |
| `session.create` | `name?`, `layout?`, `cwd?`, `rows?`, `cols?` | created session name |
| `session.kill` | `name` | — |
| `session.rename` | `name`, `new_name` | — |
| `tab.list` | `session` | tabs: id, name, active, panes |
| `tab.create` | `session`, `name?`, `cwd?`, `layout?` | — |
| `tab.close` | `session`, `tab_id` | — |
| `tab.focus` | `session`, `tab_id` | focused tab id (once the session confirms it) |
| `tab.rename` | `session`, `tab_id`, `name` | — |
| `pane.list` | `session` | panes: id, title, tab, focused, geometry, command |
| `pane.create` | `session`, `command?`, `plugin?`, `plugin_config?`, `args?`, `cwd?`, `floating?`, `direction?`, `name?`, `tab_id?` | created pane id |
| `pane.close` | `session`, `pane_id` | — |
| `pane.focus` | `session`, `pane_id` | — (focuses the pane and its tab) |
| `pane.rename` | `session`, `pane_id`, `name` | — |
| `pane.resize` | `session`, `pane_id`, `resize`, `direction?` | — |
| `input.text` | `session`, `pane_id?`, `text` | bytes written |
| `input.keys` | `session`, `pane_id?`, `keys: ["ctrl-c", "enter", ...]` | bytes written |
| `input.mouse` | `session`, `kind`, `x`, `y`, `button?` | — |
| `screen.snapshot` | `session`, `pane_id`, `scrollback?` | viewport lines + version |
| `screen.history` | `session`, `pane_id`, `since?`, `limit?` | list of diffs |
| `screen.subscribe` | `session`, `pane_ids?` (omit = all) | — |
| `screen.unsubscribe` | `session` | — |

### Events

- `screen.diff` — a pane's canvas changed:

```json
{
  "event": "screen.diff",
  "session": "main",
  "pane_id": "terminal:1",
  "seq": 42,
  "ts": 1754342400123,
  "added": 1,
  "removed": 1,
  "unified": "--- pane/terminal:1@41\n+++ pane/terminal:1@42\n@@ -3,1 +3,1 @@\n-sudr\n+sudo\n"
}
```

- `screen.reset` — a canvas baseline, sent when a pane is first followed (and
  after a resubscribe). Carries `lines` instead of a diff; the version it
  establishes is `seq`.
- `pane.opened`, `pane.closed` — the session's pane set changed.
- `session.ended` — the session's server went away.
- `stream.lagged` — this connection could not keep up and `dropped` events were
  discarded. Resync with `screen.snapshot`, then continue from its `version`.

## Running it

```sh
zellij api-server --port 8787 --token <secret>     # --token may also come from ZELLIJ_API_TOKEN
```

Without `--token` the API accepts any connection that can reach the port, and
says so on startup. There is a `GET /health` endpoint for liveness checks.

### Running it as a service

To keep the API up across reboots, install the binary somewhere stable and run
it as a **systemd user service** (no root involved — sessions belong to your
user, and its sockets live under `/run/user/$UID`):

```ini
# ~/.config/systemd/user/zellij-api-server.service
[Unit]
Description=Zellij remote-control API (WebSocket)

[Service]
Type=simple
ExecStart=%h/.local/bin/zellij api-server --bind 127.0.0.1 --port 8787
EnvironmentFile=%h/.config/zellij-api/env   # holds ZELLIJ_API_TOKEN=...
Restart=always
RestartSec=2
KillMode=process   # see below — without this, restarting the service kills every session it created
NoNewPrivileges=true

[Install]
WantedBy=default.target
```

```sh
systemctl --user daemon-reload
systemctl --user enable --now zellij-api-server
loginctl enable-linger "$USER"   # only if not already on: start at boot without logging in
```

**`KillMode=process` is required, not optional.** Zellij daemonizes a session's
server process (double-fork + setsid) precisely so it survives whatever spawned
it — that is the entire basis for "a session driven over the API is an
ordinary Zellij session". systemd's default, `KillMode=control-group`, does not
honor that: it tracks descendants by cgroup membership, which daemonizing does
not escape (unlike a traditional process group). Left at the default, every
service restart — including a routine redeploy — silently kills every session
the API created. Reproduced directly: create a session, kill the service's
tracked PID, and watch the session's own process die at the same moment,
despite having correctly detached from it.

Three things worth being deliberate about:

- **Install a release binary to a stable path.** The API server spawns session
  servers by re-executing itself, so it must point at a binary that a later
  `cargo build` (or `cargo clean`) will not replace or delete.
- **Keep it on loopback, with a token.** Anything that can reach this port can
  run commands in your terminal sessions. `--bind 127.0.0.1` plus a token in a
  `0600` environment file is the sane default; exposing it to a network means
  putting real authentication and TLS in front of it.
- **`TERM` doesn't need to be set — the server defaults it if it's missing.**
  A daemon has no controlling terminal to inherit `TERM` from, and every
  session this server creates is a fork of itself, so an empty `TERM` here
  becomes an empty `TERM` in every pane's shell — which breaks
  terminal-aware shell behavior (redraws that assume terminfo capabilities
  they never actually confirmed). `start_api_server` defaults it to
  `xterm-256color` at startup if empty, the way tmux/screen default it for
  their own panes. Found and fixed after a genuinely confusing bug: see
  "Defects found by adversarial testing" below.

A worked example client lives in `zellij-api/examples/drive.rs`:

```sh
cargo run -p zellij-api --example drive -- --token <secret> --session demo
```

It creates a session, subscribes, types a command, and prints the diffs as they
arrive:

```
[screen.diff] pane="terminal_0" seq=3 +2 -0
  @@ -1,2 +1,4 @@
   echo hello_from_the_api
   ~/dev/zellij ❯ echo hello_from_the_api
  +hello_from_the_api
  +
```

### Knowing when a pane is ready

A remote driver cannot ask a shell whether it has finished starting, and a shell
discards anything typed before it is ready. The diff stream is the signal:
after creating a tab or pane, wait until no `screen.diff` has arrived for that
pane for a beat, then send input. `zellij-api/tests/e2e.rs` does exactly this.

### Plugin panes

Plugin panes are driven exactly like terminal panes — open one, address it by
id, and its UI reacts:

```json
{"cmd": "pane.create", "session": "main", "plugin": "session-manager", "floating": true}
→ {"pane_id": "plugin_3"}

{"cmd": "input.text", "session": "main", "pane_id": "plugin_3", "text": "abc"}
{"cmd": "input.keys", "session": "main", "pane_id": "plugin_3", "keys": ["backspace"]}
```

and the change comes back on the same diff stream as any other pane:

```
+Session: abc_ <ENTER> - Create new
+Session: ab_ <ENTER> - Create new
```

`plugin` takes a URL (`file:/path/to.wasm`, `https://…`) or a built-in alias
(`session-manager`, `about`, `strider`, …), with optional `plugin_config`.

What differs is only underneath: Zellij hands the bytes to a plugin as **key
events** rather than writing them to a pty, so `input.text` on a plugin is a
sequence of keypresses, not a paste. `input.keys` is usually what you want, and
named keys (`down`, `esc`, `enter`) are how a plugin UI is actually driven.

Two things to keep in mind:

- **A plugin pane can close itself.** `esc` dismisses the session manager, and
  its pane goes with it. Writing to a pane the session no longer has used to
  succeed silently — every `input.*` now checks the pane exists first and fails
  with `session 'main' has no pane plugin_3`.
- **Tiled plugin panes take no `direction`.** The pane is placed only once the
  plugin has loaded, by which time the reference pane may have moved, so
  upstream refuses it and so do we. Use `floating`, or omit `direction`.

### Focus is owned by the API

Much of Zellij is defined relative to *the requesting client*: which tab is
active, which pane a split happens next to, where unaddressed input goes. That
reference only exists if a client is attached — and a CLI message does not
count, because the server attributes it to the *last active client*, which is
only ever set by a keystroke.

So the API **attaches a real client to each session it drives**, and sends
focus-dependent actions as that client. Focus then behaves exactly as it does
for a person at a terminal, and it is fully controllable:

- `tab.focus` makes that tab active — `tab.list` reports `active: true` for it,
  and everything tab-relative follows.
- `pane.focus` focuses the pane *and* its tab, so input and splits land there.
- `pane.create` focuses the target first, then creates — which is what makes a
  directional split land where the caller asked. With a `direction` it also
  moves focus onto a terminal pane, since a floating pane cannot be split.

Focus commands are **verified, not assumed**: they reply only once the session
reports the new focus, and return an error if it never takes. `pane.create`
likewise identifies the new pane by watching it appear, because the server
reports a pane id before placing it and drops an unresolvable placement without
an error.

### Choosing the target pane

`input.*` without a `pane_id` goes to **the pane the attached client is sitting
on** — no cleverness, no preference for one kind of pane over another. So
`pane.focus` then `input.text` does what you would expect, and the reply always
reports which pane was written to. Pass `pane_id` explicitly to address any pane
regardless of focus, including a plugin pane (see above); a pane the session
does not have is rejected rather than silently written to.

`screen.snapshot` and `screen.history` follow the same rule when `pane_id` is
left out — read back whatever is focused, symmetric with writing to it. This
was not always true: they used to require `pane_id` explicitly, so a caller
could type blind into "the pane, whichever it is" but not read the result
back without already knowing its exact id.

For that to be useful, focus has to start somewhere sensible. Zellij greets a
new session with release notes and a startup tip, both floating plugin panes —
and a floating pane takes focus, so a freshly created session would sit with a
welcome screen focused and swallow the first thing you typed. Sessions the API
creates therefore set `show_release_notes = false` and `show_startup_tips =
false`, so the greeting is never created and focus starts on the shell.

**A human and the API can end up typing into the same pane at once.** Nothing
stops it — `input.*` writes to a pane's pty exactly like a keystroke does, and
a pty has no concept of "whose" byte arrived. If you are interacting with a
pane by hand at the same moment the API sends it text, the two interleave at
the byte level, arbitrarily. Confirmed directly: a stray character a person
typed landed *inside* a string the API sent to the same pane a moment later,
read back as if it had come from nowhere until the person who typed it said
so. Not a bug to fix — the same thing happens if two people type into a shared
tmux pane — but worth knowing before assuming a character you didn't expect
means something is wrong with the pipe.

> **Do not read focus off `pane.list`.** Its `is_focused` flag means "focused
> within its layer", so several panes carry it at once — a tiled pane and a
> floating pane, in *every* tab. After focusing a second pane in a tab, both
> still report `is_focused: true`. The single pane a client is actually on comes
> from `ListClients`, which is what this API uses internally and what
> `input.*` follows. `pane.list`'s flag is still useful for layout, just not for
> answering "where does typing go".

## Canvas-as-a-file model

Each pane maps to a virtual path `session/<name>/pane/<pane_id>.screen`.

- The first `PaneRenderUpdate` (`is_initial`) establishes **version 0**; it is
  recorded as a snapshot, not a diff.
- Every later update is diffed line-by-line against the previous version with a
  Myers diff, emitted as a unified diff with 3 lines of context, and appended to
  a bounded per-pane history (default 1000 entries, oldest dropped).
- `screen.snapshot` returns the current materialised lines plus the version, so
  a client that dropped events can resync and then continue from `seq`.

Trailing whitespace is trimmed per line before diffing, so cursor movement that
does not change visible text produces no diff entry.

## Testing

```sh
cargo test -p zellij-api                       # protocol, key encoding, diff and history units

cargo xtask build                              # the API server re-executes this binary
ZELLIJ_API_E2E=target/dev-opt/zellij \
  cargo test -p zellij-api --test e2e          # drives a real session end to end
```

The end-to-end test creates a session, adds a tab, subscribes to every pane,
types into the focused one, asserts the resulting diff and history, and kills
the session again. It skips itself unless `ZELLIJ_API_E2E` points at a built
binary, so a plain `cargo test` never spawns sessions.

## Known limitations

Deliberate trade-offs rather than oversights — worth knowing before building on
this.

- **Commands on one connection run one at a time.** The read loop awaits each
  command before taking the next, so a slow one (creating a session, or an
  action that hits the 10s timeout) holds up everything else on that socket,
  including commands for other sessions. This is on purpose: input must not be
  reordered, and running commands concurrently would make the order in which
  keystrokes arrive depend on task scheduling. Use a second WebSocket
  connection for work that must not queue behind another.
- **Every `input.*` costs one query.** Naming a pane checks it exists; omitting
  one asks which pane the client is on. Both are a round trip to the session, so
  input is a few milliseconds rather than microseconds. Sending a whole line as
  one `input.text` is much cheaper than one call per keystroke.
- **A wedged session retires its link.** If the server never answers, the
  command times out and the link is marked dead; the next command opens a fresh
  one. That recovers service, but the in-flight command is lost rather than
  retried — commands are not idempotent, so retrying them automatically would be
  worse.
- **`screen.snapshot` and `screen.history` only know about panes you have
  subscribed to.** The canvas is built from the render stream, so a pane that
  was never followed has no state to report.
- **`screen.unsubscribe` stops delivery, not production.** Zellij has no
  unsubscribe message; the session keeps sending updates and we stop forwarding
  them.
- **One token, all-or-nothing.** Presenting it grants every command on every
  session the API server can see. There are no per-session or read-only scopes.
- **`pane.create`'s `plugin` hangs for anything not built in.** Zellij
  auto-grants permissions to bundled plugins (`plugin_env.plugin.is_builtin()`
  in `zellij_exports.rs`) — that's every alias in the tools table below
  (`about`, `session-manager`, `strider`, ...), confirmed working end to end
  through the API. A plugin loaded from an external URL or `file://` path is
  *not* auto-trusted: it shows an interactive permission prompt and waits for
  a keypress, same as it would for a person at a terminal — and there is no
  person, so it waits forever. Not something the API should paper over by
  auto-granting (that would quietly weaken a deliberate security gate); if
  you need a non-built-in plugin driven over the API, its permissions need
  to be pre-approved some other way before `pane.create` requests it.

## Validation

Every command that names a session, tab, or pane checks it exists before
acting, and fails with a message that says exactly what was not found —
`session 'main' has no pane terminal_9`, `session 'main' has no tab 3 (known
tab ids: 0)`. This is not the server's default behavior: Zellij typically
drops an action aimed at something that is not there and reports nothing,
which reads as success from the caller's side. Affects `input.*`,
`pane.close`, `pane.focus`, `pane.rename`, `pane.resize`, `tab.close`,
`tab.rename`, `screen.subscribe`, and `screen.history`.

A parameter the command does not recognise is rejected —
`{"error": "unknown parameter: paneids"}` — rather than silently ignored. A
misspelled `pane_ids` on `screen.subscribe` would otherwise fall back to its
default and follow every pane in the session instead of the one named.

`session.create`'s `layout` is checked to exist on disk (when it looks like a
path) before the session is created, and `session.rename`'s `new_name` is
validated the same way `session.create`'s name is — see "Two defects found by
adversarial testing" below for why that one matters more than it sounds.

`pane.create`'s `args` is rejected if `command` isn't also given — it has no
meaning attached to a bare terminal pane or to `plugin`, and used to be
silently accepted and silently ignored in that case.

## Defects found by adversarial testing

Worth recording because none of these were hypothetical — each was reproduced
against a live session, not reasoned about in the abstract.

**Blocking work running directly on the async runtime, three places.** The
overlong-session-name bug below is one instance of a class: `SessionCreate`,
`SessionKill`, and — found while looking for siblings of the first two —
`link_for` (used by nearly every other command) each called synchronous,
blocking code straight from an `async fn`. For `SessionCreate`/
`SessionKill` that meant no bound on how long the *caller* could be left
waiting (see below). For `link_for`, calling `SessionManager::link`
directly meant the first time any session gets linked — three socket
connects with handshakes, plus a poll loop that can take up to a second —
ran on a Tokio worker thread and blocked *every other task scheduled on
that same thread*, not just the one request. All three now go through
`spawn_blocking`, moving the work off the runtime's worker pool; the first
two also get an outer `CALL_TIMEOUT` (15s) as a backstop in case their own
inner deadlines ever fail to fire again for a reason not yet found.

**An overlong session name hung `session.create` indefinitely.** A
300-character session name caused the call to hang — not slowly respond,
*hang*: the code's own internal 10-second startup deadline
(`SESSION_START_TIMEOUT`) is only checked after receiving some message
from the session server, and a session server that fails at `bind()`
before sending anything at all never gives that check a chance to run.
Root cause: a session name's socket path can exceed the OS's Unix-domain-
socket path limit (`sockaddr_un.sun_path`, ~104-108 bytes depending on
platform), and the daemonized session server fails silently when it does —
no error, no reply. The interactive CLI already guards against this
(`zellij_client::check_ipc_pipe_length`), but that check lives on a code
path this API never calls, since it spawns the session server directly
rather than going through `start_client`. Fixed by teaching
`validate_session_name` (`zellij-utils/src/sessions.rs` — already called
from both `session.create` and `session.rename`) to compute the real
socket path and reject it upfront, mirroring the CLI's own check but
returning a `Result` instead of exiting the process. Verified live: a
15-second-plus hang before the fix, a 0.007-second response with a clear
error after it.

**RESOLVED — every character written via `input.text`/`input.keys` appeared
twice on screen when this service ran as a systemd unit.** A brand-new
session, a clean baseline (`~ ❯` with nothing typed), one `input.text` call
with `text: "A"` — the screen read `~ ❯ AA`. Reliably reproduced when
deployed as a systemd unit; never reproduced from an ad-hoc
foreground process launched from an interactive shell — which is exactly
what made this hard to find, since every natural way to test it by hand
avoided the real failure condition.

**Root cause: `TERM` was empty.** A systemd service is a daemon with no
controlling terminal, so it has no `TERM` to inherit — unlike a process
launched interactively, which always has one. Every session this server
creates is a fork of this process itself (`zellij_server::spawn_server`),
so whatever `TERM` the server had at startup is what every pane's shell
inherits. With `TERM` empty, the pane's shell has no terminfo entry to
consult, and its syntax-highlighting redraw (re-coloring what it just
echoed) came back malformed: `"Z"` (plain echo), immediately followed by
`"\x1b[31mZ\x1b[39m"` (red, no leading backspace) instead of the correct
`"\x08\x1b[31mZ\x1b[39m"` (backspace, *then* red) — missing the
cursor-repositioning byte that would have made it an in-place overwrite.
Applied literally, that lands two visible characters from one keystroke.
Confirmed with `log::error!` instrumentation directly in `screen.rs`'s
`ScreenInstruction::PtyBytes` handler (not `eprintln!`, which never reaches
a daemonized process — its stdout/stderr go to `/dev/null`), and with two
independent, statistically overwhelming batches against the live deployed
service: 30/30 reproductions with `TERM` empty, 0/70 across three
rebuild-and-redeploy cycles once fixed.

**Fixed** in `src/commands.rs`'s `start_api_server`: default `TERM` to
`xterm-256color` at startup if empty or unset, the same way tmux/screen
default it for their own panes. Regression test
`typing_without_a_term_environment_variable_does_not_duplicate_characters`
in `zellij-api/tests/e2e.rs` reproduces the exact condition deterministically
with `Command::env_remove("TERM")` (proved it catches the bug: failed on the
first try against the pre-fix binary, passed consistently once fixed). A
second test from earlier in the investigation,
`typing_with_no_other_client_attached_does_not_duplicate_characters`,
exercises a real but independent correctness bug found along the way —
`ClearScroll(client_id)` resolving a client-relative "active pane" instead
of the pane a pane-id-addressed write already names explicitly, fixed by
adding `ClearScrollForPaneId` — which turned out not to be the cause of
this particular symptom, but was still worth fixing on its own merits.

**`session.rename` allowed a path-traversal socket rename.** Upstream's
`RenameSession` handler builds `ZELLIJ_SOCK_DIR.join(&new_name)` and
`std::fs::rename`s the socket to it, with no validation of `new_name` at all.
Renaming a session to `../evil` moved its socket *out of* the directory session
discovery scans:

```
$ session.rename "audit7" -> "../evil"
$ ls /run/user/1000/zellij/
contract_version_1/  evil          ← orphaned, outside the scanned dir
```

The session was still running, just no longer reachable by name through either
`zellij` or the API — an orphaned process. A longer `../../..` reaches further.
This is reachable through a wire protocol in a way it is not through the
interactive rename UI (a person types a plain name), so `session.rename` now
runs `new_name` through `validate_session_name` — the same gate
`session.create` already used — before the action is ever sent.

**Renaming, then acting on the new name immediately, was flaky.** The socket
file rename is synchronous by the time the rename command replies, but a
probe connecting to the just-renamed path failed roughly one time in three in
practice — reproduced by chaining `session.create` → `session.rename` →
`session.kill` on the new name in one connection. The session was verifiably
running throughout (`list-sessions` moments later always showed it); the
existence check just lost a narrow race. `session_exists_settled` in
`zellij-api/src/session_link.rs` retries the check briefly (five attempts,
150ms apart) before concluding a session is not there, which stopped the
failure from reproducing across repeated runs.

**Two callers touching an unlinked session at once could attach two clients.**
`SessionManager::link()` used to check the link map, then — outside the lock —
open a new link if none existed. Two commands arriving together for the same
not-yet-linked session each saw "none exists" and each opened one. Since a link
attaches a real client (see "Focus is owned by the API" above), the session
ended up with two attached, which does not fail loudly — it quietly breaks
focus resolution, which identifies "our" client by it being the only one
attached. Fixed by holding the map lock across the whole get-or-open. Confirmed
by temporarily reverting the fix and re-running the regression test: five
callers produced five attached clients instead of one.

**`pane.focus` was the one mutation that skipped the existence check.**
Every other pane/tab mutation validates its target first and fails at once
with a specific message; `pane.focus` instead ran the full ~1.2s
focus-confirmation retry loop (waiting for a focus change that was never
going to happen) before failing with the vaguer "the session did not focus
pane X (no attached client moved to it)". Fixed by adding the same
`pane_exists` check the other handlers already use. Regression test
`focusing_a_missing_pane_fails_fast_with_a_specific_message` in
`zellij-api/tests/e2e.rs` asserts both the message and a sub-500ms latency
bound; proved it catches the regression by running it against the binary
built before this fix and watching it fail with the old vague message.

**`screen.history` had no existence check at all, unlike every other
pane-targeting command.** It's a read against the canvas store, keyed
purely by pane id string — "never subscribed" and "no such pane" are the
same absent-key case to that lookup, and only the former should report an
empty history rather than an error. Found by fuzzing a made-up pane id
against a live session: it came back `{"diffs": [], "ok": true}` instead of
failing. Fixed by adding the same `pane_exists` check every other
pane-targeting handler already has. Regression test
`screen_history_rejects_a_pane_that_does_not_exist` covers both directions
so the fix can't regress the legitimate empty-history case.

## Footprint

Most of the new code lives in a new workspace crate, `zellij-api/`, plus the
MCP server on top of it in `zellij-mcp/`. Existing crates gained:

- `Cargo.toml` — add the two crates to the workspace and `zellij-api` to the
  binary's deps.
- `zellij-utils/src/cli.rs` — one new `ApiServer` subcommand.
- `src/commands.rs` / `src/main.rs` — dispatch that subcommand.
- `zellij-utils/src/sessions.rs` — `validate_session_name` also rejects
  overlong socket paths and is now applied to `session.rename`, not just
  `session.create` (see "Defects found by adversarial testing" below).
- `zellij-utils/src/errors.rs`, `zellij-server/src/route.rs`,
  `zellij-server/src/screen.rs`, `zellij-server/src/tab/mod.rs` — the
  pane-id-scoped `ClearScrollForPaneId` fix, also described below.

The multiplexer, renderer, plugin system and IPC contract are otherwise
untouched.
