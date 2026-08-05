//! MCP tools, one per command in the Zellij remote-control WebSocket API (see
//! `REMOTE_API.md`). Each tool is a thin translation: build the JSON command,
//! send it over `ApiClient`, hand the result back as text.
//!
//! `attach_session` is the one composite tool — it is what "attach" means for
//! an MCP caller, which has no terminal to draw into. A human running
//! `zellij attach` gets oriented by *looking at the screen*; this tool gives
//! an agent the same orientation in one call: every tab, every pane, and the
//! current on-screen content of each terminal pane.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ProtocolVersion, ServerCapabilities,
    ServerInfo,
};
use rmcp::{ErrorData as McpError, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde_json::{json, Value};

use crate::ws_client::ApiClient;

#[derive(Clone)]
pub struct ZellijTools {
    api: ApiClient,
    tool_router: ToolRouter<ZellijTools>,
}

fn ok_json(value: Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
}

/// A tool-level error: the request was valid and reached the tool, but the
/// operation it represents failed (the session doesn't exist, the pane is
/// gone, a bad key name — anything the *caller* got wrong or that failed for
/// an ordinary reason). Returned as `Ok(CallToolResult::error(...))`, per
/// rmcp's own guidance on `CallToolResult::error`: MCP clients render this
/// content back to the model, which is what lets it see the failure and react
/// — a protocol-level `Err(McpError)` is for when the *server* cannot proceed
/// at all, not for "this Zellij command failed".
fn err_result(message: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        message.into(),
    )]))
}

// --- parameter types --------------------------------------------------------

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateSessionParams {
    /// Name for the new session. Omit to get an auto-generated one.
    #[serde(default)]
    pub name: Option<String>,
    /// A built-in layout name, or a path to a layout file.
    #[serde(default)]
    pub layout: Option<String>,
    /// Working directory for the session's initial pane.
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub rows: Option<usize>,
    #[serde(default)]
    pub cols: Option<usize>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SessionNameParams {
    pub session: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameSessionParams {
    pub session: String,
    pub new_name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SessionParams {
    pub session: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreateTabParams {
    pub session: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct TabIdParams {
    pub session: String,
    pub tab_id: u64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenameTabParams {
    pub session: String,
    pub tab_id: u64,
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CreatePaneParams {
    pub session: String,
    /// A shell command to run in the new pane.
    #[serde(default)]
    pub command: Option<String>,
    /// A plugin URL or alias to run instead of a command (e.g.
    /// `session-manager`). Give either `command` or `plugin`, not both.
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub floating: bool,
    /// `right`, `left`, `up`, or `down`. Not supported for tiled plugin panes.
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    /// Which tab to create the pane in. Defaults to the session's active tab.
    #[serde(default)]
    pub tab_id: Option<u64>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct PaneIdParams {
    pub session: String,
    pub pane_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RenamePaneParams {
    pub session: String,
    pub pane_id: String,
    pub name: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ResizePaneParams {
    pub session: String,
    pub pane_id: String,
    /// `increase` or `decrease`.
    pub resize: String,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SendTextParams {
    pub session: String,
    /// Which pane to type into. Omit to target the focused pane.
    #[serde(default)]
    pub pane_id: Option<String>,
    pub text: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SendKeysParams {
    pub session: String,
    #[serde(default)]
    pub pane_id: Option<String>,
    /// Key names in order, e.g. `["ctrl-c"]`, `["down", "down", "enter"]`,
    /// `["a"]`. Named keys: enter, tab, esc, space, backspace, delete,
    /// insert, home, end, pageup, pagedown, up/down/left/right, f1–f12.
    /// Modifiers: ctrl-, alt-, shift- (combinable: ctrl-alt-c).
    pub keys: Vec<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SendMouseParams {
    pub session: String,
    /// `press`, `release`, or `motion`.
    pub kind: String,
    /// Column, 0-based.
    pub x: u16,
    /// Row, 0-based.
    pub y: u16,
    /// `left`, `right`, `middle`, `wheel_up`, `wheel_down`. Defaults to `left`.
    #[serde(default)]
    pub button: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ReadScreenParams {
    pub session: String,
    /// Which pane to read. Omit to target the focused pane.
    #[serde(default)]
    pub pane_id: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct ScreenHistoryParams {
    pub session: String,
    /// Which pane's history to read. Omit to target the focused pane.
    #[serde(default)]
    pub pane_id: Option<String>,
    /// Only diffs newer than this version.
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub limit: Option<usize>,
}

// --- tools -------------------------------------------------------------

#[tool_router]
impl ZellijTools {
    pub fn new(api: ApiClient) -> Self {
        Self {
            api,
            tool_router: Self::tool_router(),
        }
    }

    async fn run(&self, command: Value) -> Result<CallToolResult, McpError> {
        match self.api.call(command).await {
            Ok(result) => ok_json(result),
            Err(e) => err_result(e),
        }
    }

    // --- sessions ---------------------------------------------------------

    #[tool(description = "List every Zellij session this API can see, running or resurrectable.")]
    async fn list_sessions(&self) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "session.list"})).await
    }

    #[tool(description = "Create a new detached Zellij session.")]
    async fn create_session(
        &self,
        Parameters(p): Parameters<CreateSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "session.create", "name": p.name, "layout": p.layout,
            "cwd": p.cwd, "rows": p.rows, "cols": p.cols,
        }))
        .await
    }

    #[tool(description = "Kill a session and every pane in it.")]
    async fn kill_session(
        &self,
        Parameters(p): Parameters<SessionNameParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "session.kill", "name": p.session})).await
    }

    #[tool(description = "Rename a session.")]
    async fn rename_session(
        &self,
        Parameters(p): Parameters<RenameSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "session.rename", "name": p.session, "new_name": p.new_name}))
            .await
    }

    /// The composite "attach" tool: everything a caller needs to get
    /// oriented in one call, since there is no terminal to draw into.
    #[tool(
        description = "Attach to a session: returns its tabs, its panes, and the current on-screen \
                        content of every terminal pane. This is the tool to call first when picking \
                        up a session — the MCP equivalent of `zellij attach` for an agent that has no \
                        screen to look at. Subscribes the session for you if it wasn't already."
    )]
    async fn attach_session(
        &self,
        Parameters(p): Parameters<SessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let tabs = match self
            .api
            .call(json!({"cmd": "tab.list", "session": p.session}))
            .await
        {
            Ok(tabs) => tabs,
            Err(e) => return err_result(e),
        };
        let panes = match self
            .api
            .call(json!({"cmd": "pane.list", "session": p.session}))
            .await
        {
            Ok(panes) => panes,
            Err(e) => return err_result(e),
        };

        // Following every pane is required before any of them can be
        // snapshotted; already-followed panes are unaffected. This only
        // requests the subscription — the first render event that actually
        // populates the snapshot arrives asynchronously afterward, which is
        // why the snapshot loop below retries rather than reading once.
        let _ = self
            .api
            .call(json!({"cmd": "screen.subscribe", "session": p.session}))
            .await;

        let mut screens = Vec::new();
        if let Some(pane_list) = panes["panes"].as_array() {
            for pane in pane_list {
                let is_plugin = pane["is_plugin"].as_bool().unwrap_or(false);
                let suppressed = pane["is_suppressed"].as_bool().unwrap_or(false);
                if is_plugin || suppressed {
                    continue;
                }
                let Some(id) = pane["id"].as_u64() else { continue };
                let pane_id = format!("terminal_{id}");
                let command = json!({
                    "cmd": "screen.snapshot", "session": p.session, "pane_id": pane_id,
                });
                // The subscription's baseline can take a moment to arrive;
                // a handful of short retries covers that without the tool
                // call itself hanging noticeably.
                let mut snapshot = self.api.call(command.clone()).await;
                for _ in 0..8 {
                    if snapshot.is_ok() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    snapshot = self.api.call(command.clone()).await;
                }
                match snapshot {
                    Ok(snapshot) => screens.push(snapshot),
                    Err(e) => {
                        // Report the miss in the result itself, not just the
                        // log — a caller reading `screens` should not have
                        // fewer entries than `panes` with no way to tell why.
                        log::warn!("attach_session: could not snapshot {pane_id}: {e}");
                        screens.push(json!({"pane_id": pane_id, "error": e}));
                    },
                }
            }
        }

        ok_json(json!({
            "session": p.session,
            "tabs": tabs["tabs"],
            "panes": panes["panes"],
            "screens": screens,
        }))
    }

    // --- tabs ---------------------------------------------------------------

    #[tool(description = "List a session's tabs.")]
    async fn list_tabs(
        &self,
        Parameters(p): Parameters<SessionParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "tab.list", "session": p.session})).await
    }

    #[tool(description = "Create a new tab in a session.")]
    async fn create_tab(
        &self,
        Parameters(p): Parameters<CreateTabParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "tab.create", "session": p.session, "name": p.name, "cwd": p.cwd}))
            .await
    }

    #[tool(description = "Close a tab by id.")]
    async fn close_tab(
        &self,
        Parameters(p): Parameters<TabIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "tab.close", "session": p.session, "tab_id": p.tab_id}))
            .await
    }

    #[tool(
        description = "Focus a tab by id — actually moves focus, unlike send_text/send_keys/ \
                        send_mouse, which only write. Confirmed before returning: fails if it never \
                        takes."
    )]
    async fn focus_tab(
        &self,
        Parameters(p): Parameters<TabIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "tab.focus", "session": p.session, "tab_id": p.tab_id}))
            .await
    }

    #[tool(description = "Rename a tab by id.")]
    async fn rename_tab(
        &self,
        Parameters(p): Parameters<RenameTabParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "tab.rename", "session": p.session, "tab_id": p.tab_id, "name": p.name,
        }))
        .await
    }

    // --- panes ------------------------------------------------------------

    #[tool(description = "List a session's panes, across every tab.")]
    async fn list_panes(
        &self,
        Parameters(p): Parameters<SessionParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "pane.list", "session": p.session})).await
    }

    #[tool(
        description = "Create a pane: a shell command, or a plugin (session-manager, about, ...). \
                        Returns the new pane's id."
    )]
    async fn create_pane(
        &self,
        Parameters(p): Parameters<CreatePaneParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "pane.create", "session": p.session, "command": p.command,
            "plugin": p.plugin, "args": p.args, "cwd": p.cwd, "floating": p.floating,
            "direction": p.direction, "name": p.name, "tab_id": p.tab_id,
        }))
        .await
    }

    #[tool(description = "Close a pane by id.")]
    async fn close_pane(
        &self,
        Parameters(p): Parameters<PaneIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "pane.close", "session": p.session, "pane_id": p.pane_id}))
            .await
    }

    #[tool(
        description = "Focus a pane by id (and its tab) — actually moves focus, unlike \
                        send_text/send_keys/send_mouse, which only write. Unaddressed input.* then \
                        targets it, unless a real client has taken focus back."
    )]
    async fn focus_pane(
        &self,
        Parameters(p): Parameters<PaneIdParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({"cmd": "pane.focus", "session": p.session, "pane_id": p.pane_id}))
            .await
    }

    #[tool(description = "Rename a pane by id.")]
    async fn rename_pane(
        &self,
        Parameters(p): Parameters<RenamePaneParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "pane.rename", "session": p.session, "pane_id": p.pane_id, "name": p.name,
        }))
        .await
    }

    #[tool(description = "Resize a pane: grow or shrink it, optionally in a direction.")]
    async fn resize_pane(
        &self,
        Parameters(p): Parameters<ResizePaneParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "pane.resize", "session": p.session, "pane_id": p.pane_id,
            "resize": p.resize, "direction": p.direction,
        }))
        .await
    }

    // --- input --------------------------------------------------------------

    #[tool(
        description = "Type text into a pane, as if pasted. Writes bytes only — never changes which \
                        pane or tab is focused, whether addressed by pane_id or left to default to \
                        the currently focused pane. Use focus_pane/focus_tab first if you need focus \
                        itself to move. For an interactive TUI (a plugin pane, or an app driven by \
                        arrow keys), use send_keys instead — this delivers keystrokes, not pasted text."
    )]
    async fn send_text(
        &self,
        Parameters(p): Parameters<SendTextParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "input.text", "session": p.session, "pane_id": p.pane_id, "text": p.text,
        }))
        .await
    }

    #[tool(
        description = "Send named keys to a pane in order, e.g. [\"ctrl-c\"] or [\"down\", \"enter\"]. \
                        Writes bytes only — never changes which pane or tab is focused. Use \
                        focus_pane/focus_tab first if you need focus itself to move."
    )]
    async fn send_keys(
        &self,
        Parameters(p): Parameters<SendKeysParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "input.keys", "session": p.session, "pane_id": p.pane_id, "keys": p.keys,
        }))
        .await
    }

    #[tool(
        description = "Send a mouse event at (x, y) — column, row, both 0-based, within the \
                        session's currently focused tab. Unlike send_text/send_keys there is no \
                        pane_id to target directly, since a mouse position is inherently relative \
                        to whatever is currently visible — a click can itself change pane focus or \
                        selection, the same way it would for a person clicking with a real mouse."
    )]
    async fn send_mouse(
        &self,
        Parameters(p): Parameters<SendMouseParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "input.mouse", "session": p.session, "kind": p.kind,
            "x": p.x, "y": p.y, "button": p.button,
        }))
        .await
    }

    // --- screen ---------------------------------------------------------------

    #[tool(
        description = "Read a pane's current on-screen content. Auto-subscribes to it first if it \
                        was not already followed."
    )]
    async fn read_screen(
        &self,
        Parameters(p): Parameters<ReadScreenParams>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot_command = json!({
            "cmd": "screen.snapshot", "session": p.session, "pane_id": p.pane_id,
        });
        let mut snapshot = self.api.call(snapshot_command.clone()).await;
        if snapshot.is_err() {
            // Not yet followed — subscribe, then retry. The subscription's
            // baseline arrives asynchronously (a render event, not part of
            // the subscribe reply), so give it a few short beats rather than
            // trying once and giving up.
            //
            // A subscribe failure (e.g. no such pane) is usually more
            // specific than whatever the snapshot attempt above returned —
            // surface it directly rather than falling through to a vaguer
            // "still couldn't snapshot" error.
            //
            // With no `pane_id` given, the target is "whichever pane is
            // focused" — that can only be resolved session-side, so follow
            // every pane instead of naming one.
            let subscribe_command = match &p.pane_id {
                Some(pane_id) => json!({
                    "cmd": "screen.subscribe", "session": p.session,
                    "pane_ids": [pane_id],
                }),
                None => json!({"cmd": "screen.subscribe", "session": p.session}),
            };
            if let Err(e) = self.api.call(subscribe_command).await {
                return err_result(e);
            }
            for _ in 0..8 {
                snapshot = self.api.call(snapshot_command.clone()).await;
                if snapshot.is_ok() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        match snapshot {
            Ok(value) => ok_json(value),
            Err(e) => err_result(e),
        }
    }

    #[tool(
        description = "A pane's recorded screen changes as git-style unified diffs, oldest first. \
                        Pass `since` (a version from a previous call) to resume from where you left off."
    )]
    async fn screen_history(
        &self,
        Parameters(p): Parameters<ScreenHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        self.run(json!({
            "cmd": "screen.history", "session": p.session, "pane_id": p.pane_id,
            "since": p.since, "limit": p.limit,
        }))
        .await
    }
}

#[tool_handler]
impl ServerHandler for ZellijTools {
    fn get_info(&self) -> ServerInfo {
        // Not `Implementation::from_build_env()`: that reads `env!()` at the
        // point it is *defined*, inside rmcp's own source — so it always
        // reports rmcp's own name and version, not the crate calling it.
        let server_info = Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
            .with_description("Zellij remote-control API exposed as MCP tools");
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(server_info)
            .with_protocol_version(ProtocolVersion::V_2025_03_26)
            .with_instructions(
                "Tools for driving Zellij terminal sessions through the zellij-api WebSocket \
                 control plane. Call `attach_session` first to see what's on screen — there is no \
                 real terminal here, so that is how you get oriented. `list_sessions` to see what \
                 exists, `create_session`/`create_pane` to start something, `send_text`/`send_keys` \
                 to drive it, `read_screen`/`screen_history` to see what happened. \
                 `send_text`/`send_keys`/`send_mouse` only write — they never move focus, even when \
                 addressed to a pane_id in a different tab than the one currently focused. Only \
                 `focus_pane`/`focus_tab` actually change what's focused; call one of those first if \
                 focus itself needs to move, not just the pane a write lands in."
                    .to_string(),
            )
    }
}
