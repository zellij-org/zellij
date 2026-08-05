//! A live link to one running Zellij session.
//!
//! Two IPC connections are opened per session, each with its own reader thread
//! (`ClientOsApi` is blocking, so it cannot live on the async runtime):
//!
//! - the **control** connection dispatches `Action`s one at a time and reads
//!   back the server's reply (`Log` for queries, `UnblockInputThread` for
//!   fire-and-forget commands);
//! - the **observer** connection carries only the `SubscribeToPaneRenders`
//!   stream, so unsolicited screen updates never interleave with command
//!   replies.

use std::collections::HashSet;
use std::str::FromStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{broadcast, oneshot};

use zellij_client::os_input_output::{get_cli_client_os_input, ClientOsApi};
use zellij_utils::data::{ClientId, ListPanesResponse, ListTabsResponse, PaneId, TabInfo};
use zellij_utils::input::actions::Action;
use zellij_utils::input::cli_assets::CliAssets;
use zellij_utils::ipc::{ClientToServerMsg, ServerToClientMsg};
use zellij_utils::pane_size::Size;

use crate::canvas::{CanvasStore, CanvasUpdate, HistoryEntry};
use crate::protocol::Event;

/// How long a caller waits for a command reply before giving up.
pub const ACTION_TIMEOUT: Duration = Duration::from_secs(10);
/// How often we re-check which panes exist, so that panes opened after a
/// subscription are picked up automatically.
pub const PANE_POLL_INTERVAL: Duration = Duration::from_millis(750);
/// Buffered events per subscriber before the slowest one starts losing frames.
const EVENT_BUFFER: usize = 4096;

type ActionReply = Result<Vec<String>, String>;

enum LinkRequest {
    Run {
        action: Action,
        reply: oneshot::Sender<ActionReply>,
    },
    Shutdown,
}

#[derive(Debug, Default)]
struct Subscription {
    /// When true, every pane in the session is followed, including new ones.
    follow_all: bool,
    pane_ids: HashSet<PaneId>,
}

pub struct SessionLink {
    pub name: String,
    requests: std_mpsc::Sender<LinkRequest>,
    canvas: Arc<Mutex<CanvasStore>>,
    subscription: Arc<Mutex<Subscription>>,
    /// Clone of the observer connection, used to (re)send subscription requests.
    observer: Arc<Box<dyn ClientOsApi>>,
    /// A real attached client, so the session has somewhere to hold focus.
    focus: Arc<Box<dyn ClientOsApi>>,
    /// Whether that client has been seen holding focus.
    focus_ready: AtomicBool,
    /// Cleared by the reader threads when the session goes away.
    alive: Arc<AtomicBool>,
    /// The last pane we focused. Used as a fallback for picking our client out
    /// among several attached ones, when `own_client_id` could not be
    /// determined.
    last_focused: Mutex<Option<PaneId>>,
    /// The focus client's own id, learned once at connect time (see
    /// `learn_own_client_id`) by diffing the attached-client list before and
    /// after attaching. `None` if that could not be determined — commands
    /// depending on it fall back to `last_focused`-based heuristics.
    own_client_id: Option<ClientId>,
    events: broadcast::Sender<Event>,
}

impl std::fmt::Debug for SessionLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionLink").field("name", &self.name).finish()
    }
}

pub fn session_socket_path(session_name: &str) -> PathBuf {
    let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
    let _ = std::fs::create_dir_all(&sock_dir);
    let _ = zellij_utils::shared::set_permissions(&sock_dir, 0o700);
    sock_dir.push(session_name);
    sock_dir
}

/// Whether a session's socket is up, tolerating the moment right after a
/// rename.
///
/// `session_exists` scans the socket directory and, for each entry, connects
/// and round-trips a status message — and just after `RenameSession` finishes
/// on the server (the directory entry is already renamed; that part is
/// synchronous), a probe aimed at the new name can still see it as absent. It
/// resolves within about a second in practice. Reproduced directly: creating a
/// session, renaming it, and killing it immediately by the new name fails
/// roughly one time in three without this retry, even though the session is
/// verifiably running the whole time. Five short attempts is comfortably past
/// what was needed to stop reproducing it, while staying well under what a
/// caller would notice as a hang.
pub fn session_exists_settled(session_name: &str) -> Result<bool, String> {
    use std::time::Duration;
    for attempt in 0..5 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(150));
        }
        match zellij_utils::sessions::session_exists(session_name) {
            Ok(true) => return Ok(true),
            Ok(false) => continue,
            Err(e) => return Err(format!("could not check session '{}': {:?}", session_name, e)),
        }
    }
    Ok(false)
}

fn connect(session_name: &str) -> Result<Box<dyn ClientOsApi>, String> {
    // `connect_to_server` retries forever, so refuse up front rather than
    // wedging a thread on a session that is not there.
    if !session_exists_settled(session_name)? {
        return Err(format!("session '{}' is not running", session_name));
    }
    let os_input =
        get_cli_client_os_input().map_err(|e| format!("could not open client IPC: {}", e))?;
    let path = session_socket_path(session_name);
    os_input.connect_to_server(&path);
    Ok(Box::new(os_input))
}

/// Parse `ListClients`' table into `(client_id, pane_id)` pairs.
///
/// ```text
/// CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND
/// 3         terminal_1     N/A
/// ```
fn parse_client_table(lines: &[String]) -> Vec<(ClientId, PaneId)> {
    lines
        .iter()
        .flat_map(|line| line.lines())
        .skip_while(|line| line.trim_start().starts_with("CLIENT_ID"))
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let client_id: ClientId = columns.next()?.parse().ok()?;
            let pane_id = PaneId::from_str(columns.next()?).ok()?;
            Some((client_id, pane_id))
        })
        .collect()
}

/// The attached client ids of a session, via a disposable connection.
///
/// Used only to learn the focus client's own id (see
/// `SessionLink::connect`): open, ask, close. Not folded into `SessionLink`
/// itself because it is needed *before* one exists — this is how the focus
/// client's own id is discovered in the first place.
fn list_client_ids(session_name: &str) -> Result<HashSet<ClientId>, String> {
    let os_input = connect(session_name)?;
    os_input.send_to_server(ClientToServerMsg::Action {
        action: Action::ListClients,
        terminal_id: None,
        client_id: None,
        is_cli_client: true,
    });
    let lines = read_action_reply(&*os_input, ReplyShape::Text)?;
    os_input.send_to_server(ClientToServerMsg::ClientExited);
    Ok(parse_client_table(&lines).into_iter().map(|(id, _)| id).collect())
}

/// Learn the focus client's own id by elimination: after attaching, exactly
/// one new client id should appear that was not in `before_attach`.
///
/// If a real client attaches in that same narrow window — a genuine race,
/// not one we can fully close — more than one new id can appear at once. We
/// do not guess in that case: we give up and leave the id unknown, and focus
/// resolution falls back to the `last_focused`-based heuristic instead of
/// risking attribution to the wrong client.
fn learn_own_client_id(session_name: &str, before_attach: &HashSet<ClientId>) -> Option<ClientId> {
    for attempt in 0..10 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        let Ok(after_attach) = list_client_ids(session_name) else {
            continue;
        };
        let new_ids: Vec<ClientId> = after_attach.difference(before_attach).copied().collect();
        if let [only] = new_ids.as_slice() {
            return Some(*only);
        }
    }
    None
}

impl SessionLink {
    /// Open the connections to `session_name` and start their reader threads.
    pub fn connect(session_name: &str) -> Result<Arc<Self>, String> {
        // Snapshotted before we attach our own focus client, so that client's
        // id can be learned by elimination once it appears (see below) — the
        // protocol never tells a client its own id directly.
        let before_attach = list_client_ids(session_name).unwrap_or_default();

        let control = connect(session_name)?;
        let observer = connect(session_name)?;
        let focus = connect(session_name)?;

        let (requests_tx, requests_rx) = std_mpsc::channel::<LinkRequest>();
        let (events_tx, _) = broadcast::channel(EVENT_BUFFER);
        let canvas = Arc::new(Mutex::new(CanvasStore::default()));
        let subscription = Arc::new(Mutex::new(Subscription::default()));
        let alive = Arc::new(AtomicBool::new(true));

        spawn_control_thread(
            session_name.to_string(),
            control,
            requests_rx,
            events_tx.clone(),
            alive.clone(),
        );
        let observer = Arc::new(observer);
        spawn_observer_thread(
            session_name.to_string(),
            observer.clone(),
            canvas.clone(),
            events_tx.clone(),
            alive.clone(),
        );

        let focus = Arc::new(focus);
        attach_focus_client(&**focus);
        spawn_focus_thread(session_name.to_string(), focus.clone());
        let own_client_id = learn_own_client_id(session_name, &before_attach);

        Ok(Arc::new(SessionLink {
            name: session_name.to_string(),
            requests: requests_tx,
            canvas,
            subscription,
            observer,
            focus,
            focus_ready: AtomicBool::new(false),
            alive,
            last_focused: Mutex::new(None),
            own_client_id,
            events: events_tx,
        }))
    }

    /// Run an action *as the session's attached client*.
    ///
    /// Client-relative operations — focusing a tab or pane, splitting next to
    /// the current pane — are resolved by the server against the requesting
    /// client. Sending them over the CLI connection does not work: a CLI
    /// message is attributed to the *last active client*, which is only ever
    /// set by a keystroke, so on an API-driven session it is nobody. Sent over
    /// the attached connection with `is_cli_client: false`, the server
    /// attributes the action to that client, and focus behaves exactly as it
    /// does for a human.
    ///
    /// The reply is not read here: this socket also carries the render stream,
    /// and callers that need certainty verify the resulting state with a query
    /// instead of trusting an acknowledgement.
    pub fn run_as_client(&self, action: Action) {
        self.focus.send_to_server(ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }

    /// Subscribe to this session's event stream.
    pub fn events(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// Dispatch an action and wait for the server's reply.
    ///
    /// The reply is whatever the server logged back, which for the `List*`
    /// actions is a single JSON document.
    pub async fn run_action(&self, action: Action) -> Result<Vec<String>, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.requests
            .send(LinkRequest::Run {
                action,
                reply: reply_tx,
            })
            .map_err(|_| format!("session '{}' link is closed", self.name))?;

        match tokio::time::timeout(ACTION_TIMEOUT, reply_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(format!("session '{}' link dropped the reply", self.name)),
            Err(_) => {
                // The control thread is still blocked reading a reply that has
                // not come, and it reads replies in order — so every later
                // command on this link would time out too. Retire the link:
                // the next caller opens a fresh one, and closing this one
                // unblocks the stuck thread.
                self.alive.store(false, Ordering::Relaxed);
                Err(format!(
                    "timed out after {:?} waiting for session '{}'",
                    ACTION_TIMEOUT, self.name
                ))
            },
        }
    }

    /// Run an action whose reply is a JSON document, and deserialize it.
    pub async fn query<T: serde::de::DeserializeOwned>(&self, action: Action) -> Result<T, String> {
        let lines = self.run_action(action).await?;
        let body = lines.join("\n");
        if body.trim().is_empty() {
            return Err("server returned an empty response".to_string());
        }
        serde_json::from_str(&body)
            .map_err(|e| format!("could not parse server response: {} (was: {})", e, body))
    }

    pub async fn list_tabs(&self) -> Result<ListTabsResponse, String> {
        self.query(Action::ListTabs {
            show_state: true,
            show_dimensions: false,
            show_panes: true,
            show_layout: false,
            show_all: false,
            output_json: true,
        })
        .await
    }

    pub async fn list_panes(&self) -> Result<ListPanesResponse, String> {
        self.query(Action::ListPanes {
            show_tab: true,
            show_command: true,
            show_state: true,
            show_geometry: true,
            show_all: false,
            output_json: true,
        })
        .await
    }

    /// Which pane each attached client is focused on.
    ///
    /// This is the only authoritative answer to "what is focused": the
    /// `is_focused` flag on a pane means "focused within its layer", so several
    /// panes carry it at once (a tiled pane and a floating one, in every tab).
    /// `ListClients` reports the single pane each client actually sits on.
    ///
    /// It answers with a table rather than JSON, hence the parsing:
    ///
    /// ```text
    /// CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND
    /// 3         terminal_1     N/A
    /// ```
    pub async fn attached_clients(&self) -> Result<Vec<(ClientId, PaneId)>, String> {
        let lines = self.run_action(Action::ListClients).await?;
        Ok(parse_client_table(&lines))
    }

    /// The pane the session considers focused — the default target for input.
    ///
    /// Prefers what a *real* client is actually on — a human who attached
    /// with `zellij attach`, distinguished from our own focus client by
    /// `own_client_id` (see `learn_own_client_id`). This is what makes
    /// unaddressed input follow a person who is interacting live, rather than
    /// staying wherever the API last pointed its own client: if you switch
    /// panes yourself, the next `input.text` with no `pane_id` goes where you
    /// are, not where we last put things.
    ///
    /// Falls back to reading the pane flags, which is ambiguous but better
    /// than nothing if the client listing is unavailable.
    pub async fn focused_pane(&self) -> Result<PaneId, String> {
        if let Ok(clients) = self.attached_clients().await {
            let focused = match clients.len() {
                1 => Some(clients[0].1),
                0 => None,
                _ => {
                    // More than one client: prefer a real one — anyone whose
                    // id is not ours — over our own client's last known
                    // position. If our own id was never resolved (see
                    // `learn_own_client_id`), fall back to the old heuristic
                    // rather than guessing which client is "the human".
                    match self.own_client_id {
                        Some(own) => clients
                            .iter()
                            .find(|(id, _)| *id != own)
                            .map(|(_, pane)| *pane)
                            .or_else(|| clients.iter().find(|(id, _)| *id == own).map(|(_, p)| *p)),
                        None => {
                            let ours = *self.last_focused.lock().unwrap();
                            ours.filter(|ours| clients.iter().any(|(_, p)| p == ours))
                        },
                    }
                },
            };
            if let Some(focused) = focused {
                return Ok(focused);
            }
        }
        self.focused_pane_from_flags().await
    }

    async fn focused_pane_from_flags(&self) -> Result<PaneId, String> {
        let active_tab_id = self.active_tab().await?.tab_id;
        let panes = self.list_panes().await?;
        let in_active_tab: Vec<_> = panes
            .iter()
            .filter(|p| p.tab_id == active_tab_id && !p.pane_info.is_suppressed)
            .collect();

        in_active_tab
            .iter()
            .find(|p| p.pane_info.is_focused && !p.pane_info.is_plugin)
            .or_else(|| in_active_tab.iter().find(|p| !p.pane_info.is_plugin))
            .or_else(|| in_active_tab.iter().find(|p| p.pane_info.is_focused))
            .map(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin))
            .ok_or_else(|| {
                format!(
                    "session '{}' has no focused pane in its active tab",
                    self.name
                )
            })
    }

    /// The tab commands act on by default.
    ///
    /// A tab is `active` relative to a *client*, and a session driven purely
    /// over the API usually has no client attached — so no tab is marked
    /// active. In that case the first tab is the only sensible target, and it
    /// is what a caller means by "the session's tab".
    pub async fn active_tab(&self) -> Result<TabInfo, String> {
        let tabs = self.list_tabs().await?;
        tabs.iter()
            .find(|t| t.active)
            .or_else(|| tabs.iter().min_by_key(|t| t.position))
            .cloned()
            .ok_or_else(|| format!("session '{}' has no tabs", self.name))
    }

    /// Wait until the session's attached client holds focus.
    ///
    /// Attaching is asynchronous, and until the server has registered the
    /// client no tab is focused — so a command issued in that window (a
    /// directional split, say) has nothing to resolve against. Focus appearing
    /// on some tab is the signal that the client is live.
    pub async fn ensure_focus_ready(&self) -> Result<(), String> {
        if self.focus_ready.load(Ordering::Relaxed) {
            return Ok(());
        }
        for attempt in 0..20 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            if let Ok(tabs) = self.list_tabs().await {
                if tabs.iter().any(|t| t.active) {
                    self.focus_ready.store(true, Ordering::Relaxed);
                    return Ok(());
                }
            }
        }
        // Not fatal on its own — commands that do not depend on focus still
        // work, and those that do report their own, more specific failure.
        log::warn!(
            "session '{}' never reported a focused tab; focus-dependent commands may fail",
            self.name
        );
        Ok(())
    }

    /// Poll `check` against fresh session state until it holds.
    ///
    /// Focus changes are applied asynchronously inside the session, so a
    /// command that sets focus confirms the result rather than assuming it.
    async fn settle<T, F>(&self, what: &str, mut check: F) -> Result<T, String>
    where
        F: FnMut(&ListTabsResponse, &ListPanesResponse) -> Option<T>,
    {
        let mut last_error = None;
        for attempt in 0..12 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            match (self.list_tabs().await, self.list_panes().await) {
                (Ok(tabs), Ok(panes)) => {
                    if let Some(found) = check(&tabs, &panes) {
                        return Ok(found);
                    }
                },
                (Err(e), _) | (_, Err(e)) => last_error = Some(e),
            }
        }
        Err(match last_error {
            Some(e) => format!("{} (last error: {})", what, e),
            None => what.to_string(),
        })
    }

    /// Check a tab exists, so acting on a tab id that is not there fails at
    /// once and by name — rather than after waiting for a focus change that was
    /// never going to happen.
    pub async fn require_tab(&self, tab_id: usize) -> Result<(), String> {
        let tabs = self.list_tabs().await?;
        if tabs.iter().any(|t| t.tab_id == tab_id) {
            return Ok(());
        }
        let known: Vec<String> = tabs.iter().map(|t| t.tab_id.to_string()).collect();
        Err(format!(
            "session '{}' has no tab {} (known tab ids: {})",
            self.name,
            tab_id,
            if known.is_empty() { "none".to_string() } else { known.join(", ") }
        ))
    }

    /// Focus a tab and wait until the session agrees it is focused.
    pub async fn focus_tab(&self, tab_id: usize) -> Result<usize, String> {
        let tabs = self.list_tabs().await?;
        if !tabs.iter().any(|t| t.tab_id == tab_id) {
            return self.require_tab(tab_id).await.map(|_| tab_id);
        }
        if tabs.iter().any(|t| t.tab_id == tab_id && t.active) {
            return Ok(tab_id);
        }
        self.run_as_client(Action::GoToTabById { id: tab_id as u64 });
        self.await_tab_focus(tab_id).await
    }

    /// Focus a pane and wait until the session agrees it is focused.
    pub async fn focus_pane(&self, pane_id: PaneId) -> Result<(), String> {
        self.run_as_client(Action::FocusPaneByPaneId { pane_id });
        self.await_pane_focus(pane_id).await?;
        *self.last_focused.lock().unwrap() = Some(pane_id);
        Ok(())
    }

    /// Wait until `tab_id` is the session's focused tab.
    pub async fn await_tab_focus(&self, tab_id: usize) -> Result<usize, String> {
        self.settle("the session did not focus the tab", |tabs, _| {
            tabs.iter()
                .find(|t| t.tab_id == tab_id && t.active)
                .map(|t| t.tab_id)
        })
        .await
    }

    /// Wait until the attached client is sitting on `pane_id`.
    pub async fn await_pane_focus(&self, pane_id: PaneId) -> Result<(), String> {
        for attempt in 0..12 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if let Ok(clients) = self.attached_clients().await {
                // `focus_pane` moves only our own client — `run_as_client`
                // sends `FocusPaneByPaneId` as that client specifically — so
                // this should confirm *our* client reached `pane_id`, not
                // merely that some client (possibly a human, already sitting
                // there by coincidence) is present. When our id is unknown,
                // fall back to "any client", matching the old behavior.
                let reached = match self.own_client_id {
                    Some(own) => clients.iter().any(|(id, p)| *id == own && *p == pane_id),
                    None => clients.iter().any(|(_, p)| *p == pane_id),
                };
                if reached {
                    return Ok(());
                }
            }
        }
        Err(format!(
            "the session did not focus pane {} (no attached client moved to it)",
            pane_id
        ))
    }

    /// The ids of every pane currently in the session.
    pub async fn pane_ids_now(&self) -> Result<HashSet<String>, String> {
        Ok(self
            .list_panes()
            .await?
            .iter()
            .map(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin).to_string())
            .collect())
    }

    /// Wait for a pane that was not in `before` to appear, and return its id.
    pub async fn await_new_pane(&self, before: &HashSet<String>) -> Result<String, String> {
        for attempt in 0..12 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            let now = self.pane_ids_now().await?;
            if let Some(created) = now.difference(before).next() {
                return Ok(created.clone());
            }
        }
        Err("the session did not create the pane".to_string())
    }

    /// Whether the session has this pane right now.
    ///
    /// A single check — for a pane that may still be on its way, see
    /// [`Self::await_new_pane`].
    pub async fn pane_exists(&self, pane_id: PaneId) -> Result<bool, String> {
        Ok(self
            .list_panes()
            .await?
            .iter()
            .any(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin) == pane_id))
    }

    /// A terminal pane in `tab_id` to use as the reference for a positional
    /// action, preferring the tab's focused one.
    pub async fn reference_pane_in_tab(&self, tab_id: usize) -> Result<u32, String> {
        let panes = self.list_panes().await?;
        let in_tab: Vec<_> = panes
            .iter()
            .filter(|p| {
                p.tab_id == tab_id && !p.pane_info.is_plugin && !p.pane_info.is_suppressed
            })
            .collect();
        in_tab
            .iter()
            .find(|p| p.pane_info.is_focused)
            .or_else(|| in_tab.first())
            .map(|p| p.pane_info.id)
            .ok_or_else(|| {
                format!(
                    "tab {} of session '{}' has no terminal pane to place a pane next to",
                    tab_id, self.name
                )
            })
    }

    /// Start following pane renders. `pane_ids` of `None` means every pane in
    /// the session, now and in future.
    pub async fn subscribe(&self, pane_ids: Option<Vec<PaneId>>) -> Result<Vec<PaneId>, String> {
        let targets = match pane_ids {
            Some(ids) => {
                // Check the panes exist before asking to follow them. The
                // server answers an unknown pane with an error on the observer
                // socket — which carries no replies, so nothing would surface
                // it — and installs no subscription at all if none of the panes
                // were found, leaving any previous subscription in place while
                // we believe we replaced it.
                let live: HashSet<PaneId> = self
                    .list_panes()
                    .await?
                    .iter()
                    .map(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin))
                    .collect();
                let unknown: Vec<String> = ids
                    .iter()
                    .filter(|id| !live.contains(id))
                    .map(|id| id.to_string())
                    .collect();
                if !unknown.is_empty() {
                    return Err(format!(
                        "session '{}' has no pane {}",
                        self.name,
                        unknown.join(", ")
                    ));
                }
                let mut sub = self.subscription.lock().unwrap();
                sub.follow_all = false;
                sub.pane_ids = ids.iter().copied().collect();
                ids
            },
            None => {
                let panes = self.list_panes().await?;
                let ids: Vec<PaneId> = panes
                    .iter()
                    .map(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin))
                    .collect();
                let mut sub = self.subscription.lock().unwrap();
                sub.follow_all = true;
                sub.pane_ids = ids.iter().copied().collect();
                ids
            },
        };

        if targets.is_empty() {
            return Err("no panes to subscribe to".to_string());
        }
        self.send_subscription(&targets);
        Ok(targets)
    }

    pub fn unsubscribe(&self) {
        let mut sub = self.subscription.lock().unwrap();
        sub.follow_all = false;
        sub.pane_ids.clear();
        // The server drops a subscription when its panes close; there is no
        // explicit unsubscribe message, so we simply stop tracking and let the
        // observer discard what still arrives.
    }

    fn send_subscription(&self, pane_ids: &[PaneId]) {
        self.observer
            .send_to_server(ClientToServerMsg::SubscribeToPaneRenders {
                pane_ids: pane_ids.to_vec(),
                scrollback: None,
                ansi: false,
            });
    }

    /// Refresh the followed pane set. Called periodically while following all
    /// panes, so panes opened after `subscribe` are picked up.
    pub async fn refresh_pane_set(&self) -> Result<(), String> {
        let follow_all = self.subscription.lock().unwrap().follow_all;
        if !follow_all {
            return Ok(());
        }
        let panes = self.list_panes().await?;
        let current: HashSet<PaneId> = panes
            .iter()
            .map(|p| pane_id_of(&p.pane_info.id, p.pane_info.is_plugin))
            .collect();

        let (added, removed) = {
            let sub = self.subscription.lock().unwrap();
            let added: Vec<PaneId> = current.difference(&sub.pane_ids).copied().collect();
            let removed: Vec<PaneId> = sub.pane_ids.difference(&current).copied().collect();
            (added, removed)
        };

        if added.is_empty() && removed.is_empty() {
            return Ok(());
        }

        for pane_id in &added {
            let _ = self.events.send(Event::PaneOpened {
                session: self.name.clone(),
                pane_id: pane_id.to_string(),
            });
        }
        for pane_id in &removed {
            self.canvas.lock().unwrap().forget(&pane_id.to_string());
            let _ = self.events.send(Event::PaneClosed {
                session: self.name.clone(),
                pane_id: pane_id.to_string(),
            });
        }

        {
            let mut sub = self.subscription.lock().unwrap();
            sub.pane_ids = current.clone();
        }
        let targets: Vec<PaneId> = current.into_iter().collect();
        if !targets.is_empty() {
            self.send_subscription(&targets);
        }
        Ok(())
    }

    pub fn snapshot(&self, pane_id: &str) -> Option<(u64, Vec<String>)> {
        self.canvas.lock().unwrap().snapshot(pane_id)
    }

    pub fn history(
        &self,
        pane_id: &str,
        since: Option<u64>,
        limit: Option<usize>,
    ) -> Vec<HistoryEntry> {
        self.canvas.lock().unwrap().history(pane_id, since, limit)
    }

    /// Whether the session behind this link is still there.
    ///
    /// The reader threads clear this when the server sends `Exit` or the socket
    /// closes, so a link to a session that has ended is not handed out again.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }

    /// Close this link and stop its threads.
    ///
    /// The observer and focus threads sit blocked in `recv_from_server`, so
    /// they cannot be asked to stop — telling the server we have exited closes
    /// those connections from the other end, which unblocks them. Without this
    /// they would outlive the link, and the focus client would stay attached to
    /// a session we no longer drive.
    pub fn shutdown(&self) {
        self.alive.store(false, Ordering::Relaxed);
        let _ = self.requests.send(LinkRequest::Shutdown);
        self.observer.send_to_server(ClientToServerMsg::ClientExited);
        self.focus.send_to_server(ClientToServerMsg::ClientExited);
    }
}

pub fn pane_id_of(id: &u32, is_plugin: bool) -> PaneId {
    if is_plugin {
        PaneId::Plugin(*id)
    } else {
        PaneId::Terminal(*id)
    }
}

fn spawn_control_thread(
    session_name: String,
    os_input: Box<dyn ClientOsApi>,
    requests: std_mpsc::Receiver<LinkRequest>,
    events: broadcast::Sender<Event>,
    alive: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name(format!("zellij-api-control-{}", session_name))
        .spawn(move || {
            while let Ok(request) = requests.recv() {
                let LinkRequest::Run { action, reply } = request else {
                    break;
                };

                let shape = reply_shape(&action);
                os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id: None,
                    is_cli_client: true,
                });

                let result = read_action_reply(&*os_input, shape);
                let session_ended = matches!(&result, Err(e) if e == SESSION_GONE);
                let _ = reply.send(result);

                if session_ended {
                    alive.store(false, Ordering::Relaxed);
                    let _ = events.send(Event::SessionEnded {
                        session: session_name.clone(),
                    });
                    break;
                }
            }
            os_input.send_to_server(ClientToServerMsg::ClientExited);
        })
        .expect("could not spawn the session control thread");
}

const SESSION_GONE: &str = "session ended";

/// What a given action's reply looks like on the wire.
///
/// The IPC stream carries no request ids, so replies are matched by position —
/// and two server behaviours break naive positional matching:
///
/// - `UnblockInputThread` is not one-per-action: the server emits extra ones
///   (a broadcast unblock reaches every client, ours included), so counting
///   them shifts every later reply by one;
/// - some actions log more than once (`NewTab` reports both the new tab's id
///   and the new pane's id), leaving a spare `Log` in the stream.
///
/// So each action declares the shape of its answer, and the reader skips
/// anything that cannot be it. Queries ask for JSON, which no stray line from
/// another action ever parses as — that makes them self-resynchronising.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReplyShape {
    /// A `Log` whose body is a JSON document.
    Json,
    /// A `Log` of plain text (an id, for example).
    Text,
    /// No output — the action just has to complete.
    Ack,
}

fn reply_shape(action: &Action) -> ReplyShape {
    match action {
        Action::ListPanes { .. }
        | Action::ListTabs { .. }
        | Action::CurrentTabInfo { .. }
        | Action::DumpLayout => ReplyShape::Json,
        // `ListClients` has no JSON form — it answers with a table.
        Action::ListClients => ReplyShape::Text,
        Action::NewTiledPane { .. }
        | Action::NewFloatingPane { .. }
        | Action::NewStackedPane { .. }
        | Action::NewInPlacePane { .. } => ReplyShape::Text,
        // `NewTab` does log ids, but we re-query the tab list afterwards
        // rather than depend on which of its two logs arrives first.
        _ => ReplyShape::Ack,
    }
}

/// Read the server's reply to the action we just sent.
///
/// `UnblockInputThread` cannot be treated as a per-action terminator — the
/// server emits more of them than there are actions, and counting them shifts
/// every later reply by one. So:
///
/// - an action that produces output is read until its `Log` (or `LogError`)
///   arrives, ignoring unblocks;
/// - an action that only completes returns at the first unblock, and ignores a
///   stray `Log` left over from an earlier action.
fn read_action_reply(os_input: &dyn ClientOsApi, shape: ReplyShape) -> ActionReply {
    loop {
        match os_input.recv_from_server() {
            Some((ServerToClientMsg::Log { lines }, _)) => match shape {
                ReplyShape::Text => return Ok(lines),
                ReplyShape::Json => {
                    // Skip leftovers from an earlier action. Every query
                    // answers with a JSON array or object, so a bare scalar —
                    // `NewTab` logging a tab id of `1`, say — is not it.
                    let is_query_result = serde_json::from_str::<serde_json::Value>(
                        &lines.join("\n"),
                    )
                    .map(|value| value.is_array() || value.is_object())
                    .unwrap_or(false);
                    if is_query_result {
                        return Ok(lines);
                    }
                    log::debug!("skipping a stale log line while awaiting a query result");
                },
                ReplyShape::Ack => {},
            },
            Some((ServerToClientMsg::LogError { lines }, _)) => return Err(lines.join("\n")),
            Some((ServerToClientMsg::UnblockInputThread, _)) => {
                if shape == ReplyShape::Ack {
                    return Ok(Vec::new());
                }
            },
            Some((ServerToClientMsg::Exit { .. }, _)) | None => {
                return Err(SESSION_GONE.to_string())
            },
            // Anything else on this connection is not a reply to our action.
            Some(_) => continue,
        }
    }
}

/// The size the focus client declares itself as, both on attach and whenever
/// the server asks.
///
/// Zellij shares one rendered size per tab across every client on it: it
/// takes the *minimum* rows and the *minimum* columns reported by any client
/// currently viewing that tab (`Screen::recompute_tab_size`) — the same model
/// tmux uses. The focus client is not a real terminal and has no viewport of
/// its own, but it is a genuine attached client (not a watcher, which the
/// server excludes from this computation but also from focus — see
/// `attach_focus_client`), so whatever size it declares participates in that
/// minimum for whichever tab it is on.
///
/// A small declared size (this used to be 24×80, matching a typical
/// default) meant that attaching a real terminal to the same tab clamped the
/// *real* terminal down to 24×80 — visibly: content already rendered at the
/// real terminal's native size was still on screen when Zellij redrew a
/// smaller frame over it, leaving a corrupted double-image until a full
/// clear. Declaring a size far larger than any real terminal will ever be
/// keeps the focus client from ever being the constraining minimum, so the
/// tab always renders at whatever size the real client(s) actually are.
const FOCUS_CLIENT_ROWS: usize = 999;
const FOCUS_CLIENT_COLS: usize = 999;

/// Attach as a real client, giving the session a place to hold focus.
///
/// This is a genuine attachment, not a watcher: watchers are excluded from
/// focus handling, which is the whole point of this connection.
fn attach_focus_client(os_input: &dyn ClientOsApi) {
    let cli_assets = CliAssets {
        config_file_path: None,
        config_dir: None,
        should_ignore_config: false,
        configuration_options: None,
        layout: None,
        terminal_window_size: Size {
            rows: FOCUS_CLIENT_ROWS,
            cols: FOCUS_CLIENT_COLS,
        },
        data_dir: None,
        is_debug: false,
        max_panes: None,
        force_run_layout_commands: false,
        cwd: None,
    };
    os_input.send_to_server(ClientToServerMsg::AttachClient {
        cli_assets,
        tab_position_to_focus: None,
        pane_to_focus: None,
        is_web_client: false,
    });
}

/// Drain the attached client's stream.
///
/// An attached client is sent the full render stream. If nothing reads it the
/// socket buffer fills and the server blocks writing to us, which would stall
/// the whole session — so this thread reads and discards, and answers the one
/// question the server asks of a client.
fn spawn_focus_thread(session_name: String, os_input: Arc<Box<dyn ClientOsApi>>) {
    std::thread::Builder::new()
        .name(format!("zellij-api-focus-{}", session_name))
        .spawn(move || loop {
            match os_input.recv_from_server() {
                Some((ServerToClientMsg::QueryTerminalSize, _)) => {
                    os_input.send_to_server(ClientToServerMsg::TerminalResize {
                        new_size: Size {
                            rows: FOCUS_CLIENT_ROWS,
                            cols: FOCUS_CLIENT_COLS,
                        },
                    });
                },
                Some((ServerToClientMsg::Exit { .. }, _)) | None => break,
                // Renders and everything else are of no interest here — the
                // screen is observed through the pane subscription instead.
                Some(_) => {},
            }
        })
        .expect("could not spawn the session focus thread");
}

fn spawn_observer_thread(
    session_name: String,
    os_input: Arc<Box<dyn ClientOsApi>>,
    canvas: Arc<Mutex<CanvasStore>>,
    events: broadcast::Sender<Event>,
    alive: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name(format!("zellij-api-observer-{}", session_name))
        .spawn(move || loop {
            match os_input.recv_from_server() {
                Some((
                    ServerToClientMsg::PaneRenderUpdate {
                        pane_id,
                        viewport,
                        is_initial,
                        ..
                    },
                    _,
                )) => {
                    let pane_key = pane_id.to_string();
                    let update = canvas
                        .lock()
                        .unwrap()
                        .apply(&pane_key, viewport, is_initial);
                    match update {
                        CanvasUpdate::Changed { ts, diff } => {
                            let _ =
                                events.send(Event::from_diff(&session_name, &pane_key, ts, &diff));
                        },
                        CanvasUpdate::Reset { seq, ts, lines } => {
                            let _ = events.send(Event::ScreenReset {
                                session: session_name.clone(),
                                pane_id: pane_key,
                                seq,
                                ts,
                                lines,
                            });
                        },
                        CanvasUpdate::Unchanged => {},
                    }
                },
                Some((ServerToClientMsg::SubscribedPaneClosed { pane_id }, _)) => {
                    let pane_key = pane_id.to_string();
                    canvas.lock().unwrap().forget(&pane_key);
                    let _ = events.send(Event::PaneClosed {
                        session: session_name.clone(),
                        pane_id: pane_key,
                    });
                },
                Some((ServerToClientMsg::Exit { .. }, _)) | None => {
                    alive.store(false, Ordering::Relaxed);
                    let _ = events.send(Event::SessionEnded {
                        session: session_name.clone(),
                    });
                    break;
                },
                Some(_) => {},
            }
        })
        .expect("could not spawn the session observer thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_ids_round_trip_through_their_string_form() {
        assert_eq!(pane_id_of(&3, false), PaneId::Terminal(3));
        assert_eq!(pane_id_of(&3, true), PaneId::Plugin(3));
        assert_eq!(PaneId::Terminal(3).to_string(), "terminal_3");
        assert_eq!(PaneId::Plugin(3).to_string(), "plugin_3");
    }

    #[test]
    fn connecting_to_a_missing_session_fails_fast() {
        // Guards against `connect_to_server`'s infinite retry loop.
        let result = SessionLink::connect("zellij-api-definitely-not-a-session");
        assert!(result.is_err(), "expected a missing session to be refused");
        let message = result.err().unwrap();
        assert!(
            message.contains("not running") || message.contains("could not check"),
            "unhelpful error: {}",
            message
        );
    }

    #[test]
    fn queries_declare_a_json_reply() {
        assert_eq!(
            reply_shape(&Action::ListTabs {
                show_state: false,
                show_dimensions: false,
                show_panes: false,
                show_layout: false,
                show_all: false,
                output_json: true,
            }),
            ReplyShape::Json
        );
        // ListClients is the exception: it has no JSON form, it answers with a
        // table.
        assert_eq!(reply_shape(&Action::ListClients), ReplyShape::Text);
    }

    #[test]
    fn pane_creation_declares_a_text_reply() {
        assert_eq!(
            reply_shape(&Action::NewTiledPane {
                direction: None,
                command: None,
                pane_name: None,
                near_current_pane: false,
                no_focus: false,
                borderless: None,
                tab_id: None,
            }),
            ReplyShape::Text
        );
    }

    #[test]
    fn commands_without_output_declare_an_ack() {
        assert_eq!(reply_shape(&Action::GoToTabById { id: 1 }), ReplyShape::Ack);
        assert_eq!(
            reply_shape(&Action::WriteCharsToPaneId {
                chars: "hi".into(),
                pane_id: PaneId::Terminal(0),
            }),
            ReplyShape::Ack
        );
        // NewTab logs both a tab id and a pane id; we deliberately do not try
        // to read either, and re-query instead.
        assert_eq!(
            reply_shape(&Action::NewTab {
                tiled_layout: None,
                floating_layouts: vec![],
                swap_tiled_layouts: None,
                swap_floating_layouts: None,
                tab_name: None,
                should_change_focus_to_new_tab: true,
                cwd: None,
                initial_panes: None,
                first_pane_unblock_condition: None,
            }),
            ReplyShape::Ack
        );
    }

    #[test]
    fn reads_the_focused_pane_out_of_the_client_table() {
        let table = vec![
            "CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND".to_string(),
            "3         terminal_1     N/A".to_string(),
        ];
        assert_eq!(
            super::parse_client_table(&table),
            vec![(3, PaneId::Terminal(1))]
        );
    }

    #[test]
    fn reads_every_attached_client() {
        let table = vec![[
            "CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND",
            "1         terminal_0     zsh",
            "2         plugin_4       N/A",
        ]
        .join("\n")];
        assert_eq!(
            super::parse_client_table(&table),
            vec![(1, PaneId::Terminal(0)), (2, PaneId::Plugin(4))]
        );
    }

    #[test]
    fn an_empty_client_table_yields_no_clients() {
        let table = vec!["CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND".to_string()];
        assert!(super::parse_client_table(&table).is_empty());
    }

    #[test]
    fn learning_our_own_id_ignores_a_client_that_was_already_there() {
        // `before_attach` simulates a human who was already attached before
        // our focus client connects — their id must not be mistaken for ours.
        let before_attach: HashSet<ClientId> = [7].into_iter().collect();
        let after_attach: HashSet<ClientId> = [7, 9].into_iter().collect();
        let new_ids: Vec<ClientId> = after_attach.difference(&before_attach).copied().collect();
        assert_eq!(new_ids, vec![9]);
    }

    #[test]
    fn socket_path_lands_in_the_zellij_socket_dir() {
        let path = session_socket_path("some-session");
        assert!(path.ends_with("some-session"));
        assert!(path.starts_with(&*zellij_utils::consts::ZELLIJ_SOCK_DIR));
    }
}
