//! The WebSocket control plane.
//!
//! One endpoint, `GET /api`, upgrades to a WebSocket that speaks the JSON
//! protocol in [`crate::protocol`]. Each connection may drive any number of
//! sessions and may subscribe to their screen-diff streams.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use zellij_utils::data::{Direction, PaneId, Resize};
use zellij_utils::input::actions::Action;
use zellij_utils::input::command::RunCommandAction;
use zellij_utils::input::layout::RunPluginOrAlias;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::position::Position;

use crate::keys::keys_to_bytes;
use crate::protocol::{Command, Reply, Request};
use crate::session_link::{SessionLink, PANE_POLL_INTERVAL};
use crate::sessions::SessionManager;

/// Backstop for the blocking session-create/kill calls below. Both have
/// their own internal logic that's supposed to bound how long they take,
/// but that logic depends on the session server sending *something* back —
/// if it never does (found live: a silent `bind()` failure), the blocking
/// call can outlive its own intended deadline. This is longer than either
/// inner deadline so it only fires when that inner logic has already
/// failed to, not as a matter of course.
const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone)]
pub struct ApiState {
    pub sessions: Arc<SessionManager>,
    /// When set, callers must present `?token=...` to connect.
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConnectParams {
    #[serde(default)]
    pub token: Option<String>,
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api", get(ws_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(state)
}

/// Run the API server until the process is stopped.
pub async fn serve(addr: SocketAddr, token: Option<String>) -> anyhow::Result<()> {
    let state = ApiState {
        sessions: Arc::new(SessionManager::new()),
        token,
    };
    let listener = tokio::net::TcpListener::bind(addr).await?;
    log::info!("zellij api server listening on ws://{}/api", addr);
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    if let Some(expected) = &state.token {
        let presented = params.token.as_deref().unwrap_or_default();
        if !tokens_match(expected, presented) {
            return (StatusCode::UNAUTHORIZED, "invalid or missing token").into_response();
        }
    }
    ws.on_upgrade(move |socket| handle_connection(socket, state))
        .into_response()
}

/// Compare tokens without giving away how much of one was right.
///
/// A `==` on strings stops at the first differing byte, so how long the check
/// takes leaks the length of the correct prefix. This reads both to the end
/// regardless. The length is compared separately — it is not a secret.
fn tokens_match(expected: &str, presented: &str) -> bool {
    let (expected, presented) = (expected.as_bytes(), presented.as_bytes());
    if expected.len() != presented.len() {
        return false;
    }
    expected
        .iter()
        .zip(presented)
        .fold(0u8, |differences, (a, b)| differences | (a ^ b))
        == 0
}

/// Per-connection state: the sessions this caller is streaming, and the tasks
/// doing the streaming.
#[derive(Default)]
struct Streams {
    tasks: HashMap<String, Vec<JoinHandle<()>>>,
}

impl Streams {
    fn stop(&mut self, session: &str) {
        if let Some(handles) = self.tasks.remove(session) {
            for handle in handles {
                handle.abort();
            }
        }
    }

    fn stop_all(&mut self) {
        for (_, handles) in self.tasks.drain() {
            for handle in handles {
                handle.abort();
            }
        }
    }
}

async fn handle_connection(socket: WebSocket, state: ApiState) {
    let (mut sink, mut stream) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();

    // A single writer task owns the sink, so replies and events never interleave
    // mid-frame.
    let writer = tokio::spawn(async move {
        while let Some(text) = out_rx.recv().await {
            if sink.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut streams = Streams::default();

    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(_) => {
                    send_json(&out_tx, &Reply::err(None, "binary frames must be UTF-8 JSON"));
                    continue;
                },
            },
            Message::Close(_) => break,
            _ => continue,
        };

        let raw: serde_json::Value = match serde_json::from_str(&text) {
            Ok(raw) => raw,
            Err(e) => {
                send_json(&out_tx, &Reply::err(None, format!("bad request: {}", e)));
                continue;
            },
        };
        let request: Request = match serde_json::from_value(raw.clone()) {
            Ok(request) => request,
            Err(e) => {
                send_json(&out_tx, &Reply::err(None, format!("bad request: {}", e)));
                continue;
            },
        };

        let id = request.id.clone();
        // Reject what we do not understand rather than acting on a command the
        // caller did not quite write.
        let unknown = request.unknown_fields(&raw);
        if !unknown.is_empty() {
            send_json(
                &out_tx,
                &Reply::err(id, format!("unknown parameter: {}", unknown.join(", "))),
            );
            continue;
        }
        let reply = match dispatch(request.command, &state, &out_tx, &mut streams).await {
            Ok(result) => Reply::ok(id, result),
            Err(e) => Reply::err(id, e),
        };
        send_json(&out_tx, &reply);
    }

    streams.stop_all();
    writer.abort();
}

fn send_json(out: &mpsc::UnboundedSender<String>, value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(text) => {
            let _ = out.send(text);
        },
        Err(e) => log::error!("could not serialize an outbound frame: {}", e),
    }
}

async fn dispatch(
    command: Command,
    state: &ApiState,
    out: &mpsc::UnboundedSender<String>,
    streams: &mut Streams,
) -> Result<serde_json::Value, String> {
    match command {
        // ---- sessions ---------------------------------------------------
        Command::SessionList => Ok(json!({ "sessions": state.sessions.list() })),
        Command::SessionCreate {
            name,
            layout,
            cwd,
            rows,
            cols,
        } => {
            let sessions = state.sessions.clone();
            // Session creation spawns a process and blocks on IPC, so keep it
            // off the async runtime's worker threads.
            //
            // `sessions.create` has its own internal startup deadline, but
            // that deadline is only checked *between* messages from the
            // spawned session server — a peer that fails before sending
            // anything at all (found live: an overlong session name causing
            // a silent `bind()` failure, since fixed at the validation
            // layer) never gives it the chance to run, so the blocking call
            // could hang past its own supposed timeout. This outer bound is
            // the backstop: if that inner logic ever fails to return for a
            // reason not yet found, the caller still gets an answer instead
            // of hanging forever. The blocked OS thread itself is abandoned,
            // not killed — Tokio has no way to force that — but the caller
            // is unblocked either way, which is what actually matters here.
            let outcome = tokio::time::timeout(
                CALL_TIMEOUT,
                tokio::task::spawn_blocking(move || sessions.create(name, layout, cwd, rows, cols)),
            )
            .await
            .map_err(|_| "timed out creating the session".to_string())?
            .map_err(|e| format!("session creation panicked: {}", e))??;
            Ok(json!({ "session": outcome }))
        },
        Command::SessionKill { name } => {
            let sessions = state.sessions.clone();
            let target = name.clone();
            // Same backstop as `SessionCreate` above: `sessions.kill` blocks
            // on the session server acknowledging the kill, with no timeout
            // of its own — if the server is wedged mid-shutdown and never
            // closes the connection or replies, this bounds how long the
            // caller waits instead of leaving it hanging indefinitely.
            tokio::time::timeout(
                CALL_TIMEOUT,
                tokio::task::spawn_blocking(move || sessions.kill(&target)),
            )
            .await
            .map_err(|_| "timed out killing the session".to_string())?
            .map_err(|e| format!("session kill panicked: {}", e))??;
            streams.stop(&name);
            Ok(json!({ "killed": name }))
        },
        Command::SessionRename { name, new_name } => {
            // Upstream's rename handler does `ZELLIJ_SOCK_DIR.join(&name)` and
            // renames the socket file to it, with no validation at all — a
            // `new_name` of `../evil` renames the socket *out of* the
            // directory session discovery scans, orphaning the session, and a
            // longer `../../..` reaches further still. `validate_session_name`
            // is what `session.create` already runs a name through; run the
            // rename target through the same gate before it ever reaches the
            // server.
            zellij_utils::sessions::validate_session_name(&new_name)?;
            let link = link_for(state, &name).await?;
            link.run_action(Action::RenameSession {
                name: new_name.clone(),
            })
            .await?;
            // The socket is named after the session, so the old link is stale.
            streams.stop(&name);
            state.sessions.drop_link(&name);
            Ok(json!({ "session": new_name }))
        },

        // ---- tabs -------------------------------------------------------
        Command::TabList { session } => {
            let link = link_for(state, &session).await?;
            let tabs = link.list_tabs().await?;
            Ok(json!({ "tabs": tabs }))
        },
        Command::TabCreate { session, name, cwd } => {
            let link = link_for(state, &session).await?;
            link.run_action(Action::NewTab {
                tiled_layout: None,
                floating_layouts: vec![],
                swap_tiled_layouts: None,
                swap_floating_layouts: None,
                tab_name: name,
                should_change_focus_to_new_tab: true,
                cwd: cwd.map(Into::into),
                initial_panes: None,
                first_pane_unblock_condition: None,
            })
            .await?;
            let tab = link.active_tab().await?;
            Ok(json!({ "tab_id": tab.tab_id, "name": tab.name }))
        },
        Command::TabClose { session, tab_id } => {
            let link = link_for(state, &session).await?;
            link.require_tab(tab_id as usize).await?;
            link.run_action(Action::CloseTabById { id: tab_id }).await?;
            Ok(json!({ "closed": tab_id }))
        },
        Command::TabFocus { session, tab_id } => {
            let link = link_for(state, &session).await?;
            // Focus belongs to a client, so this goes through the session's
            // attached client — and we confirm it actually took, rather than
            // reporting a focus change the session did not make.
            link.require_tab(tab_id as usize).await?;
            let focused = link
                .focus_tab(tab_id as usize)
                .await
                .map_err(|e| format!("could not focus tab {}: {}", tab_id, e))?;
            Ok(json!({ "focused": focused }))
        },
        Command::TabRename {
            session,
            tab_id,
            name,
        } => {
            let link = link_for(state, &session).await?;
            link.require_tab(tab_id as usize).await?;
            link.run_action(Action::RenameTabById {
                id: tab_id,
                name: name.clone(),
            })
            .await?;
            Ok(json!({ "tab_id": tab_id, "name": name }))
        },

        // ---- panes ------------------------------------------------------
        Command::PaneList { session } => {
            let link = link_for(state, &session).await?;
            let panes = link.list_panes().await?;
            Ok(json!({ "panes": panes }))
        },
        Command::PaneCreate {
            session,
            command,
            plugin,
            plugin_config,
            args,
            cwd,
            floating,
            direction,
            name,
            tab_id,
        } => {
            let link = link_for(state, &session).await?;
            let direction = parse_direction(direction.as_deref())?;
            if command.is_some() && plugin.is_some() {
                return Err("give either `command` or `plugin`, not both".to_string());
            }
            if command.is_none() && !args.is_empty() {
                return Err("`args` only applies to `command`".to_string());
            }
            if plugin.is_some() && !floating && direction.is_some() {
                // Upstream deliberately refuses a direction for tiled plugin
                // panes: the pane is placed only once the plugin has loaded, by
                // which time the reference pane may have moved.
                return Err(
                    "`direction` is not supported for tiled plugin panes; use `floating` \
                     or omit `direction`"
                        .to_string(),
                );
            }
            // Target a tab explicitly. Left unset, the server places the pane
            // in whatever tab the *last active client* is focused on — and a
            // session driven over the API usually has no attached client, so
            // that resolves to nothing and the pane is silently never created.
            let tab_id = match tab_id {
                Some(tab_id) => tab_id as usize,
                None => link.active_tab().await?.tab_id,
            };
            let run_command = command.map(|command| RunCommandAction {
                command: command.into(),
                args,
                cwd: cwd.clone().map(Into::into),
                direction,
                hold_on_close: true,
                hold_on_start: false,
                originating_plugin: None,
                use_terminal_title: false,
            });
            // Create the pane the way a person would: put focus where the pane
            // should go, then ask for it. Placement is resolved against the
            // focused pane of the focused tab, so this is what makes a
            // directional split land where the caller asked.
            link.focus_tab(tab_id).await?;
            if direction.is_some() {
                // A split needs a *terminal* pane to split away from. A fresh
                // session focuses a floating plugin pane (the welcome screen),
                // which cannot be split — so move focus to a real one first.
                let reference = link.reference_pane_in_tab(tab_id).await?;
                link.focus_pane(PaneId::Terminal(reference)).await?;
            }

            let action = match plugin {
                Some(plugin) => {
                    let plugin = RunPluginOrAlias::from_url(
                        &plugin,
                        &plugin_config,
                        None, // aliases are resolved by the session
                        cwd.clone().map(Into::into),
                    )
                    .map_err(|e| format!("invalid plugin '{}': {}", plugin, e))?;
                    if floating {
                        Action::NewFloatingPluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: false,
                            cwd: cwd.map(Into::into),
                            coordinates: None,
                            no_focus: false,
                            tab_id: None,
                        }
                    } else {
                        Action::NewTiledPluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: false,
                            cwd: cwd.map(Into::into),
                            no_focus: false,
                            tab_id: None,
                        }
                    }
                },
                None if floating => Action::NewFloatingPane {
                    command: run_command,
                    pane_name: name,
                    coordinates: None,
                    near_current_pane: false,
                    no_focus: false,
                    tab_id: None,
                },
                None => Action::NewTiledPane {
                    direction,
                    command: run_command,
                    pane_name: name,
                    near_current_pane: false,
                    no_focus: false,
                    borderless: None,
                    tab_id: None,
                },
            };

            // The server reports a new pane's id before placing it, and drops
            // a placement it cannot resolve without an error — so identify the
            // pane by watching it appear instead of trusting that report.
            let before = link.pane_ids_now().await?;
            link.run_as_client(action);
            let pane_id = link.await_new_pane(&before).await?;
            Ok(json!({ "pane_id": pane_id }))
        },
        Command::PaneClose { session, pane_id } => {
            let link = link_for(state, &session).await?;
            let pane = parse_pane_id(&pane_id)?;
            if !link.pane_exists(pane).await? {
                return Err(format!("session '{}' has no pane {}", session, pane_id));
            }
            let action = match pane {
                PaneId::Terminal(id) => Action::CloseTerminalPane { pane_id: id },
                PaneId::Plugin(id) => Action::ClosePluginPane { pane_id: id },
            };
            link.run_action(action).await?;
            Ok(json!({ "closed": pane_id }))
        },
        Command::PaneFocus { session, pane_id } => {
            let link = link_for(state, &session).await?;
            let pane = parse_pane_id(&pane_id)?;
            if !link.pane_exists(pane).await? {
                return Err(format!("session '{}' has no pane {}", session, pane_id));
            }
            link.focus_pane(pane)
                .await
                .map_err(|e| format!("could not focus pane {}: {}", pane_id, e))?;
            Ok(json!({ "focused": pane_id }))
        },
        Command::PaneRename {
            session,
            pane_id,
            name,
        } => {
            let link = link_for(state, &session).await?;
            let pane = parse_pane_id(&pane_id)?;
            if !link.pane_exists(pane).await? {
                return Err(format!("session '{}' has no pane {}", session, pane_id));
            }
            link.run_action(Action::RenamePaneByPaneId {
                pane_id: Some(pane),
                name: name.clone().into_bytes(),
            })
            .await?;
            Ok(json!({ "pane_id": pane_id, "name": name }))
        },
        Command::PaneResize {
            session,
            pane_id,
            resize,
            direction,
        } => {
            let link = link_for(state, &session).await?;
            let pane = parse_pane_id(&pane_id)?;
            if !link.pane_exists(pane).await? {
                return Err(format!("session '{}' has no pane {}", session, pane_id));
            }
            let resize = Resize::from_str(&resize)?;
            link.run_action(Action::ResizeByPaneId {
                pane_id: pane,
                resize,
                direction: parse_direction(direction.as_deref())?,
            })
            .await?;
            Ok(json!({ "pane_id": pane_id }))
        },

        // ---- input ------------------------------------------------------
        Command::InputText {
            session,
            pane_id,
            text,
        } => {
            let link = link_for(state, &session).await?;
            let pane = resolve_pane(&link, pane_id).await?;
            let bytes = text.len();
            link.run_action(Action::WriteCharsToPaneId {
                chars: text,
                pane_id: pane,
            })
            .await?;
            Ok(json!({ "pane_id": pane.to_string(), "bytes": bytes }))
        },
        Command::InputKeys {
            session,
            pane_id,
            keys,
        } => {
            let link = link_for(state, &session).await?;
            let pane = resolve_pane(&link, pane_id).await?;
            let bytes = keys_to_bytes(&keys)?;
            let written = bytes.len();
            link.run_action(Action::WriteToPaneId {
                bytes,
                pane_id: pane,
            })
            .await?;
            Ok(json!({ "pane_id": pane.to_string(), "bytes": written }))
        },
        Command::InputMouse {
            session,
            kind,
            x,
            y,
            button,
            alt,
            ctrl,
            shift,
        } => {
            let link = link_for(state, &session).await?;
            let event = build_mouse_event(&kind, x, y, button.as_deref(), alt, ctrl, shift)?;
            link.run_action(Action::MouseEvent { event }).await?;
            Ok(json!({ "x": x, "y": y, "kind": kind }))
        },

        // ---- screen -----------------------------------------------------
        Command::ScreenSnapshot { session, pane_id } => {
            let link = link_for(state, &session).await?;
            let pane = resolve_pane(&link, pane_id).await?;
            let pane_id = pane.to_string();
            let (version, lines) = link.snapshot(&pane_id).ok_or_else(|| {
                format!(
                    "no canvas for pane '{}' — subscribe to it first",
                    pane_id
                )
            })?;
            Ok(json!({ "pane_id": pane_id, "version": version, "lines": lines }))
        },
        Command::ScreenHistory {
            session,
            pane_id,
            since,
            limit,
        } => {
            let link = link_for(state, &session).await?;
            let pane = resolve_pane(&link, pane_id).await?;
            let pane_id = pane.to_string();
            let history = link.history(&pane_id, since, limit);
            Ok(json!({ "pane_id": pane_id, "diffs": history }))
        },
        Command::ScreenSubscribe { session, pane_ids } => {
            let link = link_for(state, &session).await?;
            let targets = match pane_ids {
                Some(ids) => Some(
                    ids.iter()
                        .map(|id| parse_pane_id(id))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                None => None,
            };
            let follow_all = targets.is_none();
            let subscribed = link.subscribe(targets).await?;

            streams.stop(&session);
            let mut handles = vec![forward_events(link.clone(), out.clone())];
            if follow_all {
                handles.push(poll_pane_set(link.clone()));
            }
            streams.tasks.insert(session.clone(), handles);

            Ok(json!({
                "session": session,
                "panes": subscribed.iter().map(|p| p.to_string()).collect::<Vec<_>>(),
                "following_new_panes": follow_all,
            }))
        },
        Command::ScreenUnsubscribe { session } => {
            if let Some(link) = state.sessions.existing_link(&session) {
                link.unsubscribe();
            }
            streams.stop(&session);
            Ok(json!({ "session": session }))
        },
    }
}

/// Pump one session's events into this connection's outbound channel.
fn forward_events(
    link: Arc<SessionLink>,
    out: mpsc::UnboundedSender<String>,
) -> JoinHandle<()> {
    let mut events = link.events();
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(event) => send_json(&out, &event),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    // Tell the caller rather than silently dropping screen
                    // changes: they can resync with screen.snapshot.
                    log::warn!("api client lagged, dropped {} events", skipped);
                    send_json(
                        &out,
                        &json!({
                            "event": "stream.lagged",
                            "session": link.name,
                            "dropped": skipped,
                        }),
                    );
                },
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

/// While following a whole session, keep the subscribed pane set current.
fn poll_pane_set(link: Arc<SessionLink>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(PANE_POLL_INTERVAL);
        loop {
            ticker.tick().await;
            if let Err(e) = link.refresh_pane_set().await {
                // Only a session that has gone away ends the follow. A single
                // failed query used to stop it for good, silently: the caller
                // kept its subscription but new panes were never picked up
                // again.
                if !link.is_alive() {
                    log::debug!("stopped following panes of '{}': {}", link.name, e);
                    break;
                }
                log::debug!("could not refresh panes of '{}': {}", link.name, e);
            }
        }
    })
}

/// Open (or reuse) a session link, ready to serve focus-dependent commands.
async fn link_for(state: &ApiState, session: &str) -> Result<Arc<SessionLink>, String> {
    // `SessionManager::link` is synchronous and, the first time it links a
    // given session (or relinks after the old link died), does real
    // blocking work: three socket connects plus handshakes, and
    // `learn_own_client_id`'s poll loop (up to 10 attempts, 100ms apart —
    // up to a full second). Called directly from this `async fn` with no
    // `spawn_blocking`, that work would run straight on a Tokio worker
    // thread, blocking it — and every other task scheduled on that same
    // thread — for the same duration. `spawn_blocking` moves it to the
    // blocking thread pool instead, the same treatment already given
    // `sessions.create`/`sessions.kill` for the same reason.
    let sessions = state.sessions.clone();
    let session_name = session.to_string();
    let link = tokio::time::timeout(
        CALL_TIMEOUT,
        tokio::task::spawn_blocking(move || sessions.link(&session_name)),
    )
    .await
    .map_err(|_| format!("timed out linking to session '{}'", session))?
    .map_err(|e| format!("linking to session panicked: {}", e))??;
    link.ensure_focus_ready().await?;
    Ok(link)
}

/// Work out which pane an input command is aimed at.
///
/// A named pane is checked for existence first. Writing to a pane id the
/// session does not have is silently discarded by the server — and a pane can
/// disappear at any moment (closing a plugin's UI closes its pane), so a caller
/// that keeps typing at a stale id would otherwise get a stream of successful
/// replies and no effect at all.
async fn resolve_pane(link: &Arc<SessionLink>, pane_id: Option<String>) -> Result<PaneId, String> {
    match pane_id {
        Some(id) => {
            let pane = parse_pane_id(&id)?;
            if !link.pane_exists(pane).await? {
                return Err(format!("session '{}' has no pane {}", link.name, id));
            }
            Ok(pane)
        },
        None => link.focused_pane().await,
    }
}

fn parse_pane_id(raw: &str) -> Result<PaneId, String> {
    PaneId::from_str(raw).map_err(|e| format!("invalid pane id '{}': {}", raw, e))
}

fn parse_direction(raw: Option<&str>) -> Result<Option<Direction>, String> {
    match raw {
        Some(value) => Direction::from_str(value).map(Some),
        None => Ok(None),
    }
}

fn build_mouse_event(
    kind: &str,
    x: u16,
    y: u16,
    button: Option<&str>,
    alt: bool,
    ctrl: bool,
    shift: bool,
) -> Result<MouseEvent, String> {
    let event_type = match kind.to_lowercase().as_str() {
        "press" | "down" | "click" => MouseEventType::Press,
        "release" | "up" => MouseEventType::Release,
        "motion" | "move" | "drag" => MouseEventType::Motion,
        other => return Err(format!("unknown mouse event kind '{}'", other)),
    };

    let mut event = MouseEvent::new();
    event.event_type = event_type;
    // Zellij positions are (line, column), and both are 0-based like the
    // coordinates a caller reads out of a pane's geometry.
    event.position = Position::new(y as i32, x);
    event.alt = alt;
    event.ctrl = ctrl;
    event.shift = shift;

    match button.unwrap_or("left").to_lowercase().as_str() {
        "left" => event.left = true,
        "right" => event.right = true,
        "middle" => event.middle = true,
        "wheel_up" | "scroll_up" => event.wheel_up = true,
        "wheel_down" | "scroll_down" => event.wheel_down = true,
        "none" => {},
        other => return Err(format!("unknown mouse button '{}'", other)),
    }

    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison_accepts_only_the_exact_token() {
        assert!(tokens_match("s3cret", "s3cret"));
        assert!(!tokens_match("s3cret", "s3creT"));
        assert!(!tokens_match("s3cret", ""));
        assert!(!tokens_match("s3cret", "s3cre"), "a prefix is not enough");
        assert!(
            !tokens_match("s3cret", "s3cretx"),
            "a longer string is not enough"
        );
        assert!(tokens_match("", ""));
    }

    #[test]
    fn mouse_coordinates_map_to_line_and_column() {
        let event = build_mouse_event("press", 12, 5, Some("left"), false, false, false).unwrap();
        assert_eq!(event.event_type, MouseEventType::Press);
        assert!(event.left);
        assert_eq!(event.position.column.0, 12, "x is the column");
        assert_eq!(event.position.line.0, 5, "y is the line");
    }

    #[test]
    fn mouse_defaults_to_the_left_button() {
        let event = build_mouse_event("press", 0, 0, None, false, false, false).unwrap();
        assert!(event.left);
        assert!(!event.right && !event.middle);
    }

    #[test]
    fn mouse_supports_wheel_and_modifiers() {
        let event =
            build_mouse_event("motion", 1, 2, Some("wheel_down"), true, true, false).unwrap();
        assert!(event.wheel_down);
        assert!(event.alt && event.ctrl && !event.shift);
        assert_eq!(event.event_type, MouseEventType::Motion);
    }

    #[test]
    fn unknown_mouse_input_is_rejected() {
        assert!(build_mouse_event("wiggle", 0, 0, None, false, false, false).is_err());
        assert!(build_mouse_event("press", 0, 0, Some("pinky"), false, false, false).is_err());
    }

    #[test]
    fn pane_ids_accept_both_forms() {
        assert_eq!(parse_pane_id("terminal_2").unwrap(), PaneId::Terminal(2));
        assert_eq!(parse_pane_id("plugin_9").unwrap(), PaneId::Plugin(9));
        // A bare number is a terminal pane, matching the CLI.
        assert_eq!(parse_pane_id("4").unwrap(), PaneId::Terminal(4));
        assert!(parse_pane_id("banana").is_err());
    }

    #[test]
    fn directions_are_optional_but_validated() {
        assert_eq!(parse_direction(None).unwrap(), None);
        assert_eq!(parse_direction(Some("left")).unwrap(), Some(Direction::Left));
        assert!(parse_direction(Some("sideways")).is_err());
    }

    #[test]
    fn resize_accepts_the_documented_spellings() {
        assert_eq!(Resize::from_str("increase").unwrap(), Resize::Increase);
        assert_eq!(Resize::from_str("decrease").unwrap(), Resize::Decrease);
        assert!(Resize::from_str("bigger").is_err());
    }
}
