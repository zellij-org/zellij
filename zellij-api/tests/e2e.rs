//! End-to-end test: drives a real Zellij session entirely over the WebSocket
//! API — creating it, typing into it, and reading back the screen diffs.
//!
//! This needs the built `zellij` binary (the API server spawns session servers
//! by re-executing it), so it is opt-in:
//!
//! ```sh
//! cargo xtask build
//! ZELLIJ_API_E2E=target/dev-opt/zellij cargo test -p zellij-api --test e2e -- --nocapture
//! ```

use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

const TOKEN: &str = "e2e-secret";
const PORT: u16 = 8799;

struct ServerProcess(Child);

impl Drop for ServerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Makes sure a test session is torn down even if the test panics — otherwise a
/// failed run leaves a real Zellij server behind.
struct SessionGuard {
    binary: String,
    session: String,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        let _ = Command::new(&self.binary)
            .args(["delete-session", &self.session, "--force"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn zellij_binary() -> Option<String> {
    std::env::var("ZELLIJ_API_E2E").ok().filter(|p| {
        let exists = std::path::Path::new(p).exists();
        if !exists {
            eprintln!("ZELLIJ_API_E2E points at '{}', which does not exist", p);
        }
        exists
    })
}

struct Client {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    next_id: u64,
    /// Every event observed, kept so failures can show what did arrive.
    seen: Vec<Value>,
}

impl Client {
    async fn connect() -> Self {
        Self::connect_to(PORT).await
    }

    async fn connect_to(port: u16) -> Self {
        let url = format!("ws://127.0.0.1:{}/api?token={}", port, TOKEN);
        for attempt in 0..50 {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((socket, _)) => {
                    return Client {
                        socket,
                        next_id: 1,
                        seen: Vec::new(),
                    }
                },
                Err(e) if attempt == 49 => panic!("could not connect to the API server: {}", e),
                Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
        unreachable!()
    }

    /// Send a command and return its `result`, asserting that it succeeded.
    async fn call(&mut self, command: Value) -> Value {
        let described = command.to_string();
        let reply = self.call_raw(command).await;
        assert_eq!(
            reply["ok"], true,
            "command {} failed: {}",
            described, reply["error"]
        );
        reply["result"].clone()
    }

    /// Send a command and return the whole reply, successful or not.
    async fn call_raw(&mut self, mut command: Value) -> Value {
        let id = self.next_id.to_string();
        self.next_id += 1;
        command["id"] = json!(id);

        self.socket
            .send(Message::Text(command.to_string().into()))
            .await
            .expect("send failed");

        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let frame = tokio::time::timeout_at(deadline, self.socket.next())
                .await
                .unwrap_or_else(|_| panic!("timed out waiting for a reply to {}", command))
                .expect("socket closed")
                .expect("websocket error");
            let Message::Text(text) = frame else { continue };
            let value: Value = serde_json::from_str(&text).expect("reply was not JSON");
            if value["id"] == json!(id) {
                return value;
            }
            if value.get("event").is_some() {
                self.seen.push(value);
            }
        }
    }

    /// Collect events until `predicate` matches one, or we time out.
    async fn wait_for_event(
        &mut self,
        timeout: Duration,
        mut predicate: impl FnMut(&Value) -> bool,
    ) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let frame = match tokio::time::timeout_at(deadline, self.socket.next()).await {
                Ok(Some(Ok(frame))) => frame,
                Ok(Some(Err(e))) => panic!("websocket error: {}", e),
                Ok(None) => return None,
                Err(_) => return None,
            };
            let Message::Text(text) = frame else { continue };
            let value: Value = serde_json::from_str(&text).expect("event was not JSON");
            if value.get("event").is_some() {
                self.seen.push(value.clone());
                if predicate(&value) {
                    return Some(value);
                }
            }
        }
    }

    /// Wait until no screen change has arrived for `idle`, giving up after
    /// `max`.
    ///
    /// A remote driver cannot know when a freshly started shell is ready to
    /// accept input — a shell discards anything typed before it finishes
    /// initialising. Watching the diff stream go quiet is the signal, and it is
    /// the same technique any API consumer would use.
    async fn wait_until_quiet(&mut self, idle: Duration, max: Duration) {
        let give_up = tokio::time::Instant::now() + max;
        loop {
            let next_idle = tokio::time::Instant::now() + idle;
            let until = next_idle.min(give_up);
            match tokio::time::timeout_at(until, self.socket.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        if value.get("event").is_some() {
                            self.seen.push(value);
                        }
                    }
                },
                Ok(Some(Ok(_))) => continue,
                Ok(Some(Err(e))) => panic!("websocket error: {}", e),
                Ok(None) => return,
                // Timed out: either the screen went quiet, or we ran out of
                // patience. Either way, stop waiting.
                Err(_) => return,
            }
            if tokio::time::Instant::now() >= give_up {
                return;
            }
        }
    }

    /// A readable dump of the events observed so far, for failure messages.
    fn event_log(&self) -> String {
        self.seen
            .iter()
            .map(|e| match e["event"].as_str() {
                Some("screen.diff") => format!(
                    "screen.diff pane={} seq={}\n{}",
                    e["pane_id"],
                    e["seq"],
                    e["unified"].as_str().unwrap_or("")
                ),
                _ => e.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drives_a_real_session_over_the_websocket() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(PORT.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect().await;
    let session = format!("zellij-api-e2e-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };

    // --- sessions ---------------------------------------------------------
    let created = client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;
    assert_eq!(created["session"], json!(session));

    let listed = client.call(json!({"cmd": "session.list"})).await;
    let names: Vec<String> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&session),
        "created session missing from session.list: {:?}",
        names
    );

    // A layout file that is not on disk must be refused, not silently ignored
    // in favour of the default layout.
    let bad_layout = client
        .call_raw(json!({
            "cmd": "session.create", "name": format!("{}-bad-layout", session),
            "layout": "/nonexistent/definitely/not/here.kdl",
        }))
        .await;
    assert_eq!(bad_layout["ok"], json!(false));
    assert!(
        bad_layout["error"]
            .as_str()
            .unwrap()
            .contains("no layout file"),
        "unhelpful error: {}",
        bad_layout["error"]
    );

    // --- tabs -------------------------------------------------------------
    let tabs = client
        .call(json!({"cmd": "tab.list", "session": session}))
        .await;
    let initial_tab_count = tabs["tabs"].as_array().unwrap().len();
    assert!(initial_tab_count >= 1, "a new session should have a tab");

    client
        .call(json!({"cmd": "tab.create", "session": session, "name": "second"}))
        .await;
    let tabs = client
        .call(json!({"cmd": "tab.list", "session": session}))
        .await;
    let tab_list = tabs["tabs"].as_array().unwrap();

    // Focus is controllable through the API: focusing a tab makes it the
    // session's active tab, which is what every "current tab" behaviour and
    // unaddressed input then follows.
    let first_tab = tab_list
        .iter()
        .find(|t| t["name"] == json!("Tab #1"))
        .expect("the original tab should still exist")["tab_id"]
        .as_u64()
        .unwrap();
    let second_tab = tab_list
        .iter()
        .find(|t| t["name"] == json!("second"))
        .expect("the created tab should exist")["tab_id"]
        .as_u64()
        .unwrap();

    for target in [first_tab, second_tab, first_tab] {
        let focused = client
            .call(json!({"cmd": "tab.focus", "session": session, "tab_id": target}))
            .await;
        assert_eq!(focused["focused"], json!(target));

        let active: Vec<u64> = client
            .call(json!({"cmd": "tab.list", "session": session}))
            .await["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["active"] == json!(true))
            .map(|t| t["tab_id"].as_u64().unwrap())
            .collect();
        assert_eq!(
            active,
            vec![target],
            "tab.focus should leave exactly tab {} active",
            target
        );
    }
    // A tab id that does not exist must fail fast and by name, not after
    // waiting out a focus-change timeout that was never going to resolve.
    let bogus_tab = second_tab + 1000;
    for (cmd, extra) in [
        ("tab.focus", json!({})),
        ("tab.close", json!({})),
        ("tab.rename", json!({"name": "x"})),
    ] {
        let mut body = json!({"cmd": cmd, "session": session, "tab_id": bogus_tab});
        body.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = client.call_raw(body).await;
        assert_eq!(
            result["ok"],
            json!(false),
            "{} on a missing tab must fail",
            cmd
        );
        assert!(
            result["error"].as_str().unwrap().contains("no tab"),
            "{}: unhelpful error: {}",
            cmd,
            result["error"]
        );
    }

    // Leave the newest tab focused for the rest of the scenario.
    client
        .call(json!({"cmd": "tab.focus", "session": session, "tab_id": second_tab}))
        .await;
    assert_eq!(
        tab_list.len(),
        initial_tab_count + 1,
        "tab.create should have added a tab"
    );
    assert!(
        tab_list.iter().any(|t| t["name"] == json!("second")),
        "named tab missing: {:?}",
        tab_list
    );

    // --- panes ------------------------------------------------------------
    let panes = client
        .call(json!({"cmd": "pane.list", "session": session}))
        .await;
    let pane_list = panes["panes"].as_array().unwrap();
    assert!(!pane_list.is_empty(), "session should have panes");

    // --- screen subscription + input --------------------------------------
    let subscribed = client
        .call(json!({"cmd": "screen.subscribe", "session": session}))
        .await;
    assert_eq!(subscribed["following_new_panes"], json!(true));
    let followed = subscribed["panes"].as_array().unwrap().clone();
    assert!(
        !followed.is_empty(),
        "should be following at least one pane"
    );

    // Every subscribed pane sends a baseline first.
    client
        .wait_for_event(Duration::from_secs(10), |e| e["event"] == "screen.reset")
        .await
        .expect("expected a screen.reset baseline");

    // Zellij greets a new session with release notes and a startup tip, as
    // floating plugin panes — and a floating pane takes focus, which would put
    // a welcome screen in the way of everything the API does. Sessions the API
    // creates switch the greeting off, so nothing floats over the shell.
    let panes = client
        .call(json!({"cmd": "pane.list", "session": session}))
        .await;
    let floating: Vec<String> = panes["panes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["is_floating"] == json!(true))
        .map(|p| p["title"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(
        floating.is_empty(),
        "an API-created session should have no floating greeting panes, found {:?}",
        floating
    );

    // With nothing stealing focus, unaddressed input reaches the shell.
    let probe = client
        .call(json!({"cmd": "input.text", "session": session, "text": ""}))
        .await;
    assert!(
        probe["pane_id"].as_str().unwrap().starts_with("terminal_"),
        "unaddressed input should reach a terminal pane, got {}",
        probe["pane_id"]
    );

    // Let the shell in the newly created tab finish starting before typing;
    // anything sent before it is ready is discarded as typeahead.
    client
        .wait_until_quiet(Duration::from_millis(1500), Duration::from_secs(20))
        .await;

    // Type a command into the focused pane and watch the screen change.
    let marker = "zellij_api_e2e_marker";
    let typed = client
        .call(json!({
            "cmd": "input.text",
            "session": session,
            "text": format!("echo {}\n", marker),
        }))
        .await;
    // With no pane_id given, input goes to the focused pane of the active tab
    // — which is the tab we just created.
    let typed_into = typed["pane_id"].as_str().unwrap().to_string();
    assert_eq!(typed["bytes"], json!(format!("echo {}\n", marker).len()));

    let diff = match client
        .wait_for_event(Duration::from_secs(20), |e| {
            e["event"] == "screen.diff"
                && e["unified"]
                    .as_str()
                    .map(|u| u.contains(marker))
                    .unwrap_or(false)
        })
        .await
    {
        Some(diff) => diff,
        None => panic!(
            "no screen.diff contained the typed command. Events seen:\n{}",
            client.event_log()
        ),
    };

    let unified = diff["unified"].as_str().unwrap();
    assert!(
        unified.contains(&format!("+{}", marker)) || unified.contains(marker),
        "diff should show the added text as an addition:\n{}",
        unified
    );
    assert!(
        unified.starts_with("--- pane/"),
        "diff should be in unified format:\n{}",
        unified
    );
    assert!(diff["seq"].as_u64().unwrap() >= 1, "diffs are versioned");
    // The change was reported against the pane we typed into.
    let pane_id = diff["pane_id"].as_str().unwrap().to_string();
    assert_eq!(pane_id, typed_into);

    // --- history + snapshot -----------------------------------------------
    let history = client
        .call(json!({"cmd": "screen.history", "session": session, "pane_id": pane_id}))
        .await;
    let diffs = history["diffs"].as_array().unwrap();
    assert!(!diffs.is_empty(), "history should have recorded the change");
    assert!(
        diffs
            .iter()
            .any(|d| d["unified"].as_str().unwrap().contains(marker)),
        "history should contain the change we caused"
    );
    // Sequence numbers are monotonic, so a caller can resume from one.
    let seqs: Vec<u64> = diffs.iter().map(|d| d["seq"].as_u64().unwrap()).collect();
    assert!(
        seqs.windows(2).all(|w| w[0] < w[1]),
        "history sequence numbers must increase: {:?}",
        seqs
    );

    let resumed = client
        .call(json!({
            "cmd": "screen.history", "session": session, "pane_id": pane_id,
            "since": seqs[seqs.len() - 1],
        }))
        .await;
    assert!(
        resumed["diffs"].as_array().unwrap().len() < diffs.len(),
        "resuming from the newest version should return fewer entries"
    );

    let snapshot = client
        .call(json!({"cmd": "screen.snapshot", "session": session, "pane_id": pane_id}))
        .await;
    let lines: Vec<String> = snapshot["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap().to_string())
        .collect();
    assert!(
        lines.iter().any(|l| l.contains(marker)),
        "the pane's canvas should show what we typed:\n{:?}",
        lines
    );

    // --- keys -------------------------------------------------------------
    client
        .call(json!({
            "cmd": "input.keys", "session": session, "keys": ["c", "l", "e", "a", "r", "enter"],
        }))
        .await;
    client
        .wait_for_event(Duration::from_secs(10), |e| e["event"] == "screen.diff")
        .await
        .expect("clearing the screen should produce a diff");

    // --- teardown ---------------------------------------------------------
    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
    let listed = client.call(json!({"cmd": "session.list"})).await;
    let still_running: Vec<String> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["resurrectable"] == json!(false))
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        !still_running.contains(&session),
        "session should be gone after session.kill: {:?}",
        still_running
    );

    drop(server);
}

/// The API's string form of a pane in a `pane.list` entry.
fn pane_id_of(pane: &Value) -> String {
    let kind = if pane["is_plugin"] == json!(true) {
        "plugin"
    } else {
        "terminal"
    };
    format!("{}_{}", kind, pane["id"])
}

async fn pane_ids(client: &mut Client, session: &str) -> Vec<String> {
    client
        .call(json!({"cmd": "pane.list", "session": session}))
        .await["panes"]
        .as_array()
        .unwrap()
        .iter()
        .map(pane_id_of)
        .collect()
}

/// Covers the parts of the surface the main scenario does not touch: the pane
/// lifecycle, mouse input, and unsubscribing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn manages_panes_and_accepts_mouse_input() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 2;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-panes-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    // --- create -----------------------------------------------------------
    // A directional split is resolved against the requesting client's focused
    // pane. The API attaches a client to each session precisely so this works
    // without a human at a terminal.
    let split = client
        .call(json!({
            "cmd": "pane.create", "session": session, "direction": "right",
        }))
        .await;
    let split_pane = split["pane_id"].as_str().unwrap().to_string();
    assert!(
        pane_ids(&mut client, &session).await.contains(&split_pane),
        "a directional split should actually create the pane"
    );

    let created = client
        .call(json!({"cmd": "pane.create", "session": session, "name": "worker"}))
        .await;
    let new_pane = created["pane_id"].as_str().unwrap().to_string();
    assert!(
        new_pane.starts_with("terminal_"),
        "pane.create should report the new pane's id, got '{}'",
        new_pane
    );

    let listed = pane_ids(&mut client, &session).await;
    assert!(
        listed.contains(&new_pane),
        "the created pane '{}' should appear in pane.list, which has {:?}",
        new_pane,
        listed
    );

    // --- focus, rename, resize -------------------------------------------
    // Focusing must actually move focus, not just be accepted: the reply is
    // only sent once the session reports the pane as focused.
    client
        .call(json!({"cmd": "pane.focus", "session": session, "pane_id": new_pane}))
        .await;
    // Unaddressed input follows focus — this is the observable consequence of
    // pane.focus, and the reply names the pane that was written to.
    let typed = client
        .call(json!({"cmd": "input.text", "session": session, "text": "# focused\n"}))
        .await;
    assert_eq!(
        typed["pane_id"],
        json!(new_pane),
        "input with no pane_id should go to the focused pane"
    );

    // Focus the other pane and confirm input moves with it.
    client
        .call(json!({"cmd": "pane.focus", "session": session, "pane_id": split_pane}))
        .await;
    let typed = client
        .call(json!({"cmd": "input.text", "session": session, "text": "# moved\n"}))
        .await;
    assert_eq!(
        typed["pane_id"],
        json!(split_pane),
        "input should follow focus to the other pane"
    );

    // Put focus back for the rest of the scenario.
    client
        .call(json!({"cmd": "pane.focus", "session": session, "pane_id": new_pane}))
        .await;
    client
        .call(json!({
            "cmd": "pane.rename", "session": session, "pane_id": new_pane, "name": "renamed",
        }))
        .await;
    client
        .call(json!({
            "cmd": "pane.resize", "session": session, "pane_id": new_pane,
            "resize": "increase", "direction": "left",
        }))
        .await;

    let panes = client
        .call(json!({"cmd": "pane.list", "session": session}))
        .await;
    let renamed = panes["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| pane_id_of(p) == new_pane)
        .expect("the created pane should still be listed")
        .clone();
    assert!(
        renamed["title"].as_str().unwrap().contains("renamed"),
        "pane.rename should be reflected in the title, got {}",
        renamed["title"]
    );

    // --- mouse ------------------------------------------------------------
    client
        .call(json!({
            "cmd": "input.mouse", "session": session,
            "kind": "press", "button": "left", "x": 2, "y": 2,
        }))
        .await;
    client
        .call(json!({
            "cmd": "input.mouse", "session": session,
            "kind": "release", "button": "left", "x": 2, "y": 2,
        }))
        .await;
    // Scrolling is a mouse event too, and takes no button.
    client
        .call(json!({
            "cmd": "input.mouse", "session": session,
            "kind": "press", "button": "wheel_up", "x": 2, "y": 2,
        }))
        .await;

    // A malformed mouse event must be refused rather than silently ignored.
    let bad = client
        .call_raw(
            json!({"cmd": "input.mouse", "session": session, "kind": "wiggle", "x": 0, "y": 0}),
        )
        .await;
    assert_eq!(bad["ok"], json!(false), "unknown mouse kind must fail");

    // --- subscribe / unsubscribe -----------------------------------------
    // Following a pane that is not there must fail. The server reports an
    // unknown pane on the observer socket, which carries no replies, so a
    // caller would otherwise be told it was following something and then wait
    // forever for events that never come.
    let bad = client
        .call_raw(json!({
            "cmd": "screen.subscribe", "session": session, "pane_ids": ["terminal_4242"],
        }))
        .await;
    assert_eq!(
        bad["ok"],
        json!(false),
        "subscribing to a missing pane must fail"
    );
    assert!(
        bad["error"].as_str().unwrap().contains("no pane"),
        "unhelpful error: {}",
        bad["error"]
    );

    // A partly-valid list is refused too, naming only what is missing.
    let partly = client
        .call_raw(json!({
            "cmd": "screen.subscribe", "session": session,
            "pane_ids": [new_pane, "plugin_4242"],
        }))
        .await;
    assert_eq!(partly["ok"], json!(false));
    assert!(
        partly["error"].as_str().unwrap().contains("plugin_4242")
            && !partly["error"].as_str().unwrap().contains(&new_pane),
        "the error should name only the missing pane: {}",
        partly["error"]
    );

    // Unsubscribing from a session we never linked is a no-op, not a reason to
    // open one.
    client
        .call(json!({"cmd": "screen.unsubscribe", "session": "zellij-api-never-linked"}))
        .await;

    client
        .call(json!({"cmd": "screen.subscribe", "session": session}))
        .await;
    client
        .wait_for_event(Duration::from_secs(10), |e| e["event"] == "screen.reset")
        .await
        .expect("subscribing should deliver a baseline");
    client
        .call(json!({"cmd": "screen.unsubscribe", "session": session}))
        .await;

    // --- validation on a pane that is not there ----------------------------
    // pane.close/rename/resize used to report success against a made-up pane
    // id — the server silently drops the action, and nothing told the caller.
    for body in [
        json!({"cmd": "pane.close", "session": session, "pane_id": "terminal_777777"}),
        json!({"cmd": "pane.rename", "session": session, "pane_id": "terminal_777777", "name": "x"}),
        json!({"cmd": "pane.resize", "session": session, "pane_id": "terminal_777777", "resize": "increase"}),
    ] {
        let cmd = body["cmd"].as_str().unwrap().to_string();
        let result = client.call_raw(body).await;
        assert_eq!(
            result["ok"],
            json!(false),
            "{} on a missing pane must fail",
            cmd
        );
        assert!(
            result["error"].as_str().unwrap().contains("no pane"),
            "{}: unhelpful error: {}",
            cmd,
            result["error"]
        );
    }

    // --- close ------------------------------------------------------------
    client
        .call(json!({"cmd": "pane.close", "session": session, "pane_id": new_pane}))
        .await;
    // Closing a pane is asynchronous; give the session a moment to settle.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !pane_ids(&mut client, &session).await.contains(&new_pane),
        "pane.close should have removed the pane again"
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// `pane.focus` on a pane id that doesn't exist should fail fast with a
/// specific message, the same way `pane.close`/`pane.rename`/`pane.resize`
/// already do — it originally skipped the existence check and instead ran
/// the full ~1.2s focus-confirmation retry loop before failing with a vaguer
/// "the session did not focus pane ... (no attached client moved to it)".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn focusing_a_missing_pane_fails_fast_with_a_specific_message() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 8;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-focus-missing-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    let started = tokio::time::Instant::now();
    let reply = client
        .call_raw(json!({
            "cmd": "pane.focus", "session": session, "pane_id": "terminal_999",
        }))
        .await;
    let elapsed = started.elapsed();

    assert_eq!(reply["ok"], false, "focusing a missing pane should fail");
    let error = reply["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("has no pane"),
        "expected a specific 'has no pane' error, got: {}",
        error
    );
    assert!(
        elapsed < Duration::from_millis(1000),
        "should fail fast via the existence check, not after the ~1.2s focus-confirmation \
         retry loop; took {:?}",
        elapsed
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// `args` only means something alongside `command` (it becomes the shell
/// command's argv). Giving it with `plugin`, or with neither, used to be
/// silently accepted and just as silently ignored — no error, no effect,
/// same "quiet no-op" class of bug as the missing existence checks elsewhere
/// in this file.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pane_create_rejects_args_without_a_command() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 9;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-args-no-command-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    let reply = client
        .call_raw(json!({
            "cmd": "pane.create", "session": session, "args": ["--flag"],
        }))
        .await;
    assert_eq!(
        reply["ok"], false,
        "args without command should be rejected"
    );
    assert!(
        reply["error"].as_str().unwrap_or_default().contains("args"),
        "expected an error mentioning `args`, got: {}",
        reply["error"]
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// `screen.history` on a pane that was never subscribed, and one that never
/// existed at all, look identical from the canvas store's point of view —
/// both are just an absent key, and the history lookup returns an empty
/// list either way. That is correct for the first case but wrong for the
/// second: unlike every pane-targeting mutation (`pane.close`, `pane.focus`,
/// ...), `screen.history` had no existence check, so a typo'd pane id
/// silently came back as "no history yet" instead of "no such pane".
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn screen_history_rejects_a_pane_that_does_not_exist() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 10;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-history-missing-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    let reply = client
        .call_raw(json!({
            "cmd": "screen.history", "session": session, "pane_id": "terminal_999",
        }))
        .await;
    assert_eq!(
        reply["ok"], false,
        "screen.history on a nonexistent pane should fail, not report empty history"
    );
    assert!(
        reply["error"]
            .as_str()
            .unwrap_or_default()
            .contains("has no pane"),
        "expected a specific 'has no pane' error, got: {}",
        reply["error"]
    );

    // The real (existing, never-subscribed) pane should still cleanly report
    // an empty history rather than erroring — this is not a regression of
    // that case.
    let real = client
        .call(json!({
            "cmd": "screen.history", "session": session, "pane_id": "terminal_0",
        }))
        .await;
    assert_eq!(
        real["diffs"].as_array().map(|d| d.is_empty()),
        Some(true),
        "an existing, never-subscribed pane should report empty history, not error: {real:?}"
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// Plugin panes are driven the same way terminal panes are: open one, address
/// it by id, and its UI reacts. Zellij delivers the bytes to a plugin as key
/// events rather than writing them to a pty, but that is invisible from here.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drives_a_plugin_pane() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 3;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-plugin-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    // A plugin pane, opened through the API by alias.
    let created = client
        .call(json!({
            "cmd": "pane.create", "session": session,
            "plugin": "session-manager", "floating": true,
        }))
        .await;
    let plugin_pane = created["pane_id"].as_str().unwrap().to_string();
    assert!(
        plugin_pane.starts_with("plugin_"),
        "expected a plugin pane, got {}",
        plugin_pane
    );

    client
        .call(json!({"cmd": "screen.subscribe", "session": session}))
        .await;
    client
        .wait_until_quiet(Duration::from_millis(1000), Duration::from_secs(15))
        .await;

    // Typing at the plugin reaches its UI — the session manager echoes what is
    // typed into its "Session:" field.
    client
        .call(json!({
            "cmd": "input.text", "session": session,
            "pane_id": plugin_pane, "text": "abc",
        }))
        .await;
    let typed = client
        .wait_for_event(Duration::from_secs(15), |e| {
            e["event"] == "screen.diff"
                && e["pane_id"] == json!(plugin_pane)
                // Match the added side of the diff: the field now reads "abc".
                && e["unified"]
                    .as_str()
                    .map(|u| u.lines().any(|l| l.starts_with('+') && l.contains("Session: abc")))
                    .unwrap_or(false)
        })
        .await;
    assert!(
        typed.is_some(),
        "the plugin should have reacted to typing. Events seen:\n{}",
        client.event_log()
    );

    // Named keys reach it too — backspace removes the last character.
    client
        .call(json!({
            "cmd": "input.keys", "session": session,
            "pane_id": plugin_pane, "keys": ["backspace"],
        }))
        .await;
    let edited = client
        .wait_for_event(Duration::from_secs(15), |e| {
            e["event"] == "screen.diff"
                && e["pane_id"] == json!(plugin_pane)
                // The added side now reads "ab" — a diff also carries the old
                // "abc" on its removed side, so match the added line only.
                && e["unified"]
                    .as_str()
                    .map(|u| {
                        u.lines()
                            .any(|l| l.starts_with('+') && l.contains("Session: ab_"))
                    })
                    .unwrap_or(false)
        })
        .await;
    assert!(
        edited.is_some(),
        "backspace should have reached the plugin. Events seen:\n{}",
        client.event_log()
    );

    // A pane that is not there must fail rather than swallow the input: a
    // plugin's pane vanishes when its UI is dismissed, and a caller still
    // typing at that id would otherwise get success replies and no effect.
    let stale = client
        .call_raw(json!({
            "cmd": "input.text", "session": session,
            "pane_id": "plugin_9999", "text": "x",
        }))
        .await;
    assert_eq!(
        stale["ok"],
        json!(false),
        "writing to a missing pane must fail"
    );
    assert!(
        stale["error"].as_str().unwrap().contains("no pane"),
        "unhelpful error: {}",
        stale["error"]
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// Two callers hitting the same session at once must not each open a link.
///
/// A link attaches a client, and focus resolution identifies our client by it
/// being the only one attached — so a second attachment would not fail loudly,
/// it would quietly degrade every focus-dependent command.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_share_one_attached_client() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 4;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let session = format!("zellij-api-race-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    Client::connect_to(port)
        .await
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    // Six callers on separate connections, all touching the session for the
    // first time together.
    let mut callers = Vec::new();
    for _ in 0..6 {
        let session = session.clone();
        callers.push(tokio::spawn(async move {
            let mut client = Client::connect_to(port).await;
            client
                .call(json!({"cmd": "pane.list", "session": session}))
                .await;
        }));
    }
    for caller in callers {
        caller.await.expect("caller panicked");
    }

    // `list-clients` reports one row per attached client.
    let listing = Command::new(&binary)
        .args(["--session", &session, "action", "list-clients"])
        .output()
        .expect("could not list clients");
    let rows = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("CLIENT_ID"))
        .count();
    assert_eq!(
        rows,
        1,
        "the API should hold exactly one attached client, found {}:\n{}",
        rows,
        String::from_utf8_lossy(&listing.stdout)
    );

    Client::connect_to(port)
        .await
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// Upstream's `RenameSession` handler builds the socket path with
/// `ZELLIJ_SOCK_DIR.join(&name)` and renames the file there — with no
/// validation of `name` at all. A `new_name` of `../evil` renames the socket
/// *out of* the directory session discovery scans (orphaning the session,
/// invisibly to `session.list`), and a longer `../../..` reaches further
/// still. This is reachable specifically through a wire API in a way it is not
/// through the interactive rename UI, so the API validates the name itself
/// before ever sending the action.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn session_rename_rejects_a_path_traversal_name() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 5;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-rename-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    for bad_name in ["../evil", "..", "a/b", "", "/etc/passwd"] {
        let result = client
            .call_raw(json!({
                "cmd": "session.rename", "name": session, "new_name": bad_name,
            }))
            .await;
        assert_eq!(
            result["ok"],
            json!(false),
            "renaming to '{}' must be refused",
            bad_name
        );
    }

    // The traversal attempts must not have renamed anything on disk — the
    // session is still reachable under its original name.
    let listed = client.call(json!({"cmd": "session.list"})).await;
    let names: Vec<String> = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&session),
        "the session should still exist under its original name after refused renames: {:?}",
        names
    );

    // An ordinary name still works.
    let renamed_to = format!("{}-renamed", session);
    let ok = client
        .call(json!({
            "cmd": "session.rename", "name": session, "new_name": renamed_to,
        }))
        .await;
    assert_eq!(ok["session"], json!(renamed_to));

    client
        .call(json!({"cmd": "session.kill", "name": renamed_to}))
        .await;
}

/// Regression test for a real bug: a session name long enough that its
/// socket path exceeds the OS's Unix-domain-socket path limit
/// (`sockaddr_un.sun_path`, ~104-108 bytes depending on platform) used to
/// hang `session.create` for the full startup timeout instead of failing
/// immediately. The daemonized session server fails silently at `bind()`
/// and never sends anything back; `wait_until_started`
/// (`zellij-api/src/sessions.rs`) can't distinguish "still starting" from
/// "never going to start" if the peer never speaks at all — its deadline
/// check only runs *after* receiving some message, so a peer that sends
/// nothing blocks the call for the whole timeout. The interactive CLI
/// already checks this once it has a full socket path in hand
/// (`zellij_client::check_ipc_pipe_length`), but that check is never
/// reached from this API, which spawns the session server directly.
///
/// Fixed by teaching `validate_session_name`
/// (`zellij-utils/src/sessions.rs`) — already called from both
/// `session.create` and `session.rename` — to compute the real socket path
/// and reject it upfront if it's too long, the same way the CLI's checker
/// does, just returning a `Result` instead of exiting the process.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_overlong_session_name_fails_fast_instead_of_hanging() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 14;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let overlong_name = "a".repeat(300);

    let started = tokio::time::Instant::now();
    let reply = client
        .call_raw(json!({
            "cmd": "session.create", "name": overlong_name, "rows": 24, "cols": 80,
        }))
        .await;
    let elapsed = started.elapsed();

    assert_eq!(
        reply["ok"],
        json!(false),
        "an overlong session name should be refused, not created: {reply:?}"
    );
    let error = reply["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("too long"),
        "expected an error explaining the name is too long, got: {}",
        error
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "should fail fast via upfront validation, not after the multi-second startup \
         timeout the old bug produced; took {:?}",
        elapsed
    );
}

/// A raw, non-watcher attach — deliberately built the same way a real
/// `zellij attach` connects, so tests can exercise "what happens when a human
/// also attaches" without needing an actual terminal.
struct SimulatedHuman {
    os_input: std::sync::Arc<Box<dyn zellij_client::os_input_output::ClientOsApi>>,
}

impl SimulatedHuman {
    /// Attach to `session` with the given terminal size and start draining its
    /// render stream in the background — an attached client that never reads
    /// its stream stalls the server, the same concern the API's own focus
    /// client has (see `session_link::spawn_focus_thread`).
    fn attach(session: &str, rows: usize, cols: usize) -> Self {
        use zellij_client::os_input_output::get_cli_client_os_input;
        use zellij_utils::input::cli_assets::CliAssets;
        use zellij_utils::ipc::{ClientToServerMsg, ServerToClientMsg};
        use zellij_utils::pane_size::Size;

        let os_input: Box<dyn zellij_client::os_input_output::ClientOsApi> =
            Box::new(get_cli_client_os_input().expect("could not open client IPC"));
        os_input.connect_to_server(&zellij_api::session_link::session_socket_path(session));

        let cli_assets = CliAssets {
            config_file_path: None,
            config_dir: None,
            should_ignore_config: false,
            configuration_options: None,
            layout: None,
            terminal_window_size: Size { rows, cols },
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

        let os_input: std::sync::Arc<Box<dyn zellij_client::os_input_output::ClientOsApi>> =
            std::sync::Arc::new(os_input);
        let reader = os_input.clone();
        std::thread::spawn(move || loop {
            match reader.recv_from_server() {
                Some((ServerToClientMsg::QueryTerminalSize, _)) => {
                    reader.send_to_server(ClientToServerMsg::TerminalResize {
                        new_size: Size { rows, cols },
                    });
                },
                Some((ServerToClientMsg::Exit { .. }, _)) | None => break,
                Some(_) => {},
            }
        });

        // Give the server a moment to fully register the attach before the
        // caller does anything that depends on it (e.g. counts this client as
        // already present).
        std::thread::sleep(Duration::from_millis(300));

        SimulatedHuman { os_input }
    }

    /// Move this client's own focus — mirrors exactly what a real keypress
    /// (or `session_link::run_as_client`) does: an `Action` sent as a
    /// non-CLI client is attributed to the client that sent it.
    ///
    /// Takes `Arc<Box<dyn ClientOsApi>>` (clone `.connection()` first) rather
    /// than `&self`, so callers can move it into `spawn_blocking`, whose
    /// closure must be `'static` and cannot borrow across the await.
    fn focus_pane_as(
        connection: &std::sync::Arc<Box<dyn zellij_client::os_input_output::ClientOsApi>>,
        pane_id: zellij_utils::data::PaneId,
    ) {
        use zellij_utils::input::actions::Action;
        use zellij_utils::ipc::ClientToServerMsg;
        connection.send_to_server(ClientToServerMsg::Action {
            action: Action::FocusPaneByPaneId { pane_id },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }

    fn connection(&self) -> std::sync::Arc<Box<dyn zellij_client::os_input_output::ClientOsApi>> {
        self.os_input.clone()
    }
}

/// The two bugs this covers, both only visible with a *second* real client
/// attached alongside the API's own:
///
/// 1. Zellij shares one rendered size per tab: the minimum rows and minimum
///    columns across every client on it. The API's own focus client used to
///    declare a small fixed size, which clamped a real terminal attached to
///    the same tab down to that size — visible as a corrupted, doubled render
///    when the real terminal was actually much bigger.
/// 2. Unaddressed input used to keep following wherever the API itself had
///    last set focus, even after a human attached and moved elsewhere by
///    hand.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn focus_and_size_follow_a_real_client_that_attaches_alongside_the_api() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 6;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-focus-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;

    // Attach a "human" with a real, distinct terminal size *before* any API
    // command touches this session — so when the API's own focus client
    // attaches moments later (triggered by the first command below), the
    // human is already counted and cleanly excluded from "whose id is new".
    let human = tokio::task::spawn_blocking({
        let session = session.clone();
        move || SimulatedHuman::attach(&session, 50, 200)
    })
    .await
    .expect("attaching the simulated human panicked");

    // --- sizing: the tab must render at the human's size, not clamp it -----
    // `display_area_*` is the raw negotiated client size (the direct output of
    // the min-across-clients computation this test exists to check);
    // `viewport_*` is that minus Zellij's own UI chrome (tab bar, status bar),
    // so it is always a couple of rows smaller and not what this is testing.
    let tabs = client
        .call(json!({"cmd": "tab.list", "session": session}))
        .await;
    let tab = &tabs["tabs"].as_array().unwrap()[0];
    assert_eq!(
        (
            tab["display_area_rows"].as_u64(),
            tab["display_area_columns"].as_u64()
        ),
        (Some(50), Some(200)),
        "the tab should render at the real client's size, not be clamped by \
         the API's own focus client; got {}",
        tab
    );

    // --- focus: unaddressed input must follow the human, not stay where the
    // API last put things --------------------------------------------------
    let created = client
        .call(json!({"cmd": "pane.create", "session": session, "name": "second"}))
        .await;
    let second_pane = created["pane_id"].as_str().unwrap().to_string();
    let second_pane_id = zellij_utils::data::PaneId::from_str(&second_pane).unwrap();

    // The API explicitly focuses the *original* pane first — this is what
    // makes the test meaningful: it pins `last_focused` to a real, different
    // pane, so "follow the human" and "stay where the API last put things"
    // give different, checkable answers. Without this the two coincide (the
    // API never focused anything, so the old code's fallback path can drift
    // into a right answer by coincidence rather than by tracking the human).
    let panes = client
        .call(json!({"cmd": "pane.list", "session": session}))
        .await;
    let second_pane_num = match second_pane_id {
        zellij_utils::data::PaneId::Terminal(id) => id,
        zellij_utils::data::PaneId::Plugin(id) => id,
    };
    let original_pane = panes["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| {
            p["is_plugin"] == json!(false) && p["id"].as_u64() != Some(second_pane_num as u64)
        })
        .expect("the session's original pane should still exist")["id"]
        .as_u64()
        .unwrap();
    client
        .call(json!({
            "cmd": "pane.focus", "session": session,
            "pane_id": format!("terminal_{}", original_pane),
        }))
        .await;

    // The human moves their own focus by hand — nothing routed through the API.
    let human_connection = human.connection();
    tokio::task::spawn_blocking(move || {
        SimulatedHuman::focus_pane_as(&human_connection, second_pane_id)
    })
    .await
    .expect("focusing as the simulated human panicked");

    // Poll: this is a real, independently-timed state change on the server,
    // not something the API caused, so there is nothing to await except the
    // session settling.
    let mut followed = String::new();
    for attempt in 0..20 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        let typed = client
            .call(json!({"cmd": "input.text", "session": session, "text": ""}))
            .await;
        followed = typed["pane_id"].as_str().unwrap().to_string();
        if followed == second_pane {
            break;
        }
    }
    assert_eq!(
        followed, second_pane,
        "unaddressed input should follow the human's live focus, not stay on \
         wherever the API last set it"
    );

    let _ = human; // keep alive until here
    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// Regression test for a real bug: typing into a session with no other
/// client attached (the ordinary case for pure API/MCP usage — the API's
/// own always-attached "focus" client is the only one there) was observed
/// to duplicate every character on screen (`input.text "A"` → `AA`) when
/// deployed as a systemd service — never in an ad-hoc foreground process.
///
/// This uncovered a real, independent correctness bug along the way, which
/// this test exercises: `Action::WriteCharsToPaneId`/`WriteToPaneId` sent a
/// preceding `ScreenInstruction::ClearScroll(client_id)` that resolves a
/// *client*-relative "active pane" — even though the action already names
/// an explicit `pane_id` and has no reason to go through client-focus
/// resolution to find it. Fixed by adding `ClearScrollForPaneId`, which
/// clears scroll for the pane the write already targets directly, with no
/// client_id resolution involved — `zellij-server/src/route.rs`'s
/// `WriteToPaneId`/`WriteCharsToPaneId` arms now use it instead of
/// `ClearScroll`. Correct either way, but on its own this did **not**
/// eliminate the character-duplication bug — see
/// `typing_without_a_term_environment_variable_does_not_duplicate_characters`
/// below for the actual root cause and fix, found afterward.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn typing_with_no_other_client_attached_does_not_duplicate_characters() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 11;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-no-dup-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;
    // Give the shell prompt time to fully settle before typing — this
    // matters: the original bug raced against the shell's own initial
    // prompt redraw, and typing too soon after creation dodges the race.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Deliberately no SimulatedHuman here — this is the exact condition
    // that reproduced the bug: only the API's own focus client attached.
    // A single character, not a word — the original repro used one
    // character at a time (each `input.text` call races the shell's own
    // async re-render of what it just echoed, e.g. zsh-syntax-highlighting
    // recoloring the line); writing multiple characters per call, or many
    // calls back to back without settling time, didn't reliably reproduce
    // it, since each character races independently rather than compounding.
    let reply = client
        .call(json!({
            "cmd": "input.text", "session": session, "pane_id": "terminal_0", "text": "Z",
        }))
        .await;
    assert_eq!(
        reply["bytes"],
        json!(1),
        "writing 'Z' should report 1 byte written"
    );

    tokio::time::sleep(Duration::from_millis(1000)).await;
    client
        .call(json!({
            "cmd": "screen.subscribe", "session": session, "pane_ids": ["terminal_0"],
        }))
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshot = client
        .call(json!({"cmd": "screen.snapshot", "session": session, "pane_id": "terminal_0"}))
        .await;
    let lines = snapshot["lines"].as_array().cloned().unwrap_or_default();
    let joined: String = lines
        .iter()
        .map(|l| l.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("ZZ"),
        "expected a single 'Z' with no duplicated character, got:\n{joined}"
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

/// Regression test for the actual root cause of the character-duplication
/// bug: `TERM` was empty in every environment that reproduced it (deployed
/// as a systemd service — a daemon with no controlling terminal to inherit
/// `TERM` from) and never reproduced in any environment where `TERM` was
/// present (an ad-hoc process launched from an interactive shell, which
/// always has `TERM` set). With `TERM` unset, the spawned shell's syntax
/// highlighting redraw came back malformed — missing a cursor-repositioning
/// backspace before the recolored redraw of what it had just echoed — so
/// applying it landed the colored character in a new cell instead of
/// overwriting the plain one, producing two visible characters from one
/// keystroke. Confirmed with instrumented logging against the deployed
/// service (30/30 reproductions with `TERM` empty; 0/50 across two
/// independent large batches once fixed), a signal strong enough that no
/// further theory explained the two states better.
///
/// Fixed in `src/commands.rs`'s `start_api_server`: default `TERM` to
/// `xterm-256color` at startup if empty or unset, the same way tmux/screen
/// default it for their own panes — every session this server creates is a
/// fork of this process (`zellij_server::spawn_server`), so whatever `TERM`
/// this process has at startup is what every pane's shell inherits.
///
/// This test reproduces the exact condition directly, with `Command`'s own
/// `env_remove` rather than relying on the ambient shell (which — as this
/// investigation discovered the hard way — may or may not have `TERM` set
/// for reasons entirely outside this test's control, e.g. how the parent
/// process's own session was activated).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn typing_without_a_term_environment_variable_does_not_duplicate_characters() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 13;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .env_remove("TERM")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    let mut client = Client::connect_to(port).await;
    let session = format!("zellij-api-no-term-{}", std::process::id());
    let _guard = SessionGuard {
        binary: binary.clone(),
        session: session.clone(),
    };
    client
        .call(json!({"cmd": "session.create", "name": session, "rows": 24, "cols": 80}))
        .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let reply = client
        .call(json!({
            "cmd": "input.text", "session": session, "pane_id": "terminal_0", "text": "Z",
        }))
        .await;
    assert_eq!(
        reply["bytes"],
        json!(1),
        "writing 'Z' should report 1 byte written"
    );

    tokio::time::sleep(Duration::from_millis(1000)).await;
    client
        .call(json!({
            "cmd": "screen.subscribe", "session": session, "pane_ids": ["terminal_0"],
        }))
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let snapshot = client
        .call(json!({"cmd": "screen.snapshot", "session": session, "pane_id": "terminal_0"}))
        .await;
    let lines = snapshot["lines"].as_array().cloned().unwrap_or_default();
    let joined: String = lines
        .iter()
        .map(|l| l.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !joined.contains("ZZ"),
        "expected a single 'Z' with no duplicated character even with TERM unset, got:\n{joined}"
    );

    client
        .call(json!({"cmd": "session.kill", "name": session}))
        .await;
}

#[tokio::test]
async fn rejects_connections_without_the_token() {
    let Some(binary) = zellij_binary() else {
        eprintln!("skipping: set ZELLIJ_API_E2E=<path to zellij binary> to run this test");
        return;
    };

    let port = PORT + 1;
    let _server = ServerProcess(
        Command::new(&binary)
            .arg("api-server")
            .arg("--port")
            .arg(port.to_string())
            .arg("--token")
            .arg(TOKEN)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the api server"),
    );

    // Wait for it to come up.
    let health = format!("http://127.0.0.1:{}/health", port);
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let _ = health;

    let no_token = format!("ws://127.0.0.1:{}/api", port);
    assert!(
        tokio_tungstenite::connect_async(&no_token).await.is_err(),
        "connecting without a token must be refused"
    );

    let wrong_token = format!("ws://127.0.0.1:{}/api?token=nope", port);
    assert!(
        tokio_tungstenite::connect_async(&wrong_token)
            .await
            .is_err(),
        "connecting with the wrong token must be refused"
    );
}
