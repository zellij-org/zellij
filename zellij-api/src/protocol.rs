//! The JSON protocol spoken over the control WebSocket.
//!
//! Three message shapes travel over the socket:
//!
//! - a **request** from the caller: `{"id": "1", "cmd": "tab.create", "session": "main"}`
//! - a **reply** from us:          `{"id": "1", "ok": true, "result": {...}}`
//! - an **event** from us:         `{"event": "screen.diff", ...}` (unsolicited)

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diff::{CanvasDiff, DiffHunk};

/// One inbound frame.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// Echoed back on the reply so callers can correlate. Optional, but
    /// without it a caller cannot tell replies apart.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(flatten)]
    pub command: Command,
}

impl Request {
    /// Parameters in `raw` that this command does not understand.
    ///
    /// Serde ignores unknown fields by default, and `deny_unknown_fields` does
    /// not work through `flatten` (nor does a catch-all map — with two
    /// flattened targets the map swallows every field). So the known names are
    /// taken from the command itself, by serialising it back: that cannot drift
    /// out of step with the variants the way a hand-written list would.
    ///
    /// It matters because a misspelt parameter would otherwise be dropped in
    /// silence and the command would run with a default — writing `paneids`
    /// instead of `pane_ids` would quietly subscribe to *every* pane in the
    /// session rather than the one asked for.
    pub fn unknown_fields(&self, raw: &Value) -> Vec<String> {
        let Some(sent) = raw.as_object() else {
            return Vec::new();
        };
        let mut known: Vec<String> = vec!["cmd".to_string(), "id".to_string()];
        if let Ok(Value::Object(fields)) = serde_json::to_value(&self.command) {
            known.extend(fields.keys().cloned());
        }
        sent.keys()
            .filter(|name| !known.contains(name))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "cmd")]
pub enum Command {
    // ---- sessions -------------------------------------------------------
    #[serde(rename = "session.list")]
    SessionList,
    #[serde(rename = "session.create")]
    SessionCreate {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        layout: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
        /// Geometry of the virtual terminal the session is laid out in. This
        /// is the size the canvas diffs are computed against.
        #[serde(default)]
        rows: Option<usize>,
        #[serde(default)]
        cols: Option<usize>,
    },
    #[serde(rename = "session.kill")]
    SessionKill { name: String },
    #[serde(rename = "session.rename")]
    SessionRename { name: String, new_name: String },

    // ---- tabs -----------------------------------------------------------
    #[serde(rename = "tab.list")]
    TabList { session: String },
    #[serde(rename = "tab.create")]
    TabCreate {
        session: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    #[serde(rename = "tab.close")]
    TabClose { session: String, tab_id: u64 },
    #[serde(rename = "tab.focus")]
    TabFocus { session: String, tab_id: u64 },
    #[serde(rename = "tab.rename")]
    TabRename {
        session: String,
        tab_id: u64,
        name: String,
    },

    // ---- panes ----------------------------------------------------------
    #[serde(rename = "pane.list")]
    PaneList { session: String },
    #[serde(rename = "pane.create")]
    PaneCreate {
        session: String,
        #[serde(default)]
        command: Option<String>,
        /// A plugin to run in the pane instead of a command: a URL
        /// (`file:/path/to.wasm`, `https://...`) or an alias (`session-manager`).
        #[serde(default)]
        plugin: Option<String>,
        /// Configuration passed to the plugin on startup.
        #[serde(default)]
        plugin_config: Option<std::collections::BTreeMap<String, String>>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        floating: bool,
        /// One of `right`, `left`, `up`, `down`.
        #[serde(default)]
        direction: Option<String>,
        #[serde(default)]
        name: Option<String>,
        /// Which tab to open the pane in. Defaults to the session's active tab
        /// (or its first tab, when nothing is attached to focus one).
        #[serde(default)]
        tab_id: Option<u64>,
    },
    #[serde(rename = "pane.close")]
    PaneClose { session: String, pane_id: String },
    #[serde(rename = "pane.focus")]
    PaneFocus { session: String, pane_id: String },
    #[serde(rename = "pane.rename")]
    PaneRename {
        session: String,
        pane_id: String,
        name: String,
    },
    #[serde(rename = "pane.resize")]
    PaneResize {
        session: String,
        pane_id: String,
        /// `increase` or `decrease`.
        resize: String,
        #[serde(default)]
        direction: Option<String>,
    },

    // ---- input ----------------------------------------------------------
    #[serde(rename = "input.text")]
    InputText {
        session: String,
        /// Omit to target the session's focused pane.
        #[serde(default)]
        pane_id: Option<String>,
        text: String,
    },
    #[serde(rename = "input.keys")]
    InputKeys {
        session: String,
        #[serde(default)]
        pane_id: Option<String>,
        /// Key names such as `enter`, `ctrl-c`, `alt-f`, `up`, `f5`, `a`.
        keys: Vec<String>,
    },
    #[serde(rename = "input.mouse")]
    InputMouse {
        session: String,
        /// `press`, `release` or `motion`.
        kind: String,
        /// Column, 0-based, in the session's coordinate space.
        x: u16,
        /// Row, 0-based, in the session's coordinate space.
        y: u16,
        /// `left`, `right`, `middle`, `wheel_up`, `wheel_down`.
        #[serde(default)]
        button: Option<String>,
        #[serde(default)]
        alt: bool,
        #[serde(default)]
        ctrl: bool,
        #[serde(default)]
        shift: bool,
    },

    // ---- screen ---------------------------------------------------------
    #[serde(rename = "screen.snapshot")]
    ScreenSnapshot {
        session: String,
        /// Omit to target the session's currently focused pane, the same
        /// way `input.*` does.
        #[serde(default)]
        pane_id: Option<String>,
    },
    #[serde(rename = "screen.history")]
    ScreenHistory {
        session: String,
        /// Omit to target the session's currently focused pane, the same
        /// way `input.*` does.
        #[serde(default)]
        pane_id: Option<String>,
        /// Return only entries with `seq` greater than this.
        #[serde(default)]
        since: Option<u64>,
        #[serde(default)]
        limit: Option<usize>,
    },
    #[serde(rename = "screen.subscribe")]
    ScreenSubscribe {
        session: String,
        /// Omit to follow every pane in the session, including ones opened later.
        #[serde(default)]
        pane_ids: Option<Vec<String>>,
    },
    #[serde(rename = "screen.unsubscribe")]
    ScreenUnsubscribe { session: String },
}

/// One outbound reply frame.
#[derive(Debug, Clone, Serialize)]
pub struct Reply {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    pub fn ok(id: Option<String>, result: Value) -> Self {
        Reply {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<String>, error: impl std::fmt::Display) -> Self {
        Reply {
            id,
            ok: false,
            result: None,
            error: Some(error.to_string()),
        }
    }
}

/// One outbound event frame.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event")]
pub enum Event {
    /// A pane's canvas changed. This is the git-diff of the screen.
    #[serde(rename = "screen.diff")]
    ScreenDiff {
        session: String,
        pane_id: String,
        /// Monotonic per-pane sequence number; equals the new canvas version.
        seq: u64,
        /// Unix milliseconds.
        ts: u64,
        added: usize,
        removed: usize,
        unified: String,
        hunks: Vec<DiffHunk>,
    },
    /// The canvas baseline was (re)established — on first subscribe, or after
    /// a resync. Carries the full contents rather than a diff.
    #[serde(rename = "screen.reset")]
    ScreenReset {
        session: String,
        pane_id: String,
        seq: u64,
        ts: u64,
        lines: Vec<String>,
    },
    #[serde(rename = "pane.opened")]
    PaneOpened { session: String, pane_id: String },
    #[serde(rename = "pane.closed")]
    PaneClosed { session: String, pane_id: String },
    #[serde(rename = "session.ended")]
    SessionEnded { session: String },
}

impl Event {
    pub fn from_diff(session: &str, pane_id: &str, ts: u64, diff: &CanvasDiff) -> Self {
        Event::ScreenDiff {
            session: session.to_string(),
            pane_id: pane_id.to_string(),
            seq: diff.to_version,
            ts,
            added: diff.added,
            removed: diff.removed,
            unified: diff.unified.clone(),
            hunks: diff.hunks.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: serde_json::Value) -> Request {
        serde_json::from_value(raw).expect("request should parse")
    }

    #[test]
    fn parses_a_command_with_no_params() {
        let req = parse(serde_json::json!({"id": "7", "cmd": "session.list"}));
        assert_eq!(req.id.as_deref(), Some("7"));
        assert!(matches!(req.command, Command::SessionList));
    }

    #[test]
    fn parses_dotted_command_names_with_params() {
        let req = parse(serde_json::json!({
            "id": "1", "cmd": "input.text", "session": "main", "pane_id": "terminal_3",
            "text": "sudo\n"
        }));
        match req.command {
            Command::InputText {
                session,
                pane_id,
                text,
            } => {
                assert_eq!(session, "main");
                assert_eq!(pane_id.as_deref(), Some("terminal_3"));
                assert_eq!(text, "sudo\n");
            },
            other => panic!("wrong command: {:?}", other),
        }
    }

    #[test]
    fn optional_params_may_be_omitted() {
        let req = parse(serde_json::json!({"cmd": "session.create"}));
        match req.command {
            Command::SessionCreate {
                name,
                layout,
                cwd,
                rows,
                cols,
            } => {
                assert!(name.is_none() && layout.is_none() && cwd.is_none());
                assert!(rows.is_none() && cols.is_none());
            },
            other => panic!("wrong command: {:?}", other),
        }
        assert!(req.id.is_none(), "id is optional");
    }

    #[test]
    fn omitting_pane_ids_means_follow_every_pane() {
        let req = parse(serde_json::json!({"cmd": "screen.subscribe", "session": "main"}));
        match req.command {
            Command::ScreenSubscribe { pane_ids, .. } => assert!(pane_ids.is_none()),
            other => panic!("wrong command: {:?}", other),
        }
    }

    #[test]
    fn unknown_command_is_rejected_not_silently_ignored() {
        let raw = serde_json::json!({"cmd": "nope.nope"});
        assert!(serde_json::from_value::<Request>(raw).is_err());
    }

    #[test]
    fn a_misspelt_parameter_is_reported_rather_than_dropped() {
        // `paneids` is not `pane_ids`; taken silently this would follow every
        // pane in the session instead of the one the caller named.
        let raw = serde_json::json!({
            "cmd": "screen.subscribe", "session": "main", "paneids": ["terminal_0"],
        });
        let req = parse(raw.clone());
        assert_eq!(req.unknown_fields(&raw), vec!["paneids".to_string()]);
    }

    #[test]
    fn a_well_formed_command_has_nothing_left_over() {
        let raw = serde_json::json!({
            "id": "1", "cmd": "screen.subscribe", "session": "main",
            "pane_ids": ["terminal_0"],
        });
        let req = parse(raw.clone());
        assert!(
            req.unknown_fields(&raw).is_empty(),
            "recognised fields must not be reported as unknown: {:?}",
            req.unknown_fields(&raw)
        );
    }

    #[test]
    fn optional_fields_left_out_are_not_unknown() {
        let raw = serde_json::json!({"cmd": "session.create"});
        let req = parse(raw.clone());
        assert!(req.unknown_fields(&raw).is_empty());
    }

    #[test]
    fn every_documented_parameter_is_accepted() {
        // Guards the check itself: if serialising a command ever stopped
        // naming its fields, every parameter would look unknown.
        let raw = serde_json::json!({
            "id": "7", "cmd": "pane.create", "session": "main",
            "command": "htop", "args": ["-d", "5"], "cwd": "/tmp",
            "floating": true, "direction": "right", "name": "top",
            "tab_id": 2, "plugin": null, "plugin_config": null,
        });
        let req = parse(raw.clone());
        assert!(
            req.unknown_fields(&raw).is_empty(),
            "documented parameters rejected: {:?}",
            req.unknown_fields(&raw)
        );
    }

    #[test]
    fn replies_omit_the_unused_half() {
        let ok =
            serde_json::to_value(Reply::ok(Some("1".into()), serde_json::json!({"a": 1}))).unwrap();
        assert_eq!(ok["ok"], true);
        assert!(
            ok.get("error").is_none(),
            "successful reply carries no error"
        );

        let err = serde_json::to_value(Reply::err(Some("1".into()), "boom")).unwrap();
        assert_eq!(err["ok"], false);
        assert_eq!(err["error"], "boom");
        assert!(
            err.get("result").is_none(),
            "failed reply carries no result"
        );
    }

    #[test]
    fn diff_events_serialize_in_the_documented_shape() {
        let diff = crate::diff::diff_canvas(
            &["sudr".to_string()],
            &["sudo".to_string()],
            41,
            42,
            "pane/terminal_1",
        )
        .unwrap();
        let event = Event::from_diff("main", "terminal_1", 1754342400123, &diff);
        let json = serde_json::to_value(&event).unwrap();

        assert_eq!(json["event"], "screen.diff");
        assert_eq!(json["session"], "main");
        assert_eq!(json["pane_id"], "terminal_1");
        assert_eq!(json["seq"], 42);
        assert_eq!(json["ts"], 1754342400123u64);
        assert_eq!(json["added"], 1);
        assert_eq!(json["removed"], 1);
        assert!(json["unified"].as_str().unwrap().contains("-sudr"));
        assert!(json["unified"].as_str().unwrap().contains("+sudo"));
        assert!(json["hunks"].is_array());
    }
}
