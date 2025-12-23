use crate::{
    client_server_contract::client_server_contract::{
        client_to_server_msg, server_to_client_msg, ActionMsg, AttachClientMsg,
        AttachWatcherClientMsg, BackgroundColorMsg, CliPipeOutputMsg, ClientExitedMsg,
        ClientToServerMsg as ProtoClientToServerMsg, ColorRegistersMsg, ConfigFileUpdatedMsg,
        ConnStatusMsg, ConnectedMsg, DetachSessionMsg, ExitMsg, ExitReason as ProtoExitReason,
        FailedToStartWebServerMsg, FirstClientConnectedMsg, ForegroundColorMsg,
        InputMode as ProtoInputMode, KeyMsg, KillSessionMsg, LogErrorMsg, LogMsg,
        QueryTerminalSizeMsg, RenamedSessionMsg, RenderMsg,
        ServerToClientMsg as ProtoServerToClientMsg, StartWebServerMsg, SwitchSessionMsg,
        TerminalPixelDimensionsMsg, TerminalResizeMsg, UnblockCliPipeInputMsg,
        UnblockInputThreadMsg, WebServerStartedMsg,
    },
    data::InputMode,
    errors::prelude::*,
    ipc::{
        ClientToServerMsg, ColorRegister, ExitReason, PaneReference, PixelDimensions,
        ServerToClientMsg,
    },
};
use std::collections::BTreeMap;
use std::path::PathBuf;

// Convert Rust ClientToServerMsg to protobuf
impl From<ClientToServerMsg> for ProtoClientToServerMsg {
    fn from(msg: ClientToServerMsg) -> Self {
        let message = match msg {
            ClientToServerMsg::DetachSession { client_ids } => {
                client_to_server_msg::Message::DetachSession(DetachSessionMsg {
                    client_ids: client_ids.into_iter().map(|id| id as u32).collect(),
                })
            },
            ClientToServerMsg::TerminalPixelDimensions { pixel_dimensions } => {
                client_to_server_msg::Message::TerminalPixelDimensions(TerminalPixelDimensionsMsg {
                    pixel_dimensions: Some(pixel_dimensions.into()),
                })
            },
            ClientToServerMsg::BackgroundColor { color } => {
                client_to_server_msg::Message::BackgroundColor(BackgroundColorMsg { color })
            },
            ClientToServerMsg::ForegroundColor { color } => {
                client_to_server_msg::Message::ForegroundColor(ForegroundColorMsg { color })
            },
            ClientToServerMsg::ColorRegisters { color_registers } => {
                client_to_server_msg::Message::ColorRegisters(ColorRegistersMsg {
                    color_registers: color_registers.into_iter().map(|cr| cr.into()).collect(),
                })
            },
            ClientToServerMsg::TerminalResize { new_size } => {
                client_to_server_msg::Message::TerminalResize(TerminalResizeMsg {
                    new_size: Some(new_size.into()),
                })
            },
            ClientToServerMsg::FirstClientConnected {
                cli_assets,
                is_web_client,
            } => client_to_server_msg::Message::FirstClientConnected(FirstClientConnectedMsg {
                cli_assets: Some(cli_assets.into()),
                is_web_client,
            }),
            ClientToServerMsg::AttachClient {
                cli_assets,
                tab_position_to_focus,
                pane_to_focus,
                is_web_client,
            } => client_to_server_msg::Message::AttachClient(AttachClientMsg {
                cli_assets: Some(cli_assets.into()),
                tab_position_to_focus: tab_position_to_focus.map(|pos| pos as u32),
                pane_to_focus: pane_to_focus.map(|p| p.into()),
                is_web_client,
            }),
            ClientToServerMsg::AttachWatcherClient {
                terminal_size,
                is_web_client,
            } => client_to_server_msg::Message::AttachWatcherClient(AttachWatcherClientMsg {
                terminal_size: Some(terminal_size.into()),
                is_web_client,
            }),
            ClientToServerMsg::Action {
                action,
                terminal_id,
                client_id,
                is_cli_client,
            } => client_to_server_msg::Message::Action(ActionMsg {
                action: Some(action.into()),
                terminal_id,
                client_id: client_id.map(|id| id as u32),
                is_cli_client,
            }),
            ClientToServerMsg::Key {
                key,
                raw_bytes,
                is_kitty_keyboard_protocol,
            } => client_to_server_msg::Message::Key(KeyMsg {
                key: Some(key.into()),
                raw_bytes: raw_bytes.into_iter().map(|b| b as u32).collect(),
                is_kitty_keyboard_protocol,
            }),
            ClientToServerMsg::ClientExited => {
                client_to_server_msg::Message::ClientExited(ClientExitedMsg {})
            },
            ClientToServerMsg::KillSession => {
                client_to_server_msg::Message::KillSession(KillSessionMsg {})
            },
            ClientToServerMsg::ConnStatus => {
                client_to_server_msg::Message::ConnStatus(ConnStatusMsg {})
            },
            ClientToServerMsg::WebServerStarted { base_url } => {
                client_to_server_msg::Message::WebServerStarted(WebServerStartedMsg { base_url })
            },
            ClientToServerMsg::FailedToStartWebServer { error } => {
                client_to_server_msg::Message::FailedToStartWebServer(FailedToStartWebServerMsg {
                    error,
                })
            },
        };

        ProtoClientToServerMsg {
            message: Some(message),
        }
    }
}

// Convert protobuf ClientToServerMsg to Rust
impl TryFrom<ProtoClientToServerMsg> for ClientToServerMsg {
    type Error = anyhow::Error;

    fn try_from(msg: ProtoClientToServerMsg) -> Result<Self> {
        match msg.message {
            Some(client_to_server_msg::Message::DetachSession(detach)) => {
                Ok(ClientToServerMsg::DetachSession {
                    client_ids: detach.client_ids.into_iter().map(|id| id as u16).collect(),
                })
            },
            Some(client_to_server_msg::Message::TerminalPixelDimensions(pixel_dims)) => {
                Ok(ClientToServerMsg::TerminalPixelDimensions {
                    pixel_dimensions: pixel_dims
                        .pixel_dimensions
                        .ok_or_else(|| anyhow!("Missing pixel_dimensions"))?
                        .try_into()?,
                })
            },
            Some(client_to_server_msg::Message::BackgroundColor(bg_color)) => {
                Ok(ClientToServerMsg::BackgroundColor {
                    color: bg_color.color,
                })
            },
            Some(client_to_server_msg::Message::ForegroundColor(fg_color)) => {
                Ok(ClientToServerMsg::ForegroundColor {
                    color: fg_color.color,
                })
            },
            Some(client_to_server_msg::Message::ColorRegisters(color_regs)) => {
                Ok(ClientToServerMsg::ColorRegisters {
                    color_registers: color_regs
                        .color_registers
                        .into_iter()
                        .map(|cr| cr.try_into())
                        .collect::<Result<Vec<_>>>()?,
                })
            },
            Some(client_to_server_msg::Message::TerminalResize(resize)) => {
                Ok(ClientToServerMsg::TerminalResize {
                    new_size: resize
                        .new_size
                        .ok_or_else(|| anyhow!("Missing new_size"))?
                        .try_into()?,
                })
            },
            Some(client_to_server_msg::Message::FirstClientConnected(first_client)) => {
                Ok(ClientToServerMsg::FirstClientConnected {
                    cli_assets: first_client
                        .cli_assets
                        .ok_or_else(|| anyhow!("Missing cli_assets"))?
                        .try_into()?,
                    is_web_client: first_client.is_web_client,
                })
            },
            Some(client_to_server_msg::Message::AttachClient(attach)) => {
                Ok(ClientToServerMsg::AttachClient {
                    cli_assets: attach
                        .cli_assets
                        .ok_or_else(|| anyhow!("Missing cli_assets"))?
                        .try_into()?,
                    tab_position_to_focus: attach.tab_position_to_focus.map(|pos| pos as usize),
                    pane_to_focus: attach.pane_to_focus.map(|p| p.try_into()).transpose()?,
                    is_web_client: attach.is_web_client,
                })
            },
            Some(client_to_server_msg::Message::AttachWatcherClient(attach_watcher)) => {
                Ok(ClientToServerMsg::AttachWatcherClient {
                    terminal_size: attach_watcher
                        .terminal_size
                        .ok_or_else(|| anyhow::anyhow!("Missing terminal_size"))?
                        .try_into()?,
                    is_web_client: attach_watcher.is_web_client,
                })
            },
            Some(client_to_server_msg::Message::Action(action)) => Ok(ClientToServerMsg::Action {
                action: action
                    .action
                    .ok_or_else(|| anyhow!("Missing action"))?
                    .try_into()?,
                terminal_id: action.terminal_id,
                client_id: action.client_id.map(|id| id as u16),
                is_cli_client: action.is_cli_client,
            }),
            Some(client_to_server_msg::Message::Key(key)) => Ok(ClientToServerMsg::Key {
                key: key.key.ok_or_else(|| anyhow!("Missing key"))?.try_into()?,
                raw_bytes: key.raw_bytes.into_iter().map(|b| b as u8).collect(),
                is_kitty_keyboard_protocol: key.is_kitty_keyboard_protocol,
            }),
            Some(client_to_server_msg::Message::ClientExited(_)) => {
                Ok(ClientToServerMsg::ClientExited)
            },
            Some(client_to_server_msg::Message::KillSession(_)) => {
                Ok(ClientToServerMsg::KillSession)
            },
            Some(client_to_server_msg::Message::ConnStatus(_)) => Ok(ClientToServerMsg::ConnStatus),
            Some(client_to_server_msg::Message::WebServerStarted(web_server)) => {
                Ok(ClientToServerMsg::WebServerStarted {
                    base_url: web_server.base_url,
                })
            },
            Some(client_to_server_msg::Message::FailedToStartWebServer(failed)) => {
                Ok(ClientToServerMsg::FailedToStartWebServer {
                    error: failed.error,
                })
            },
            None => Err(anyhow!("Empty ClientToServerMsg message")),
        }
    }
}

// Convert Rust ServerToClientMsg to protobuf
impl From<ServerToClientMsg> for ProtoServerToClientMsg {
    fn from(msg: ServerToClientMsg) -> Self {
        let message = match msg {
            ServerToClientMsg::Render { content } => {
                server_to_client_msg::Message::Render(RenderMsg { content })
            },
            ServerToClientMsg::UnblockInputThread => {
                server_to_client_msg::Message::UnblockInputThread(UnblockInputThreadMsg {})
            },
            ServerToClientMsg::Exit { exit_reason } => {
                let (proto_exit_reason, payload) = match exit_reason {
                    ExitReason::Error(ref msg) => (ProtoExitReason::Error, Some(msg.clone())),
                    ExitReason::CustomExitStatus(status) => {
                        (ProtoExitReason::CustomExitStatus, Some(status.to_string()))
                    },
                    other => (ProtoExitReason::from(other), None),
                };
                server_to_client_msg::Message::Exit(ExitMsg {
                    exit_reason: proto_exit_reason as i32,
                    payload,
                })
            },
            ServerToClientMsg::Connected => {
                server_to_client_msg::Message::Connected(ConnectedMsg {})
            },
            ServerToClientMsg::Log { lines } => {
                server_to_client_msg::Message::Log(LogMsg { lines })
            },
            ServerToClientMsg::LogError { lines } => {
                server_to_client_msg::Message::LogError(LogErrorMsg { lines })
            },
            ServerToClientMsg::SwitchSession { connect_to_session } => {
                server_to_client_msg::Message::SwitchSession(SwitchSessionMsg {
                    connect_to_session: Some(connect_to_session.into()),
                })
            },
            ServerToClientMsg::UnblockCliPipeInput { pipe_name } => {
                server_to_client_msg::Message::UnblockCliPipeInput(UnblockCliPipeInputMsg {
                    pipe_name,
                })
            },
            ServerToClientMsg::CliPipeOutput { pipe_name, output } => {
                server_to_client_msg::Message::CliPipeOutput(CliPipeOutputMsg { pipe_name, output })
            },
            ServerToClientMsg::QueryTerminalSize => {
                server_to_client_msg::Message::QueryTerminalSize(QueryTerminalSizeMsg {})
            },
            ServerToClientMsg::StartWebServer => {
                server_to_client_msg::Message::StartWebServer(StartWebServerMsg {})
            },
            ServerToClientMsg::RenamedSession { name } => {
                server_to_client_msg::Message::RenamedSession(RenamedSessionMsg { name })
            },
            ServerToClientMsg::ConfigFileUpdated => {
                server_to_client_msg::Message::ConfigFileUpdated(ConfigFileUpdatedMsg {})
            },
        };

        ProtoServerToClientMsg {
            message: Some(message),
        }
    }
}

// Convert protobuf ServerToClientMsg to Rust
impl TryFrom<ProtoServerToClientMsg> for ServerToClientMsg {
    type Error = anyhow::Error;

    fn try_from(msg: ProtoServerToClientMsg) -> Result<Self> {
        match msg.message {
            Some(server_to_client_msg::Message::Render(render)) => Ok(ServerToClientMsg::Render {
                content: render.content,
            }),
            Some(server_to_client_msg::Message::UnblockInputThread(_)) => {
                Ok(ServerToClientMsg::UnblockInputThread)
            },
            Some(server_to_client_msg::Message::Exit(exit)) => {
                let proto_exit_reason = ProtoExitReason::from_i32(exit.exit_reason)
                    .ok_or_else(|| anyhow!("Invalid exit_reason"))?;

                let exit_reason = match proto_exit_reason {
                    ProtoExitReason::Error => {
                        let error_msg =
                            exit.payload.unwrap_or_else(|| "Protobuf error".to_string());
                        ExitReason::Error(error_msg)
                    },
                    ProtoExitReason::CustomExitStatus => {
                        let status_str = exit.payload.unwrap_or_else(|| "0".to_string());
                        let status = status_str
                            .parse::<i32>()
                            .map_err(|_| anyhow!("Invalid custom exit status: {}", status_str))?;
                        ExitReason::CustomExitStatus(status)
                    },
                    other => other.try_into()?,
                };

                Ok(ServerToClientMsg::Exit { exit_reason })
            },
            Some(server_to_client_msg::Message::Connected(_)) => Ok(ServerToClientMsg::Connected),
            Some(server_to_client_msg::Message::Log(log)) => {
                Ok(ServerToClientMsg::Log { lines: log.lines })
            },
            Some(server_to_client_msg::Message::LogError(log_error)) => {
                Ok(ServerToClientMsg::LogError {
                    lines: log_error.lines,
                })
            },
            Some(server_to_client_msg::Message::SwitchSession(switch)) => {
                Ok(ServerToClientMsg::SwitchSession {
                    connect_to_session: switch
                        .connect_to_session
                        .ok_or_else(|| anyhow!("Missing connect_to_session"))?
                        .try_into()?,
                })
            },
            Some(server_to_client_msg::Message::UnblockCliPipeInput(unblock)) => {
                Ok(ServerToClientMsg::UnblockCliPipeInput {
                    pipe_name: unblock.pipe_name,
                })
            },
            Some(server_to_client_msg::Message::CliPipeOutput(pipe_output)) => {
                Ok(ServerToClientMsg::CliPipeOutput {
                    pipe_name: pipe_output.pipe_name,
                    output: pipe_output.output,
                })
            },
            Some(server_to_client_msg::Message::QueryTerminalSize(_)) => {
                Ok(ServerToClientMsg::QueryTerminalSize)
            },
            Some(server_to_client_msg::Message::StartWebServer(_)) => {
                Ok(ServerToClientMsg::StartWebServer)
            },
            Some(server_to_client_msg::Message::RenamedSession(renamed)) => {
                Ok(ServerToClientMsg::RenamedSession { name: renamed.name })
            },
            Some(server_to_client_msg::Message::ConfigFileUpdated(_)) => {
                Ok(ServerToClientMsg::ConfigFileUpdated)
            },
            None => Err(anyhow!("Empty ServerToClientMsg message")),
        }
    }
}

// Basic type conversions
impl From<crate::pane_size::Size> for crate::client_server_contract::client_server_contract::Size {
    fn from(size: crate::pane_size::Size) -> Self {
        Self {
            cols: size.cols as u32,
            rows: size.rows as u32,
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::Size>
    for crate::pane_size::Size
{
    type Error = anyhow::Error;
    fn try_from(size: crate::client_server_contract::client_server_contract::Size) -> Result<Self> {
        Ok(Self {
            rows: size.rows as usize,
            cols: size.cols as usize,
        })
    }
}

impl From<PixelDimensions>
    for crate::client_server_contract::client_server_contract::PixelDimensions
{
    fn from(pixel_dims: PixelDimensions) -> Self {
        Self {
            text_area_size: pixel_dims.text_area_size.map(|size| {
                crate::client_server_contract::client_server_contract::SizeInPixels {
                    width: size.width as u32,
                    height: size.height as u32,
                }
            }),
            character_cell_size: pixel_dims.character_cell_size.map(|size| {
                crate::client_server_contract::client_server_contract::SizeInPixels {
                    width: size.width as u32,
                    height: size.height as u32,
                }
            }),
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::PixelDimensions>
    for PixelDimensions
{
    type Error = anyhow::Error;
    fn try_from(
        pixel_dims: crate::client_server_contract::client_server_contract::PixelDimensions,
    ) -> Result<Self> {
        Ok(Self {
            text_area_size: pixel_dims
                .text_area_size
                .map(|size| crate::pane_size::SizeInPixels {
                    width: size.width as usize,
                    height: size.height as usize,
                }),
            character_cell_size: pixel_dims.character_cell_size.map(|size| {
                crate::pane_size::SizeInPixels {
                    width: size.width as usize,
                    height: size.height as usize,
                }
            }),
        })
    }
}

impl From<PaneReference> for crate::client_server_contract::client_server_contract::PaneReference {
    fn from(pane_ref: PaneReference) -> Self {
        Self {
            pane_id: pane_ref.pane_id,
            is_plugin: pane_ref.is_plugin,
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::PaneReference>
    for PaneReference
{
    type Error = anyhow::Error;
    fn try_from(
        pane_ref: crate::client_server_contract::client_server_contract::PaneReference,
    ) -> Result<Self> {
        Ok(Self {
            pane_id: pane_ref.pane_id,
            is_plugin: pane_ref.is_plugin,
        })
    }
}

impl From<ColorRegister> for crate::client_server_contract::client_server_contract::ColorRegister {
    fn from(color_reg: ColorRegister) -> Self {
        Self {
            index: color_reg.index as u32,
            color: color_reg.color,
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::ColorRegister>
    for ColorRegister
{
    type Error = anyhow::Error;
    fn try_from(
        color_reg: crate::client_server_contract::client_server_contract::ColorRegister,
    ) -> Result<Self> {
        Ok(Self {
            index: color_reg.index as usize,
            color: color_reg.color,
        })
    }
}

impl From<crate::input::cli_assets::CliAssets>
    for crate::client_server_contract::client_server_contract::CliAssets
{
    fn from(cli_assets: crate::input::cli_assets::CliAssets) -> Self {
        Self {
            config_file_path: cli_assets
                .config_file_path
                .map(|p| p.to_string_lossy().to_string()),
            config_dir: cli_assets
                .config_dir
                .map(|p| p.to_string_lossy().to_string()),
            should_ignore_config: cli_assets.should_ignore_config,
            configuration_options: cli_assets.configuration_options.map(|o| o.into()),
            layout: cli_assets.layout.map(|l| l.into()),
            terminal_window_size: Some(cli_assets.terminal_window_size.into()),
            data_dir: cli_assets.data_dir.map(|p| p.to_string_lossy().to_string()),
            is_debug: cli_assets.is_debug,
            max_panes: cli_assets.max_panes.map(|m| m as u32),
            force_run_layout_commands: cli_assets.force_run_layout_commands,
            cwd: cli_assets.cwd.map(|p| p.to_string_lossy().to_string()),
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::CliAssets>
    for crate::input::cli_assets::CliAssets
{
    type Error = anyhow::Error;
    fn try_from(
        cli_assets: crate::client_server_contract::client_server_contract::CliAssets,
    ) -> Result<Self> {
        Ok(Self {
            config_file_path: cli_assets.config_file_path.map(PathBuf::from),
            config_dir: cli_assets.config_dir.map(PathBuf::from),
            should_ignore_config: cli_assets.should_ignore_config,
            configuration_options: cli_assets
                .configuration_options
                .map(|o| o.try_into())
                .transpose()?,
            layout: cli_assets.layout.map(|l| l.try_into()).transpose()?,
            terminal_window_size: cli_assets
                .terminal_window_size
                .ok_or_else(|| anyhow!("CliAssets missing terminal_window_size"))?
                .try_into()?,
            data_dir: cli_assets.data_dir.map(PathBuf::from),
            is_debug: cli_assets.is_debug,
            max_panes: cli_assets.max_panes.map(|m| m as usize),
            force_run_layout_commands: cli_assets.force_run_layout_commands,
            cwd: cli_assets.cwd.map(PathBuf::from),
        })
    }
}

impl From<crate::input::options::Options>
    for crate::client_server_contract::client_server_contract::Options
{
    fn from(options: crate::input::options::Options) -> Self {
        use crate::client_server_contract::client_server_contract::{
            Clipboard as ProtoClipboard, OnForceClose as ProtoOnForceClose,
            WebSharing as ProtoWebSharing,
        };

        Self {
            simplified_ui: options.simplified_ui,
            theme: options.theme,
            default_mode: options.default_mode.map(|m| input_mode_to_proto_i32(m)),
            default_shell: options
                .default_shell
                .map(|p| p.to_string_lossy().to_string()),
            default_cwd: options.default_cwd.map(|p| p.to_string_lossy().to_string()),
            default_layout: options
                .default_layout
                .map(|p| p.to_string_lossy().to_string()),
            layout_dir: options.layout_dir.map(|p| p.to_string_lossy().to_string()),
            theme_dir: options.theme_dir.map(|p| p.to_string_lossy().to_string()),
            mouse_mode: options.mouse_mode,
            pane_frames: options.pane_frames,
            mirror_session: options.mirror_session,
            on_force_close: options.on_force_close.map(|o| match o {
                crate::input::options::OnForceClose::Quit => ProtoOnForceClose::Quit as i32,
                crate::input::options::OnForceClose::Detach => ProtoOnForceClose::Detach as i32,
            }),
            scroll_buffer_size: options.scroll_buffer_size.map(|s| s as u32),
            copy_command: options.copy_command,
            copy_clipboard: options.copy_clipboard.map(|c| match c {
                crate::input::options::Clipboard::System => ProtoClipboard::System as i32,
                crate::input::options::Clipboard::Primary => ProtoClipboard::Primary as i32,
            }),
            copy_on_select: options.copy_on_select,
            scrollback_editor: options
                .scrollback_editor
                .map(|p| p.to_string_lossy().to_string()),
            session_name: options.session_name,
            attach_to_session: options.attach_to_session,
            auto_layout: options.auto_layout,
            session_serialization: options.session_serialization,
            serialize_pane_viewport: options.serialize_pane_viewport,
            scrollback_lines_to_serialize: options.scrollback_lines_to_serialize.map(|s| s as u32),
            styled_underlines: options.styled_underlines,
            serialization_interval: options.serialization_interval,
            disable_session_metadata: options.disable_session_metadata,
            support_kitty_keyboard_protocol: options.support_kitty_keyboard_protocol,
            web_server: options.web_server,
            web_sharing: options.web_sharing.map(|w| match w {
                crate::data::WebSharing::On => ProtoWebSharing::On as i32,
                crate::data::WebSharing::Off => ProtoWebSharing::Off as i32,
                crate::data::WebSharing::Disabled => ProtoWebSharing::Disabled as i32,
            }),
            stacked_resize: options.stacked_resize,
            show_startup_tips: options.show_startup_tips,
            show_release_notes: options.show_release_notes,
            advanced_mouse_actions: options.advanced_mouse_actions,
            web_server_ip: options.web_server_ip.map(|ip| ip.to_string()),
            web_server_port: options.web_server_port.map(|p| p as u32),
            web_server_cert: options
                .web_server_cert
                .map(|p| p.to_string_lossy().to_string()),
            web_server_key: options
                .web_server_key
                .map(|p| p.to_string_lossy().to_string()),
            enforce_https_for_localhost: options.enforce_https_for_localhost,
            post_command_discovery_hook: options.post_command_discovery_hook,
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::Options>
    for crate::input::options::Options
{
    type Error = anyhow::Error;
    fn try_from(
        options: crate::client_server_contract::client_server_contract::Options,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::{
            Clipboard as ProtoClipboard, OnForceClose as ProtoOnForceClose,
            WebSharing as ProtoWebSharing,
        };

        Ok(Self {
            simplified_ui: options.simplified_ui,
            theme: options.theme,
            default_mode: options
                .default_mode
                .map(|m| proto_i32_to_input_mode(m))
                .transpose()?,
            default_shell: options.default_shell.map(std::path::PathBuf::from),
            default_cwd: options.default_cwd.map(std::path::PathBuf::from),
            default_layout: options.default_layout.map(std::path::PathBuf::from),
            layout_dir: options.layout_dir.map(std::path::PathBuf::from),
            theme_dir: options.theme_dir.map(std::path::PathBuf::from),
            mouse_mode: options.mouse_mode,
            pane_frames: options.pane_frames,
            mirror_session: options.mirror_session,
            on_force_close: options
                .on_force_close
                .map(|o| match ProtoOnForceClose::from_i32(o) {
                    Some(ProtoOnForceClose::Quit) => Ok(crate::input::options::OnForceClose::Quit),
                    Some(ProtoOnForceClose::Detach) => {
                        Ok(crate::input::options::OnForceClose::Detach)
                    },
                    _ => Err(anyhow!("Invalid OnForceClose value: {}", o)),
                })
                .transpose()?,
            scroll_buffer_size: options.scroll_buffer_size.map(|s| s as usize),
            copy_command: options.copy_command,
            copy_clipboard: options
                .copy_clipboard
                .map(|c| match ProtoClipboard::from_i32(c) {
                    Some(ProtoClipboard::System) => Ok(crate::input::options::Clipboard::System),
                    Some(ProtoClipboard::Primary) => Ok(crate::input::options::Clipboard::Primary),
                    _ => Err(anyhow!("Invalid Clipboard value: {}", c)),
                })
                .transpose()?,
            copy_on_select: options.copy_on_select,
            scrollback_editor: options.scrollback_editor.map(std::path::PathBuf::from),
            session_name: options.session_name,
            attach_to_session: options.attach_to_session,
            auto_layout: options.auto_layout,
            session_serialization: options.session_serialization,
            serialize_pane_viewport: options.serialize_pane_viewport,
            scrollback_lines_to_serialize: options
                .scrollback_lines_to_serialize
                .map(|s| s as usize),
            styled_underlines: options.styled_underlines,
            serialization_interval: options.serialization_interval,
            disable_session_metadata: options.disable_session_metadata,
            support_kitty_keyboard_protocol: options.support_kitty_keyboard_protocol,
            web_server: options.web_server,
            web_sharing: options
                .web_sharing
                .map(|w| match ProtoWebSharing::from_i32(w) {
                    Some(ProtoWebSharing::On) => Ok(crate::data::WebSharing::On),
                    Some(ProtoWebSharing::Off) => Ok(crate::data::WebSharing::Off),
                    Some(ProtoWebSharing::Disabled) => Ok(crate::data::WebSharing::Disabled),
                    _ => Err(anyhow!("Invalid WebSharing value: {}", w)),
                })
                .transpose()?,
            stacked_resize: options.stacked_resize,
            show_startup_tips: options.show_startup_tips,
            show_release_notes: options.show_release_notes,
            advanced_mouse_actions: options.advanced_mouse_actions,
            web_server_ip: options
                .web_server_ip
                .map(|ip| ip.parse())
                .transpose()
                .map_err(|e| anyhow!("Invalid IP address: {}", e))?,
            web_server_port: options.web_server_port.map(|p| p as u16),
            web_server_cert: options.web_server_cert.map(std::path::PathBuf::from),
            web_server_key: options.web_server_key.map(std::path::PathBuf::from),
            enforce_https_for_localhost: options.enforce_https_for_localhost,
            post_command_discovery_hook: options.post_command_discovery_hook,
        })
    }
}

// Complete Action conversion implementation - all 91 variants
impl From<crate::input::actions::Action>
    for crate::client_server_contract::client_server_contract::Action
{
    fn from(action: crate::input::actions::Action) -> Self {
        use crate::client_server_contract::client_server_contract::{
            action::ActionType, BreakPaneAction, BreakPaneLeftAction, BreakPaneRightAction,
            ChangeFloatingPaneCoordinatesAction, ClearScreenAction, CliPipeAction,
            CloseFocusAction, ClosePluginPaneAction, CloseTabAction, CloseTerminalPaneAction,
            ConfirmAction, CopyAction, DenyAction, DetachAction, DumpLayoutAction,
            DumpScreenAction, EditFileAction, EditScrollbackAction, FocusNextPaneAction,
            FocusPluginPaneWithIdAction, FocusPreviousPaneAction, FocusTerminalPaneWithIdAction,
            GoToNextTabAction, GoToPreviousTabAction, GoToTabAction, GoToTabNameAction,
            HalfPageScrollDownAction, HalfPageScrollUpAction, KeybindPipeAction,
            LaunchOrFocusPluginAction, LaunchPluginAction, ListClientsAction, MouseEventAction,
            MoveFocusAction, MoveFocusOrTabAction, MovePaneAction, MovePaneBackwardsAction,
            MoveTabAction, NewBlockingPaneAction, NewFloatingPaneAction,
            NewFloatingPluginPaneAction, NewInPlacePaneAction, NewInPlacePluginPaneAction,
            NewPaneAction, NewStackedPaneAction, NewTabAction, NewTiledPaneAction,
            NewTiledPluginPaneAction, NextSwapLayoutAction, NoOpAction, OverrideLayoutAction,
            PageScrollDownAction, PageScrollUpAction, PaneIdWithPlugin, PaneNameInputAction,
            PreviousSwapLayoutAction, QueryTabNamesAction, QuitAction, RenamePluginPaneAction,
            RenameSessionAction, RenameTabAction, RenameTerminalPaneAction, ResizeAction,
            RunAction, ScrollDownAction, ScrollDownAtAction, ScrollToBottomAction,
            ScrollToTopAction, ScrollUpAction, ScrollUpAtAction, SearchAction, SearchInputAction,
            SearchToggleOptionAction, SkipConfirmAction, StackPanesAction,
            StartOrReloadPluginAction, SwitchFocusAction, SwitchModeForAllClientsAction,
            SwitchSessionAction, SwitchToModeAction, TabNameInputAction, ToggleActiveSyncTabAction,
            ToggleFloatingPanesAction, ToggleFocusFullscreenAction, ToggleGroupMarkingAction,
            ToggleMouseModeAction, TogglePaneEmbedOrFloatingAction, TogglePaneFramesAction,
            TogglePaneInGroupAction, TogglePanePinnedAction, ToggleTabAction, UndoRenamePaneAction,
            UndoRenameTabAction, WriteAction, WriteCharsAction,
        };
        use std::collections::HashMap;

        let action_type = match action {
            crate::input::actions::Action::Quit => ActionType::Quit(QuitAction {}),
            crate::input::actions::Action::Write {
                key_with_modifier,
                bytes,
                is_kitty_keyboard_protocol,
            } => ActionType::Write(WriteAction {
                key_with_modifier: key_with_modifier.map(|k| k.into()),
                bytes: bytes.into_iter().map(|b| b as u32).collect(),
                is_kitty_keyboard_protocol,
            }),
            crate::input::actions::Action::WriteChars { chars } => {
                ActionType::WriteChars(WriteCharsAction { chars })
            },
            crate::input::actions::Action::SwitchToMode { input_mode } => {
                ActionType::SwitchToMode(SwitchToModeAction {
                    input_mode: input_mode_to_proto_i32(input_mode),
                })
            },
            crate::input::actions::Action::SwitchModeForAllClients { input_mode } => {
                ActionType::SwitchModeForAllClients(SwitchModeForAllClientsAction {
                    input_mode: input_mode_to_proto_i32(input_mode),
                })
            },
            crate::input::actions::Action::Resize { resize, direction } => {
                ActionType::Resize(ResizeAction {
                    resize: resize_to_proto_i32(resize),
                    direction: direction.map(|d| direction_to_proto_i32(d)),
                })
            },
            crate::input::actions::Action::FocusNextPane => {
                ActionType::FocusNextPane(FocusNextPaneAction {})
            },
            crate::input::actions::Action::FocusPreviousPane => {
                ActionType::FocusPreviousPane(FocusPreviousPaneAction {})
            },
            crate::input::actions::Action::SwitchFocus => {
                ActionType::SwitchFocus(SwitchFocusAction {})
            },
            crate::input::actions::Action::MoveFocus { direction } => {
                ActionType::MoveFocus(MoveFocusAction {
                    direction: direction_to_proto_i32(direction),
                })
            },
            crate::input::actions::Action::MoveFocusOrTab { direction } => {
                ActionType::MoveFocusOrTab(MoveFocusOrTabAction {
                    direction: direction_to_proto_i32(direction),
                })
            },
            crate::input::actions::Action::MovePane { direction } => {
                ActionType::MovePane(MovePaneAction {
                    direction: direction.map(|d| direction_to_proto_i32(d)),
                })
            },
            crate::input::actions::Action::MovePaneBackwards => {
                ActionType::MovePaneBackwards(MovePaneBackwardsAction {})
            },
            crate::input::actions::Action::ClearScreen => {
                ActionType::ClearScreen(ClearScreenAction {})
            },
            crate::input::actions::Action::DumpScreen {
                file_path,
                include_scrollback,
            } => ActionType::DumpScreen(DumpScreenAction {
                file_path,
                include_scrollback,
            }),
            crate::input::actions::Action::DumpLayout => {
                ActionType::DumpLayout(DumpLayoutAction {})
            },
            crate::input::actions::Action::EditScrollback => {
                ActionType::EditScrollback(EditScrollbackAction {})
            },
            crate::input::actions::Action::ScrollUp => ActionType::ScrollUp(ScrollUpAction {}),
            crate::input::actions::Action::ScrollUpAt { position } => {
                ActionType::ScrollUpAt(ScrollUpAtAction {
                    position: Some(position.into()),
                })
            },
            crate::input::actions::Action::ScrollDown => {
                ActionType::ScrollDown(ScrollDownAction {})
            },
            crate::input::actions::Action::ScrollDownAt { position } => {
                ActionType::ScrollDownAt(ScrollDownAtAction {
                    position: Some(position.into()),
                })
            },
            crate::input::actions::Action::ScrollToBottom => {
                ActionType::ScrollToBottom(ScrollToBottomAction {})
            },
            crate::input::actions::Action::ScrollToTop => {
                ActionType::ScrollToTop(ScrollToTopAction {})
            },
            crate::input::actions::Action::PageScrollUp => {
                ActionType::PageScrollUp(PageScrollUpAction {})
            },
            crate::input::actions::Action::PageScrollDown => {
                ActionType::PageScrollDown(PageScrollDownAction {})
            },
            crate::input::actions::Action::HalfPageScrollUp => {
                ActionType::HalfPageScrollUp(HalfPageScrollUpAction {})
            },
            crate::input::actions::Action::HalfPageScrollDown => {
                ActionType::HalfPageScrollDown(HalfPageScrollDownAction {})
            },
            crate::input::actions::Action::ToggleFocusFullscreen => {
                ActionType::ToggleFocusFullscreen(ToggleFocusFullscreenAction {})
            },
            crate::input::actions::Action::TogglePaneFrames => {
                ActionType::TogglePaneFrames(TogglePaneFramesAction {})
            },
            crate::input::actions::Action::ToggleActiveSyncTab => {
                ActionType::ToggleActiveSyncTab(ToggleActiveSyncTabAction {})
            },
            crate::input::actions::Action::NewPane {
                direction,
                pane_name,
                start_suppressed,
            } => ActionType::NewPane(NewPaneAction {
                direction: direction.map(|d| direction_to_proto_i32(d)),
                pane_name,
                start_suppressed,
                near_current_pane: false,
            }),
            crate::input::actions::Action::EditFile {
                payload,
                direction,
                floating,
                in_place,
                start_suppressed,
                coordinates,
                near_current_pane,
            } => ActionType::EditFile(EditFileAction {
                payload: Some(payload.into()),
                direction: direction.map(|d| direction_to_proto_i32(d)),
                floating,
                in_place,
                start_suppressed,
                coordinates: coordinates.map(|c| c.into()),
                near_current_pane,
            }),
            crate::input::actions::Action::NewFloatingPane {
                command,
                pane_name,
                coordinates,
                near_current_pane,
            } => ActionType::NewFloatingPane(NewFloatingPaneAction {
                command: command.map(|c| c.into()),
                pane_name,
                coordinates: coordinates.map(|c| c.into()),
                near_current_pane,
            }),
            crate::input::actions::Action::NewTiledPane {
                direction,
                command,
                pane_name,
                near_current_pane,
            } => ActionType::NewTiledPane(NewTiledPaneAction {
                direction: direction.map(|d| direction_to_proto_i32(d)),
                command: command.map(|c| c.into()),
                pane_name,
                near_current_pane,
            }),
            crate::input::actions::Action::NewInPlacePane {
                command,
                pane_name,
                near_current_pane,
                pane_id_to_replace,
                close_replace_pane,
            } => ActionType::NewInPlacePane(NewInPlacePaneAction {
                command: command.map(|c| c.into()),
                pane_name,
                near_current_pane,
                pane_id_to_replace: pane_id_to_replace.and_then(|p| p.try_into().ok()),
                close_replace_pane,
            }),
            crate::input::actions::Action::NewStackedPane {
                command,
                pane_name,
                near_current_pane,
            } => ActionType::NewStackedPane(NewStackedPaneAction {
                command: command.map(|c| c.into()),
                pane_name,
                near_current_pane,
            }),
            crate::input::actions::Action::NewBlockingPane {
                placement,
                pane_name,
                command,
                unblock_condition,
                near_current_pane,
            } => ActionType::NewBlockingPane(NewBlockingPaneAction {
                placement: Some(placement.into()),
                pane_name,
                command: command.map(|c| c.into()),
                unblock_condition: unblock_condition.map(|c| unblock_condition_to_proto_i32(c)),
                near_current_pane,
            }),
            crate::input::actions::Action::TogglePaneEmbedOrFloating => {
                ActionType::TogglePaneEmbedOrFloating(TogglePaneEmbedOrFloatingAction {})
            },
            crate::input::actions::Action::ToggleFloatingPanes => {
                ActionType::ToggleFloatingPanes(ToggleFloatingPanesAction {})
            },
            crate::input::actions::Action::CloseFocus => {
                ActionType::CloseFocus(CloseFocusAction {})
            },
            crate::input::actions::Action::PaneNameInput { input } => {
                ActionType::PaneNameInput(PaneNameInputAction {
                    input: input.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::UndoRenamePane => {
                ActionType::UndoRenamePane(UndoRenamePaneAction {})
            },
            crate::input::actions::Action::NewTab {
                tiled_layout,
                floating_layouts,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
                should_change_focus_to_new_tab,
                cwd,
                initial_panes,
                first_pane_unblock_condition,
            } => ActionType::NewTab(NewTabAction {
                tiled_layout: tiled_layout.map(|l| l.into()),
                floating_layouts: floating_layouts.into_iter().map(|l| l.into()).collect(),
                swap_tiled_layouts: swap_tiled_layouts
                    .map(|layouts| layouts.into_iter().map(|l| l.into()).collect())
                    .unwrap_or_default(),
                swap_floating_layouts: swap_floating_layouts
                    .map(|layouts| layouts.into_iter().map(|l| l.into()).collect())
                    .unwrap_or_default(),
                tab_name,
                should_change_focus_to_new_tab,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
                initial_panes: initial_panes
                    .map(|panes| panes.into_iter().map(|p| p.into()).collect())
                    .unwrap_or_default(),
                first_pane_unblock_condition: first_pane_unblock_condition
                    .map(|c| unblock_condition_to_proto_i32(c)),
            }),
            crate::input::actions::Action::NoOp => ActionType::NoOp(NoOpAction {}),
            crate::input::actions::Action::GoToNextTab => {
                ActionType::GoToNextTab(GoToNextTabAction {})
            },
            crate::input::actions::Action::GoToPreviousTab => {
                ActionType::GoToPreviousTab(GoToPreviousTabAction {})
            },
            crate::input::actions::Action::CloseTab => ActionType::CloseTab(CloseTabAction {}),
            crate::input::actions::Action::GoToTab { index } => {
                ActionType::GoToTab(GoToTabAction { index })
            },
            crate::input::actions::Action::GoToTabName { name, create } => {
                ActionType::GoToTabName(GoToTabNameAction { name, create })
            },
            crate::input::actions::Action::ToggleTab => ActionType::ToggleTab(ToggleTabAction {}),
            crate::input::actions::Action::TabNameInput { input } => {
                ActionType::TabNameInput(TabNameInputAction {
                    input: input.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::UndoRenameTab => {
                ActionType::UndoRenameTab(UndoRenameTabAction {})
            },
            crate::input::actions::Action::MoveTab { direction } => {
                ActionType::MoveTab(MoveTabAction {
                    direction: direction_to_proto_i32(direction),
                })
            },
            crate::input::actions::Action::Run {
                command,
                near_current_pane,
            } => ActionType::Run(RunAction {
                command: Some(command.into()),
                near_current_pane,
            }),
            crate::input::actions::Action::Detach => ActionType::Detach(DetachAction {}),
            crate::input::actions::Action::SwitchSession {
                name,
                tab_position,
                pane_id,
                layout,
                cwd,
            } => ActionType::SwitchSession(SwitchSessionAction {
                name: name.clone(),
                tab_position: tab_position.map(|p| p as u32),
                pane_id: pane_id.map(|(id, is_plugin)| PaneIdWithPlugin {
                    pane_id: id,
                    is_plugin: is_plugin,
                }),
                layout: layout.as_ref().map(|l| l.clone().into()),
                cwd: cwd.as_ref().map(|p| p.to_string_lossy().to_string()),
            }),
            crate::input::actions::Action::LaunchOrFocusPlugin {
                plugin,
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                skip_cache,
            } => ActionType::LaunchOrFocusPlugin(LaunchOrFocusPluginAction {
                plugin: Some(plugin.into()),
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                skip_cache,
            }),
            crate::input::actions::Action::LaunchPlugin {
                plugin,
                should_float,
                should_open_in_place,
                skip_cache,
                cwd,
            } => ActionType::LaunchPlugin(LaunchPluginAction {
                plugin: Some(plugin.into()),
                should_float,
                should_open_in_place,
                skip_cache,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
            }),
            crate::input::actions::Action::MouseEvent { event } => {
                ActionType::MouseEvent(MouseEventAction {
                    event: Some(event.into()),
                })
            },
            crate::input::actions::Action::Copy => ActionType::Copy(CopyAction {}),
            crate::input::actions::Action::Confirm => ActionType::Confirm(ConfirmAction {}),
            crate::input::actions::Action::Deny => ActionType::Deny(DenyAction {}),
            crate::input::actions::Action::SkipConfirm { action } => {
                ActionType::SkipConfirm(Box::new(SkipConfirmAction {
                    action: Some(Box::new((*action).into())),
                }))
            },
            crate::input::actions::Action::SearchInput { input } => {
                ActionType::SearchInput(SearchInputAction {
                    input: input.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::Search { direction } => {
                ActionType::Search(SearchAction {
                    direction: search_direction_to_proto_i32(direction),
                })
            },
            crate::input::actions::Action::SearchToggleOption { option } => {
                ActionType::SearchToggleOption(SearchToggleOptionAction {
                    option: search_option_to_proto_i32(option),
                })
            },
            crate::input::actions::Action::ToggleMouseMode => {
                ActionType::ToggleMouseMode(ToggleMouseModeAction {})
            },
            crate::input::actions::Action::PreviousSwapLayout => {
                ActionType::PreviousSwapLayout(PreviousSwapLayoutAction {})
            },
            crate::input::actions::Action::NextSwapLayout => {
                ActionType::NextSwapLayout(NextSwapLayoutAction {})
            },
            crate::input::actions::Action::OverrideLayout {
                tiled_layout,
                floating_layouts,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
                retain_existing_terminal_panes,
                retain_existing_plugin_panes,
            } => ActionType::OverrideLayout(OverrideLayoutAction {
                tiled_layout: tiled_layout.map(|l| l.into()),
                floating_layouts: floating_layouts.into_iter().map(|l| l.into()).collect(),
                swap_tiled_layouts: swap_tiled_layouts
                    .map(|layouts| layouts.into_iter().map(|l| l.into()).collect())
                    .unwrap_or_default(),
                swap_floating_layouts: swap_floating_layouts
                    .map(|layouts| layouts.into_iter().map(|l| l.into()).collect())
                    .unwrap_or_default(),
                tab_name,
                retain_existing_terminal_panes,
                retain_existing_plugin_panes,
            }),
            crate::input::actions::Action::QueryTabNames => {
                ActionType::QueryTabNames(QueryTabNamesAction {})
            },
            crate::input::actions::Action::NewTiledPluginPane {
                plugin,
                pane_name,
                skip_cache,
                cwd,
            } => ActionType::NewTiledPluginPane(NewTiledPluginPaneAction {
                plugin: Some(plugin.into()),
                pane_name,
                skip_cache,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
            }),
            crate::input::actions::Action::NewFloatingPluginPane {
                plugin,
                pane_name,
                skip_cache,
                cwd,
                coordinates,
            } => ActionType::NewFloatingPluginPane(NewFloatingPluginPaneAction {
                plugin: Some(plugin.into()),
                pane_name,
                skip_cache,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
                coordinates: coordinates.map(|c| c.into()),
            }),
            crate::input::actions::Action::NewInPlacePluginPane {
                plugin,
                pane_name,
                skip_cache,
            } => ActionType::NewInPlacePluginPane(NewInPlacePluginPaneAction {
                plugin: Some(plugin.into()),
                pane_name,
                skip_cache,
            }),
            crate::input::actions::Action::StartOrReloadPlugin { plugin } => {
                ActionType::StartOrReloadPlugin(StartOrReloadPluginAction {
                    plugin: Some(plugin.into()),
                })
            },
            crate::input::actions::Action::CloseTerminalPane { pane_id } => {
                ActionType::CloseTerminalPane(CloseTerminalPaneAction { pane_id })
            },
            crate::input::actions::Action::ClosePluginPane { pane_id } => {
                ActionType::ClosePluginPane(ClosePluginPaneAction { pane_id })
            },
            crate::input::actions::Action::FocusTerminalPaneWithId {
                pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => ActionType::FocusTerminalPaneWithId(FocusTerminalPaneWithIdAction {
                pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            }),
            crate::input::actions::Action::FocusPluginPaneWithId {
                pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => ActionType::FocusPluginPaneWithId(FocusPluginPaneWithIdAction {
                pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            }),
            crate::input::actions::Action::RenameTerminalPane { pane_id, name } => {
                ActionType::RenameTerminalPane(RenameTerminalPaneAction {
                    pane_id,
                    name: name.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::RenamePluginPane { pane_id, name } => {
                ActionType::RenamePluginPane(RenamePluginPaneAction {
                    pane_id,
                    name: name.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::RenameTab { tab_index, name } => {
                ActionType::RenameTab(RenameTabAction {
                    tab_index,
                    name: name.into_iter().map(|b| b as u32).collect(),
                })
            },
            crate::input::actions::Action::BreakPane => ActionType::BreakPane(BreakPaneAction {}),
            crate::input::actions::Action::BreakPaneRight => {
                ActionType::BreakPaneRight(BreakPaneRightAction {})
            },
            crate::input::actions::Action::BreakPaneLeft => {
                ActionType::BreakPaneLeft(BreakPaneLeftAction {})
            },
            crate::input::actions::Action::RenameSession { name } => {
                ActionType::RenameSession(RenameSessionAction { name })
            },
            crate::input::actions::Action::CliPipe {
                pipe_id,
                name,
                payload,
                args,
                plugin,
                configuration,
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd,
                pane_title,
            } => ActionType::CliPipe(CliPipeAction {
                pipe_id,
                name,
                payload,
                args: args
                    .map(|a| a.into_iter().collect::<HashMap<_, _>>())
                    .unwrap_or_default(),
                plugin,
                configuration: configuration
                    .map(|c| c.into_iter().collect::<HashMap<_, _>>())
                    .unwrap_or_default(),
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
                pane_title,
            }),
            crate::input::actions::Action::KeybindPipe {
                name,
                payload,
                args,
                plugin,
                plugin_id,
                configuration,
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd,
                pane_title,
            } => ActionType::KeybindPipe(KeybindPipeAction {
                name,
                payload,
                args: args
                    .map(|a| a.into_iter().collect::<HashMap<_, _>>())
                    .unwrap_or_default(),
                plugin,
                plugin_id,
                configuration: configuration
                    .map(|c| c.into_iter().collect::<HashMap<_, _>>())
                    .unwrap_or_default(),
                launch_new,
                skip_cache,
                floating,
                in_place,
                cwd: cwd.map(|p| p.to_string_lossy().to_string()),
                pane_title,
            }),
            crate::input::actions::Action::ListClients => {
                ActionType::ListClients(ListClientsAction {})
            },
            crate::input::actions::Action::TogglePanePinned => {
                ActionType::TogglePanePinned(TogglePanePinnedAction {})
            },
            crate::input::actions::Action::StackPanes { pane_ids } => {
                ActionType::StackPanes(StackPanesAction {
                    pane_ids: pane_ids.into_iter().map(|id| id.into()).collect(),
                })
            },
            crate::input::actions::Action::ChangeFloatingPaneCoordinates {
                pane_id,
                coordinates,
            } => ActionType::ChangeFloatingPaneCoordinates(ChangeFloatingPaneCoordinatesAction {
                pane_id: Some(pane_id.into()),
                coordinates: Some(coordinates.into()),
            }),
            crate::input::actions::Action::TogglePaneInGroup => {
                ActionType::TogglePaneInGroup(TogglePaneInGroupAction {})
            },
            crate::input::actions::Action::ToggleGroupMarking => {
                ActionType::ToggleGroupMarking(ToggleGroupMarkingAction {})
            },
        };

        Self {
            action_type: Some(action_type),
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::Action>
    for crate::input::actions::Action
{
    type Error = anyhow::Error;
    fn try_from(
        action: crate::client_server_contract::client_server_contract::Action,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::action::ActionType;

        let action_type = action
            .action_type
            .ok_or_else(|| anyhow!("Action missing action_type"))?;

        match action_type {
            ActionType::Quit(_) => Ok(crate::input::actions::Action::Quit),
            ActionType::Write(write_action) => Ok(crate::input::actions::Action::Write {
                key_with_modifier: write_action
                    .key_with_modifier
                    .map(|k| k.try_into())
                    .transpose()?,
                bytes: write_action.bytes.into_iter().map(|b| b as u8).collect(),
                is_kitty_keyboard_protocol: write_action.is_kitty_keyboard_protocol,
            }),
            ActionType::WriteChars(write_chars_action) => {
                Ok(crate::input::actions::Action::WriteChars {
                    chars: write_chars_action.chars,
                })
            },
            ActionType::SwitchToMode(switch_mode_action) => {
                Ok(crate::input::actions::Action::SwitchToMode {
                    input_mode: proto_i32_to_input_mode(switch_mode_action.input_mode)?,
                })
            },
            ActionType::SwitchModeForAllClients(switch_mode_action) => {
                Ok(crate::input::actions::Action::SwitchModeForAllClients {
                    input_mode: proto_i32_to_input_mode(switch_mode_action.input_mode)?,
                })
            },
            ActionType::Resize(resize_action) => Ok(crate::input::actions::Action::Resize {
                resize: proto_i32_to_resize(resize_action.resize)?,
                direction: resize_action
                    .direction
                    .map(|d| proto_i32_to_direction(d))
                    .transpose()?,
            }),
            ActionType::FocusNextPane(_) => Ok(crate::input::actions::Action::FocusNextPane),
            ActionType::FocusPreviousPane(_) => {
                Ok(crate::input::actions::Action::FocusPreviousPane)
            },
            ActionType::SwitchFocus(_) => Ok(crate::input::actions::Action::SwitchFocus),
            ActionType::MoveFocus(move_focus_action) => {
                Ok(crate::input::actions::Action::MoveFocus {
                    direction: proto_i32_to_direction(move_focus_action.direction)?,
                })
            },
            ActionType::MoveFocusOrTab(move_focus_action) => {
                Ok(crate::input::actions::Action::MoveFocusOrTab {
                    direction: proto_i32_to_direction(move_focus_action.direction)?,
                })
            },
            ActionType::MovePane(move_pane_action) => Ok(crate::input::actions::Action::MovePane {
                direction: move_pane_action
                    .direction
                    .map(|d| proto_i32_to_direction(d))
                    .transpose()?,
            }),
            ActionType::MovePaneBackwards(_) => {
                Ok(crate::input::actions::Action::MovePaneBackwards)
            },
            ActionType::ClearScreen(_) => Ok(crate::input::actions::Action::ClearScreen),
            ActionType::DumpScreen(dump_screen_action) => {
                Ok(crate::input::actions::Action::DumpScreen {
                    file_path: dump_screen_action.file_path,
                    include_scrollback: dump_screen_action.include_scrollback,
                })
            },
            ActionType::DumpLayout(_) => Ok(crate::input::actions::Action::DumpLayout),
            ActionType::EditScrollback(_) => Ok(crate::input::actions::Action::EditScrollback),
            ActionType::ScrollUp(_) => Ok(crate::input::actions::Action::ScrollUp),
            ActionType::ScrollUpAt(scroll_action) => {
                Ok(crate::input::actions::Action::ScrollUpAt {
                    position: scroll_action
                        .position
                        .ok_or_else(|| anyhow!("ScrollUpAt missing position"))?
                        .try_into()?,
                })
            },
            ActionType::ScrollDown(_) => Ok(crate::input::actions::Action::ScrollDown),
            ActionType::ScrollDownAt(scroll_action) => {
                Ok(crate::input::actions::Action::ScrollDownAt {
                    position: scroll_action
                        .position
                        .ok_or_else(|| anyhow!("ScrollDownAt missing position"))?
                        .try_into()?,
                })
            },
            ActionType::ScrollToBottom(_) => Ok(crate::input::actions::Action::ScrollToBottom),
            ActionType::ScrollToTop(_) => Ok(crate::input::actions::Action::ScrollToTop),
            ActionType::PageScrollUp(_) => Ok(crate::input::actions::Action::PageScrollUp),
            ActionType::PageScrollDown(_) => Ok(crate::input::actions::Action::PageScrollDown),
            ActionType::HalfPageScrollUp(_) => Ok(crate::input::actions::Action::HalfPageScrollUp),
            ActionType::HalfPageScrollDown(_) => {
                Ok(crate::input::actions::Action::HalfPageScrollDown)
            },
            ActionType::ToggleFocusFullscreen(_) => {
                Ok(crate::input::actions::Action::ToggleFocusFullscreen)
            },
            ActionType::TogglePaneFrames(_) => Ok(crate::input::actions::Action::TogglePaneFrames),
            ActionType::ToggleActiveSyncTab(_) => {
                Ok(crate::input::actions::Action::ToggleActiveSyncTab)
            },
            ActionType::NewPane(new_pane_action) => Ok(crate::input::actions::Action::NewPane {
                direction: new_pane_action
                    .direction
                    .map(|d| proto_i32_to_direction(d))
                    .transpose()?,
                pane_name: new_pane_action.pane_name,
                start_suppressed: new_pane_action.start_suppressed,
            }),
            ActionType::EditFile(edit_file_action) => Ok(crate::input::actions::Action::EditFile {
                payload: edit_file_action
                    .payload
                    .ok_or_else(|| anyhow!("EditFile missing payload"))?
                    .try_into()?,
                direction: edit_file_action
                    .direction
                    .map(|d| proto_i32_to_direction(d))
                    .transpose()?,
                floating: edit_file_action.floating,
                in_place: edit_file_action.in_place,
                start_suppressed: edit_file_action.start_suppressed,
                coordinates: edit_file_action
                    .coordinates
                    .map(|c| c.try_into())
                    .transpose()?,
                near_current_pane: edit_file_action.near_current_pane,
            }),
            ActionType::NewFloatingPane(new_floating_action) => {
                Ok(crate::input::actions::Action::NewFloatingPane {
                    command: new_floating_action
                        .command
                        .map(|c| c.try_into())
                        .transpose()?,
                    pane_name: new_floating_action.pane_name,
                    coordinates: new_floating_action
                        .coordinates
                        .map(|c| c.try_into())
                        .transpose()?,
                    near_current_pane: new_floating_action.near_current_pane,
                })
            },
            ActionType::NewTiledPane(new_tiled_action) => {
                Ok(crate::input::actions::Action::NewTiledPane {
                    direction: new_tiled_action
                        .direction
                        .map(|d| proto_i32_to_direction(d))
                        .transpose()?,
                    command: new_tiled_action.command.map(|c| c.try_into()).transpose()?,
                    pane_name: new_tiled_action.pane_name,
                    near_current_pane: new_tiled_action.near_current_pane,
                })
            },
            ActionType::NewInPlacePane(new_in_place_action) => {
                Ok(crate::input::actions::Action::NewInPlacePane {
                    command: new_in_place_action
                        .command
                        .map(|c| c.try_into())
                        .transpose()?,
                    pane_name: new_in_place_action.pane_name,
                    near_current_pane: new_in_place_action.near_current_pane,
                    pane_id_to_replace: new_in_place_action
                        .pane_id_to_replace
                        .and_then(|p| p.try_into().ok()),
                    close_replace_pane: new_in_place_action.close_replace_pane,
                })
            },
            ActionType::NewStackedPane(new_stacked_action) => {
                Ok(crate::input::actions::Action::NewStackedPane {
                    command: new_stacked_action
                        .command
                        .map(|c| c.try_into())
                        .transpose()?,
                    pane_name: new_stacked_action.pane_name,
                    near_current_pane: new_stacked_action.near_current_pane,
                })
            },
            ActionType::NewBlockingPane(new_blocking_action) => {
                Ok(crate::input::actions::Action::NewBlockingPane {
                    placement: new_blocking_action
                        .placement
                        .ok_or_else(|| anyhow!("NewBlockingPane missing placement"))?
                        .try_into()?,
                    pane_name: new_blocking_action.pane_name,
                    command: new_blocking_action
                        .command
                        .map(|c| c.try_into())
                        .transpose()?,
                    unblock_condition: new_blocking_action
                        .unblock_condition
                        .map(|c| proto_i32_to_unblock_condition(c))
                        .transpose()?,
                    near_current_pane: new_blocking_action.near_current_pane,
                })
            },
            ActionType::TogglePaneEmbedOrFloating(_) => {
                Ok(crate::input::actions::Action::TogglePaneEmbedOrFloating)
            },
            ActionType::ToggleFloatingPanes(_) => {
                Ok(crate::input::actions::Action::ToggleFloatingPanes)
            },
            ActionType::CloseFocus(_) => Ok(crate::input::actions::Action::CloseFocus),
            ActionType::PaneNameInput(pane_name_action) => {
                Ok(crate::input::actions::Action::PaneNameInput {
                    input: pane_name_action
                        .input
                        .into_iter()
                        .map(|b| b as u8)
                        .collect(),
                })
            },
            ActionType::UndoRenamePane(_) => Ok(crate::input::actions::Action::UndoRenamePane),
            ActionType::NewTab(new_tab_action) => Ok(crate::input::actions::Action::NewTab {
                tiled_layout: new_tab_action
                    .tiled_layout
                    .map(|l| l.try_into())
                    .transpose()?,
                floating_layouts: new_tab_action
                    .floating_layouts
                    .into_iter()
                    .map(|l| l.try_into())
                    .collect::<Result<Vec<_>>>()?,
                swap_tiled_layouts: if new_tab_action.swap_tiled_layouts.is_empty() {
                    None
                } else {
                    Some(
                        new_tab_action
                            .swap_tiled_layouts
                            .into_iter()
                            .map(|l| l.try_into())
                            .collect::<Result<Vec<_>>>()?,
                    )
                },
                swap_floating_layouts: if new_tab_action.swap_floating_layouts.is_empty() {
                    None
                } else {
                    Some(
                        new_tab_action
                            .swap_floating_layouts
                            .into_iter()
                            .map(|l| l.try_into())
                            .collect::<Result<Vec<_>>>()?,
                    )
                },
                tab_name: new_tab_action.tab_name,
                should_change_focus_to_new_tab: new_tab_action.should_change_focus_to_new_tab,
                cwd: new_tab_action.cwd.map(PathBuf::from),
                initial_panes: if new_tab_action.initial_panes.is_empty() {
                    None
                } else {
                    Some(
                        new_tab_action
                            .initial_panes
                            .into_iter()
                            .map(|p| p.try_into())
                            .collect::<Result<Vec<_>>>()?,
                    )
                },

                first_pane_unblock_condition: new_tab_action
                    .first_pane_unblock_condition
                    .map(|c| proto_i32_to_unblock_condition(c))
                    .transpose()?,
            }),
            ActionType::NoOp(_) => Ok(crate::input::actions::Action::NoOp),
            ActionType::GoToNextTab(_) => Ok(crate::input::actions::Action::GoToNextTab),
            ActionType::GoToPreviousTab(_) => Ok(crate::input::actions::Action::GoToPreviousTab),
            ActionType::CloseTab(_) => Ok(crate::input::actions::Action::CloseTab),
            ActionType::GoToTab(go_to_tab_action) => Ok(crate::input::actions::Action::GoToTab {
                index: go_to_tab_action.index,
            }),
            ActionType::GoToTabName(go_to_tab_name_action) => {
                Ok(crate::input::actions::Action::GoToTabName {
                    name: go_to_tab_name_action.name,
                    create: go_to_tab_name_action.create,
                })
            },
            ActionType::ToggleTab(_) => Ok(crate::input::actions::Action::ToggleTab),
            ActionType::TabNameInput(tab_name_action) => {
                Ok(crate::input::actions::Action::TabNameInput {
                    input: tab_name_action.input.into_iter().map(|b| b as u8).collect(),
                })
            },
            ActionType::UndoRenameTab(_) => Ok(crate::input::actions::Action::UndoRenameTab),
            ActionType::MoveTab(move_tab_action) => Ok(crate::input::actions::Action::MoveTab {
                direction: proto_i32_to_direction(move_tab_action.direction)?,
            }),
            ActionType::Run(run_action) => Ok(crate::input::actions::Action::Run {
                command: run_action
                    .command
                    .ok_or_else(|| anyhow!("Run missing command"))?
                    .try_into()?,
                near_current_pane: run_action.near_current_pane,
            }),
            ActionType::Detach(_) => Ok(crate::input::actions::Action::Detach),
            ActionType::SwitchSession(switch_session_action) => {
                Ok(crate::input::actions::Action::SwitchSession {
                    name: switch_session_action.name.clone(),
                    tab_position: switch_session_action.tab_position.map(|p| p as usize),
                    pane_id: switch_session_action
                        .pane_id
                        .as_ref()
                        .map(|p| (p.pane_id, p.is_plugin)),
                    layout: switch_session_action
                        .layout
                        .map(|l| l.try_into())
                        .transpose()?,
                    cwd: switch_session_action.cwd.map(PathBuf::from),
                })
            },
            ActionType::LaunchOrFocusPlugin(launch_plugin_action) => {
                Ok(crate::input::actions::Action::LaunchOrFocusPlugin {
                    plugin: launch_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("LaunchOrFocusPlugin missing plugin"))?
                        .try_into()?,
                    should_float: launch_plugin_action.should_float,
                    move_to_focused_tab: launch_plugin_action.move_to_focused_tab,
                    should_open_in_place: launch_plugin_action.should_open_in_place,
                    skip_cache: launch_plugin_action.skip_cache,
                })
            },
            ActionType::LaunchPlugin(launch_plugin_action) => {
                Ok(crate::input::actions::Action::LaunchPlugin {
                    plugin: launch_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("LaunchPlugin missing plugin"))?
                        .try_into()?,
                    should_float: launch_plugin_action.should_float,
                    should_open_in_place: launch_plugin_action.should_open_in_place,
                    skip_cache: launch_plugin_action.skip_cache,
                    cwd: launch_plugin_action.cwd.map(PathBuf::from),
                })
            },
            ActionType::MouseEvent(mouse_event_action) => {
                Ok(crate::input::actions::Action::MouseEvent {
                    event: mouse_event_action
                        .event
                        .ok_or_else(|| anyhow!("MouseEvent missing event"))?
                        .try_into()?,
                })
            },
            ActionType::Copy(_) => Ok(crate::input::actions::Action::Copy),
            ActionType::Confirm(_) => Ok(crate::input::actions::Action::Confirm),
            ActionType::Deny(_) => Ok(crate::input::actions::Action::Deny),
            ActionType::SkipConfirm(skip_confirm_action) => {
                Ok(crate::input::actions::Action::SkipConfirm {
                    action: Box::new(
                        skip_confirm_action
                            .action
                            .ok_or_else(|| anyhow!("SkipConfirm missing action"))?
                            .as_ref()
                            .clone()
                            .try_into()?,
                    ),
                })
            },
            ActionType::SearchInput(search_input_action) => {
                Ok(crate::input::actions::Action::SearchInput {
                    input: search_input_action
                        .input
                        .into_iter()
                        .map(|b| b as u8)
                        .collect(),
                })
            },
            ActionType::Search(search_action) => Ok(crate::input::actions::Action::Search {
                direction: proto_i32_to_search_direction(search_action.direction)?,
            }),
            ActionType::SearchToggleOption(search_toggle_action) => {
                Ok(crate::input::actions::Action::SearchToggleOption {
                    option: proto_i32_to_search_option(search_toggle_action.option)?,
                })
            },
            ActionType::ToggleMouseMode(_) => Ok(crate::input::actions::Action::ToggleMouseMode),
            ActionType::PreviousSwapLayout(_) => {
                Ok(crate::input::actions::Action::PreviousSwapLayout)
            },
            ActionType::NextSwapLayout(_) => Ok(crate::input::actions::Action::NextSwapLayout),
            ActionType::OverrideLayout(override_layout_action) => {
                Ok(crate::input::actions::Action::OverrideLayout {
                    tiled_layout: override_layout_action
                        .tiled_layout
                        .map(|l| l.try_into())
                        .transpose()?,
                    floating_layouts: override_layout_action
                        .floating_layouts
                        .into_iter()
                        .map(|l| l.try_into())
                        .collect::<Result<Vec<_>>>()?,
                    swap_tiled_layouts: if override_layout_action.swap_tiled_layouts.is_empty() {
                        None
                    } else {
                        Some(
                            override_layout_action
                                .swap_tiled_layouts
                                .into_iter()
                                .map(|l| l.try_into())
                                .collect::<Result<Vec<_>>>()?,
                        )
                    },
                    swap_floating_layouts: if override_layout_action
                        .swap_floating_layouts
                        .is_empty()
                    {
                        None
                    } else {
                        Some(
                            override_layout_action
                                .swap_floating_layouts
                                .into_iter()
                                .map(|l| l.try_into())
                                .collect::<Result<Vec<_>>>()?,
                        )
                    },
                    tab_name: override_layout_action.tab_name,
                    retain_existing_terminal_panes: override_layout_action
                        .retain_existing_terminal_panes,
                    retain_existing_plugin_panes: override_layout_action
                        .retain_existing_plugin_panes,
                })
            },
            ActionType::QueryTabNames(_) => Ok(crate::input::actions::Action::QueryTabNames),
            ActionType::NewTiledPluginPane(new_tiled_plugin_action) => {
                Ok(crate::input::actions::Action::NewTiledPluginPane {
                    plugin: new_tiled_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("NewTiledPluginPane missing plugin"))?
                        .try_into()?,
                    pane_name: new_tiled_plugin_action.pane_name,
                    skip_cache: new_tiled_plugin_action.skip_cache,
                    cwd: new_tiled_plugin_action.cwd.map(PathBuf::from),
                })
            },
            ActionType::NewFloatingPluginPane(new_floating_plugin_action) => {
                Ok(crate::input::actions::Action::NewFloatingPluginPane {
                    plugin: new_floating_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("NewFloatingPluginPane missing plugin"))?
                        .try_into()?,
                    pane_name: new_floating_plugin_action.pane_name,
                    skip_cache: new_floating_plugin_action.skip_cache,
                    cwd: new_floating_plugin_action.cwd.map(PathBuf::from),
                    coordinates: new_floating_plugin_action
                        .coordinates
                        .map(|c| c.try_into())
                        .transpose()?,
                })
            },
            ActionType::NewInPlacePluginPane(new_in_place_plugin_action) => {
                Ok(crate::input::actions::Action::NewInPlacePluginPane {
                    plugin: new_in_place_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("NewInPlacePluginPane missing plugin"))?
                        .try_into()?,
                    pane_name: new_in_place_plugin_action.pane_name,
                    skip_cache: new_in_place_plugin_action.skip_cache,
                })
            },
            ActionType::StartOrReloadPlugin(start_plugin_action) => {
                Ok(crate::input::actions::Action::StartOrReloadPlugin {
                    plugin: start_plugin_action
                        .plugin
                        .ok_or_else(|| anyhow!("StartOrReloadPlugin missing plugin"))?
                        .try_into()?,
                })
            },
            ActionType::CloseTerminalPane(close_pane_action) => {
                Ok(crate::input::actions::Action::CloseTerminalPane {
                    pane_id: close_pane_action.pane_id,
                })
            },
            ActionType::ClosePluginPane(close_pane_action) => {
                Ok(crate::input::actions::Action::ClosePluginPane {
                    pane_id: close_pane_action.pane_id,
                })
            },
            ActionType::FocusTerminalPaneWithId(focus_pane_action) => {
                Ok(crate::input::actions::Action::FocusTerminalPaneWithId {
                    pane_id: focus_pane_action.pane_id,
                    should_float_if_hidden: focus_pane_action.should_float_if_hidden,
                    should_be_in_place_if_hidden: focus_pane_action.should_be_in_place_if_hidden,
                })
            },
            ActionType::FocusPluginPaneWithId(focus_pane_action) => {
                Ok(crate::input::actions::Action::FocusPluginPaneWithId {
                    pane_id: focus_pane_action.pane_id,
                    should_float_if_hidden: focus_pane_action.should_float_if_hidden,
                    should_be_in_place_if_hidden: focus_pane_action.should_be_in_place_if_hidden,
                })
            },
            ActionType::RenameTerminalPane(rename_pane_action) => {
                Ok(crate::input::actions::Action::RenameTerminalPane {
                    pane_id: rename_pane_action.pane_id,
                    name: rename_pane_action
                        .name
                        .into_iter()
                        .map(|b| b as u8)
                        .collect(),
                })
            },
            ActionType::RenamePluginPane(rename_pane_action) => {
                Ok(crate::input::actions::Action::RenamePluginPane {
                    pane_id: rename_pane_action.pane_id,
                    name: rename_pane_action
                        .name
                        .into_iter()
                        .map(|b| b as u8)
                        .collect(),
                })
            },
            ActionType::RenameTab(rename_tab_action) => {
                Ok(crate::input::actions::Action::RenameTab {
                    tab_index: rename_tab_action.tab_index,
                    name: rename_tab_action
                        .name
                        .into_iter()
                        .map(|b| b as u8)
                        .collect(),
                })
            },
            ActionType::BreakPane(_) => Ok(crate::input::actions::Action::BreakPane),
            ActionType::BreakPaneRight(_) => Ok(crate::input::actions::Action::BreakPaneRight),
            ActionType::BreakPaneLeft(_) => Ok(crate::input::actions::Action::BreakPaneLeft),
            ActionType::RenameSession(rename_session_action) => {
                Ok(crate::input::actions::Action::RenameSession {
                    name: rename_session_action.name,
                })
            },
            ActionType::CliPipe(cli_pipe_action) => Ok(crate::input::actions::Action::CliPipe {
                pipe_id: cli_pipe_action.pipe_id,
                name: cli_pipe_action.name,
                payload: cli_pipe_action.payload,
                args: if cli_pipe_action.args.is_empty() {
                    None
                } else {
                    Some(cli_pipe_action.args.into_iter().collect())
                },
                plugin: cli_pipe_action.plugin,
                configuration: if cli_pipe_action.configuration.is_empty() {
                    None
                } else {
                    Some(cli_pipe_action.configuration.into_iter().collect())
                },
                launch_new: cli_pipe_action.launch_new,
                skip_cache: cli_pipe_action.skip_cache,
                floating: cli_pipe_action.floating,
                in_place: cli_pipe_action.in_place,
                cwd: cli_pipe_action.cwd.map(PathBuf::from),
                pane_title: cli_pipe_action.pane_title,
            }),
            ActionType::KeybindPipe(keybind_pipe_action) => {
                Ok(crate::input::actions::Action::KeybindPipe {
                    name: keybind_pipe_action.name,
                    payload: keybind_pipe_action.payload,
                    args: if keybind_pipe_action.args.is_empty() {
                        None
                    } else {
                        Some(keybind_pipe_action.args.into_iter().collect())
                    },
                    plugin: keybind_pipe_action.plugin,
                    plugin_id: keybind_pipe_action.plugin_id,
                    configuration: if keybind_pipe_action.configuration.is_empty() {
                        None
                    } else {
                        Some(keybind_pipe_action.configuration.into_iter().collect())
                    },
                    launch_new: keybind_pipe_action.launch_new,
                    skip_cache: keybind_pipe_action.skip_cache,
                    floating: keybind_pipe_action.floating,
                    in_place: keybind_pipe_action.in_place,
                    cwd: keybind_pipe_action.cwd.map(PathBuf::from),
                    pane_title: keybind_pipe_action.pane_title,
                })
            },
            ActionType::ListClients(_) => Ok(crate::input::actions::Action::ListClients),
            ActionType::TogglePanePinned(_) => Ok(crate::input::actions::Action::TogglePanePinned),
            ActionType::StackPanes(stack_panes_action) => {
                Ok(crate::input::actions::Action::StackPanes {
                    pane_ids: stack_panes_action
                        .pane_ids
                        .into_iter()
                        .map(|id| id.try_into())
                        .collect::<Result<Vec<_>>>()?,
                })
            },
            ActionType::ChangeFloatingPaneCoordinates(change_coords_action) => Ok(
                crate::input::actions::Action::ChangeFloatingPaneCoordinates {
                    pane_id: change_coords_action
                        .pane_id
                        .ok_or_else(|| anyhow!("ChangeFloatingPaneCoordinates missing pane_id"))?
                        .try_into()?,
                    coordinates: change_coords_action
                        .coordinates
                        .ok_or_else(|| {
                            anyhow!("ChangeFloatingPaneCoordinates missing coordinates")
                        })?
                        .try_into()?,
                },
            ),
            ActionType::TogglePaneInGroup(_) => {
                Ok(crate::input::actions::Action::TogglePaneInGroup)
            },
            ActionType::ToggleGroupMarking(_) => {
                Ok(crate::input::actions::Action::ToggleGroupMarking)
            },
        }
    }
}

impl From<crate::data::KeyWithModifier>
    for crate::client_server_contract::client_server_contract::KeyWithModifier
{
    fn from(key: crate::data::KeyWithModifier) -> Self {
        use crate::ipc::enum_conversions::{bare_key_to_proto_i32, key_modifier_to_proto_i32};

        // Handle character keys specially - store the character for Char variant
        let (bare_key_enum, char_data) = match &key.bare_key {
            crate::data::BareKey::Char(c) => (
                crate::client_server_contract::client_server_contract::BareKey::Char as i32,
                Some(c.to_string()),
            ),
            other => (bare_key_to_proto_i32(*other), None),
        };

        Self {
            bare_key: bare_key_enum,
            key_modifiers: key
                .key_modifiers
                .into_iter()
                .map(|modifier| key_modifier_to_proto_i32(modifier))
                .collect(),
            character: char_data,
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::KeyWithModifier>
    for crate::data::KeyWithModifier
{
    type Error = anyhow::Error;
    fn try_from(
        key: crate::client_server_contract::client_server_contract::KeyWithModifier,
    ) -> Result<Self> {
        use crate::ipc::enum_conversions::{bare_key_from_proto_i32, key_modifier_from_proto_i32};
        use std::collections::BTreeSet;

        // Handle character keys specially
        let bare_key = if key.bare_key
            == crate::client_server_contract::client_server_contract::BareKey::Char as i32
        {
            let character_str = key
                .character
                .ok_or_else(|| anyhow!("Character key missing character data"))?;
            let character = character_str
                .chars()
                .next()
                .ok_or_else(|| anyhow!("Empty character string"))?;
            crate::data::BareKey::Char(character)
        } else {
            bare_key_from_proto_i32(key.bare_key)?
        };

        let key_modifiers: Result<BTreeSet<_>> = key
            .key_modifiers
            .into_iter()
            .map(|modifier| key_modifier_from_proto_i32(modifier))
            .collect();

        Ok(Self {
            bare_key,
            key_modifiers: key_modifiers?,
        })
    }
}

impl From<crate::data::ConnectToSession>
    for crate::client_server_contract::client_server_contract::ConnectToSession
{
    fn from(connect: crate::data::ConnectToSession) -> Self {
        Self {
            name: connect.name,
            tab_position: connect.tab_position.map(|p| p as u32),
            pane_id: connect.pane_id.map(|(id, is_plugin)| {
                crate::client_server_contract::client_server_contract::PaneIdWithPlugin {
                    pane_id: id,
                    is_plugin,
                }
            }),
            layout: connect.layout.map(|l| l.into()),
            cwd: connect.cwd.map(|p| p.to_string_lossy().to_string()),
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::ConnectToSession>
    for crate::data::ConnectToSession
{
    type Error = anyhow::Error;
    fn try_from(
        connect: crate::client_server_contract::client_server_contract::ConnectToSession,
    ) -> Result<Self> {
        Ok(Self {
            name: connect.name,
            tab_position: connect.tab_position.map(|p| p as usize),
            pane_id: connect.pane_id.map(|p| (p.pane_id, p.is_plugin)),
            layout: connect.layout.map(|l| l.try_into()).transpose()?,
            cwd: connect.cwd.map(PathBuf::from),
        })
    }
}

impl From<crate::data::LayoutInfo>
    for crate::client_server_contract::client_server_contract::LayoutInfo
{
    fn from(layout: crate::data::LayoutInfo) -> Self {
        use crate::client_server_contract::client_server_contract::layout_info::LayoutType;
        let layout_type = match layout {
            crate::data::LayoutInfo::BuiltIn(name) => LayoutType::BuiltinName(name),
            crate::data::LayoutInfo::File(path) => LayoutType::FilePath(path),
            crate::data::LayoutInfo::Url(url) => LayoutType::Url(url),
            crate::data::LayoutInfo::Stringified(content) => LayoutType::Stringified(content),
        };
        Self {
            layout_type: Some(layout_type),
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::LayoutInfo>
    for crate::data::LayoutInfo
{
    type Error = anyhow::Error;
    fn try_from(
        layout: crate::client_server_contract::client_server_contract::LayoutInfo,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::layout_info::LayoutType;
        match layout.layout_type {
            Some(LayoutType::BuiltinName(name)) => Ok(crate::data::LayoutInfo::BuiltIn(name)),
            Some(LayoutType::FilePath(path)) => Ok(crate::data::LayoutInfo::File(path)),
            Some(LayoutType::Url(url)) => Ok(crate::data::LayoutInfo::Url(url)),
            Some(LayoutType::Stringified(content)) => {
                Ok(crate::data::LayoutInfo::Stringified(content))
            },
            None => Err(anyhow!("LayoutInfo missing layout_type")),
        }
    }
}

impl From<ExitReason> for ProtoExitReason {
    fn from(reason: ExitReason) -> Self {
        match reason {
            ExitReason::Normal => ProtoExitReason::Normal,
            ExitReason::NormalDetached => ProtoExitReason::NormalDetached,
            ExitReason::ForceDetached => ProtoExitReason::ForceDetached,
            ExitReason::CannotAttach => ProtoExitReason::CannotAttach,
            ExitReason::Disconnect => ProtoExitReason::Disconnect,
            ExitReason::WebClientsForbidden => ProtoExitReason::WebClientsForbidden,
            ExitReason::Error(_msg) => ProtoExitReason::Error,
            ExitReason::CustomExitStatus(_status) => ProtoExitReason::CustomExitStatus,
        }
    }
}

impl TryFrom<ProtoExitReason> for ExitReason {
    type Error = anyhow::Error;
    fn try_from(reason: ProtoExitReason) -> Result<Self> {
        match reason {
            ProtoExitReason::Normal => Ok(ExitReason::Normal),
            ProtoExitReason::NormalDetached => Ok(ExitReason::NormalDetached),
            ProtoExitReason::ForceDetached => Ok(ExitReason::ForceDetached),
            ProtoExitReason::CannotAttach => Ok(ExitReason::CannotAttach),
            ProtoExitReason::Disconnect => Ok(ExitReason::Disconnect),
            ProtoExitReason::WebClientsForbidden => Ok(ExitReason::WebClientsForbidden),
            ProtoExitReason::Error => Ok(ExitReason::Error("Protobuf error".to_string())),
            ProtoExitReason::CustomExitStatus => Ok(ExitReason::CustomExitStatus(0)),
            ProtoExitReason::Unspecified => Err(anyhow!("Unspecified exit reason")),
        }
    }
}

// InputMode conversion helper functions
fn input_mode_to_proto_i32(mode: InputMode) -> i32 {
    match mode {
        InputMode::Normal => ProtoInputMode::Normal as i32,
        InputMode::Locked => ProtoInputMode::Locked as i32,
        InputMode::Resize => ProtoInputMode::Resize as i32,
        InputMode::Pane => ProtoInputMode::Pane as i32,
        InputMode::Tab => ProtoInputMode::Tab as i32,
        InputMode::Scroll => ProtoInputMode::Scroll as i32,
        InputMode::EnterSearch => ProtoInputMode::EnterSearch as i32,
        InputMode::Search => ProtoInputMode::Search as i32,
        InputMode::RenameTab => ProtoInputMode::RenameTab as i32,
        InputMode::RenamePane => ProtoInputMode::RenamePane as i32,
        InputMode::Session => ProtoInputMode::Session as i32,
        InputMode::Move => ProtoInputMode::Move as i32,
        InputMode::Prompt => ProtoInputMode::Prompt as i32,
        InputMode::Tmux => ProtoInputMode::Tmux as i32,
    }
}

fn proto_i32_to_input_mode(i: i32) -> Result<InputMode> {
    match ProtoInputMode::from_i32(i) {
        Some(ProtoInputMode::Normal) => Ok(InputMode::Normal),
        Some(ProtoInputMode::Locked) => Ok(InputMode::Locked),
        Some(ProtoInputMode::Resize) => Ok(InputMode::Resize),
        Some(ProtoInputMode::Pane) => Ok(InputMode::Pane),
        Some(ProtoInputMode::Tab) => Ok(InputMode::Tab),
        Some(ProtoInputMode::Scroll) => Ok(InputMode::Scroll),
        Some(ProtoInputMode::EnterSearch) => Ok(InputMode::EnterSearch),
        Some(ProtoInputMode::Search) => Ok(InputMode::Search),
        Some(ProtoInputMode::RenameTab) => Ok(InputMode::RenameTab),
        Some(ProtoInputMode::RenamePane) => Ok(InputMode::RenamePane),
        Some(ProtoInputMode::Session) => Ok(InputMode::Session),
        Some(ProtoInputMode::Move) => Ok(InputMode::Move),
        Some(ProtoInputMode::Prompt) => Ok(InputMode::Prompt),
        Some(ProtoInputMode::Tmux) => Ok(InputMode::Tmux),
        _ => Err(anyhow!("Invalid InputMode value: {}", i)),
    }
}

// Additional helper functions for Action conversion
fn resize_to_proto_i32(resize: crate::data::Resize) -> i32 {
    use crate::client_server_contract::client_server_contract::ResizeType;
    match resize {
        crate::data::Resize::Increase => ResizeType::Increase as i32,
        crate::data::Resize::Decrease => ResizeType::Decrease as i32,
    }
}

fn direction_to_proto_i32(direction: crate::data::Direction) -> i32 {
    use crate::client_server_contract::client_server_contract::Direction as ProtoDirection;
    match direction {
        crate::data::Direction::Left => ProtoDirection::Left as i32,
        crate::data::Direction::Right => ProtoDirection::Right as i32,
        crate::data::Direction::Up => ProtoDirection::Up as i32,
        crate::data::Direction::Down => ProtoDirection::Down as i32,
    }
}

fn search_direction_to_proto_i32(direction: crate::input::actions::SearchDirection) -> i32 {
    use crate::client_server_contract::client_server_contract::SearchDirection as ProtoSearchDirection;
    match direction {
        crate::input::actions::SearchDirection::Up => ProtoSearchDirection::Up as i32,
        crate::input::actions::SearchDirection::Down => ProtoSearchDirection::Down as i32,
    }
}

fn search_option_to_proto_i32(option: crate::input::actions::SearchOption) -> i32 {
    use crate::client_server_contract::client_server_contract::SearchOption as ProtoSearchOption;
    match option {
        crate::input::actions::SearchOption::CaseSensitivity => {
            ProtoSearchOption::CaseSensitivity as i32
        },
        crate::input::actions::SearchOption::Wrap => ProtoSearchOption::Wrap as i32,
        crate::input::actions::SearchOption::WholeWord => ProtoSearchOption::WholeWord as i32,
    }
}

fn unblock_condition_to_proto_i32(condition: crate::data::UnblockCondition) -> i32 {
    use crate::client_server_contract::client_server_contract::UnblockCondition as ProtoUnblockCondition;
    match condition {
        crate::data::UnblockCondition::OnExitSuccess => ProtoUnblockCondition::OnExitSuccess as i32,
        crate::data::UnblockCondition::OnExitFailure => ProtoUnblockCondition::OnExitFailure as i32,
        crate::data::UnblockCondition::OnAnyExit => ProtoUnblockCondition::OnAnyExit as i32,
    }
}

// Reverse helper functions for Action conversion

fn proto_i32_to_resize(resize: i32) -> Result<crate::data::Resize> {
    use crate::client_server_contract::client_server_contract::ResizeType as ProtoResize;
    let proto_resize = match resize {
        x if x == ProtoResize::Increase as i32 => ProtoResize::Increase,
        x if x == ProtoResize::Decrease as i32 => ProtoResize::Decrease,
        _ => return Err(anyhow!("Invalid ResizeType: {}", resize)),
    };
    match proto_resize {
        ProtoResize::Increase => Ok(crate::data::Resize::Increase),
        ProtoResize::Decrease => Ok(crate::data::Resize::Decrease),
        ProtoResize::Unspecified => Err(anyhow!("Unspecified ResizeType")),
    }
}

fn proto_i32_to_direction(direction: i32) -> Result<crate::data::Direction> {
    use crate::client_server_contract::client_server_contract::Direction as ProtoDirection;
    let proto_direction = match direction {
        x if x == ProtoDirection::Left as i32 => ProtoDirection::Left,
        x if x == ProtoDirection::Right as i32 => ProtoDirection::Right,
        x if x == ProtoDirection::Up as i32 => ProtoDirection::Up,
        x if x == ProtoDirection::Down as i32 => ProtoDirection::Down,
        _ => return Err(anyhow!("Invalid Direction: {}", direction)),
    };
    match proto_direction {
        ProtoDirection::Left => Ok(crate::data::Direction::Left),
        ProtoDirection::Right => Ok(crate::data::Direction::Right),
        ProtoDirection::Up => Ok(crate::data::Direction::Up),
        ProtoDirection::Down => Ok(crate::data::Direction::Down),
        ProtoDirection::Unspecified => Err(anyhow!("Unspecified direction")),
    }
}

fn proto_i32_to_search_direction(direction: i32) -> Result<crate::input::actions::SearchDirection> {
    use crate::client_server_contract::client_server_contract::SearchDirection as ProtoSearchDirection;
    let proto_direction = match direction {
        x if x == ProtoSearchDirection::Up as i32 => ProtoSearchDirection::Up,
        x if x == ProtoSearchDirection::Down as i32 => ProtoSearchDirection::Down,
        _ => return Err(anyhow!("Invalid SearchDirection: {}", direction)),
    };
    match proto_direction {
        ProtoSearchDirection::Up => Ok(crate::input::actions::SearchDirection::Up),
        ProtoSearchDirection::Down => Ok(crate::input::actions::SearchDirection::Down),
        ProtoSearchDirection::Unspecified => Err(anyhow!("Unspecified search direction")),
    }
}

fn proto_i32_to_search_option(option: i32) -> Result<crate::input::actions::SearchOption> {
    use crate::client_server_contract::client_server_contract::SearchOption as ProtoSearchOption;
    let proto_option = match option {
        x if x == ProtoSearchOption::CaseSensitivity as i32 => ProtoSearchOption::CaseSensitivity,
        x if x == ProtoSearchOption::WholeWord as i32 => ProtoSearchOption::WholeWord,
        x if x == ProtoSearchOption::Wrap as i32 => ProtoSearchOption::Wrap,
        _ => return Err(anyhow!("Invalid SearchOption: {}", option)),
    };
    match proto_option {
        ProtoSearchOption::CaseSensitivity => {
            Ok(crate::input::actions::SearchOption::CaseSensitivity)
        },
        ProtoSearchOption::Wrap => Ok(crate::input::actions::SearchOption::Wrap),
        ProtoSearchOption::WholeWord => Ok(crate::input::actions::SearchOption::WholeWord),
        ProtoSearchOption::Unspecified => Err(anyhow!("Unspecified search option")),
    }
}

fn proto_i32_to_unblock_condition(condition: i32) -> Result<crate::data::UnblockCondition> {
    use crate::client_server_contract::client_server_contract::UnblockCondition as ProtoUnblockCondition;
    let proto_condition = match condition {
        x if x == ProtoUnblockCondition::OnExitSuccess as i32 => {
            ProtoUnblockCondition::OnExitSuccess
        },
        x if x == ProtoUnblockCondition::OnExitFailure as i32 => {
            ProtoUnblockCondition::OnExitFailure
        },
        x if x == ProtoUnblockCondition::OnAnyExit as i32 => ProtoUnblockCondition::OnAnyExit,
        _ => return Err(anyhow!("Invalid UnblockCondition: {}", condition)),
    };
    match proto_condition {
        ProtoUnblockCondition::OnExitSuccess => Ok(crate::data::UnblockCondition::OnExitSuccess),
        ProtoUnblockCondition::OnExitFailure => Ok(crate::data::UnblockCondition::OnExitFailure),
        ProtoUnblockCondition::OnAnyExit => Ok(crate::data::UnblockCondition::OnAnyExit),
        ProtoUnblockCondition::Unspecified => Err(anyhow!("Unspecified unblock condition")),
    }
}

// Position conversion
impl From<crate::position::Position>
    for crate::client_server_contract::client_server_contract::Position
{
    fn from(pos: crate::position::Position) -> Self {
        Self {
            line: pos.line.0 as i32,
            column: pos.column.0 as u64,
        }
    }
}

// Reverse Position conversion
impl TryFrom<crate::client_server_contract::client_server_contract::Position>
    for crate::position::Position
{
    type Error = anyhow::Error;
    fn try_from(
        pos: crate::client_server_contract::client_server_contract::Position,
    ) -> Result<Self> {
        Ok(Self {
            line: crate::position::Line(pos.line as isize),
            column: crate::position::Column(pos.column as usize),
        })
    }
}

// OpenFilePayload conversion
impl From<crate::input::command::OpenFilePayload>
    for crate::client_server_contract::client_server_contract::OpenFilePayload
{
    fn from(payload: crate::input::command::OpenFilePayload) -> Self {
        Self {
            file_to_open: payload.path.to_string_lossy().to_string(),
            line_number: payload.line_number.map(|n| n as u32),
            cwd: payload.cwd.map(|p| p.to_string_lossy().to_string()),
            originating_plugin: payload.originating_plugin.map(|op| op.into()),
        }
    }
}

// Reverse OpenFilePayload conversion
impl TryFrom<crate::client_server_contract::client_server_contract::OpenFilePayload>
    for crate::input::command::OpenFilePayload
{
    type Error = anyhow::Error;
    fn try_from(
        payload: crate::client_server_contract::client_server_contract::OpenFilePayload,
    ) -> Result<Self> {
        Ok(Self {
            path: PathBuf::from(payload.file_to_open),
            line_number: payload.line_number.map(|n| n as usize),
            cwd: payload.cwd.map(PathBuf::from),
            originating_plugin: payload
                .originating_plugin
                .map(|op| op.try_into())
                .transpose()?,
        })
    }
}

// PaneId conversion
impl From<crate::data::PaneId> for crate::client_server_contract::client_server_contract::PaneId {
    fn from(pane_id: crate::data::PaneId) -> Self {
        use crate::client_server_contract::client_server_contract::pane_id::PaneType;
        match pane_id {
            crate::data::PaneId::Terminal(id) => Self {
                pane_type: Some(PaneType::Terminal(id)),
            },
            crate::data::PaneId::Plugin(id) => Self {
                pane_type: Some(PaneType::Plugin(id)),
            },
        }
    }
}

// Reverse PaneId conversion
impl TryFrom<crate::client_server_contract::client_server_contract::PaneId>
    for crate::data::PaneId
{
    type Error = anyhow::Error;
    fn try_from(
        pane_id: crate::client_server_contract::client_server_contract::PaneId,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::pane_id::PaneType;
        match pane_id
            .pane_type
            .ok_or_else(|| anyhow!("PaneId missing pane_type"))?
        {
            PaneType::Terminal(id) => Ok(crate::data::PaneId::Terminal(id)),
            PaneType::Plugin(id) => Ok(crate::data::PaneId::Plugin(id)),
        }
    }
}

// FloatingCoordinate conversion - SplitSize to FloatingCoordinate
impl From<crate::input::layout::SplitSize>
    for crate::client_server_contract::client_server_contract::FloatingCoordinate
{
    fn from(size: crate::input::layout::SplitSize) -> Self {
        match size {
            crate::input::layout::SplitSize::Percent(p) => Self {
                coordinate_type: Some(crate::client_server_contract::client_server_contract::floating_coordinate::CoordinateType::Percent(p as f32)),
            },
            crate::input::layout::SplitSize::Fixed(f) => Self {
                coordinate_type: Some(crate::client_server_contract::client_server_contract::floating_coordinate::CoordinateType::Fixed(f as u32)),
            },
        }
    }
}

// Reverse FloatingCoordinate conversion
impl TryFrom<crate::client_server_contract::client_server_contract::FloatingCoordinate>
    for crate::input::layout::SplitSize
{
    type Error = anyhow::Error;
    fn try_from(
        coord: crate::client_server_contract::client_server_contract::FloatingCoordinate,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::floating_coordinate::CoordinateType;
        match coord
            .coordinate_type
            .ok_or_else(|| anyhow!("FloatingCoordinate missing coordinate_type"))?
        {
            CoordinateType::Percent(p) => Ok(crate::input::layout::SplitSize::Percent(p as usize)),
            CoordinateType::Fixed(f) => Ok(crate::input::layout::SplitSize::Fixed(f as usize)),
        }
    }
}

// FloatingPaneCoordinates conversion
impl From<crate::data::FloatingPaneCoordinates>
    for crate::client_server_contract::client_server_contract::FloatingPaneCoordinates
{
    fn from(coords: crate::data::FloatingPaneCoordinates) -> Self {
        Self {
            x: coords.x.map(|x| x.into()),
            y: coords.y.map(|y| y.into()),
            width: coords.width.map(|w| w.into()),
            height: coords.height.map(|h| h.into()),
            pinned: coords.pinned,
        }
    }
}

// Reverse FloatingPaneCoordinates conversion
impl TryFrom<crate::client_server_contract::client_server_contract::FloatingPaneCoordinates>
    for crate::data::FloatingPaneCoordinates
{
    type Error = anyhow::Error;
    fn try_from(
        coords: crate::client_server_contract::client_server_contract::FloatingPaneCoordinates,
    ) -> Result<Self> {
        Ok(Self {
            x: coords.x.map(|x| x.try_into()).transpose()?,
            y: coords.y.map(|y| y.try_into()).transpose()?,
            width: coords.width.map(|w| w.try_into()).transpose()?,
            height: coords.height.map(|h| h.try_into()).transpose()?,
            pinned: coords.pinned,
        })
    }
}

// NewPanePlacement conversion
impl From<crate::data::NewPanePlacement>
    for crate::client_server_contract::client_server_contract::NewPanePlacement
{
    fn from(placement: crate::data::NewPanePlacement) -> Self {
        use crate::client_server_contract::client_server_contract::new_pane_placement::PlacementType;
        let placement_type = match placement {
            crate::data::NewPanePlacement::NoPreference => PlacementType::NoPreference(true),
            crate::data::NewPanePlacement::Tiled(direction) => {
                PlacementType::Tiled(direction.map(direction_to_proto_i32).unwrap_or(0))
            },
            crate::data::NewPanePlacement::Floating(coords) => {
                PlacementType::Floating(coords.map(|c| c.into()).unwrap_or_default())
            },
            crate::data::NewPanePlacement::InPlace {
                pane_id_to_replace,
                close_replaced_pane,
            } => PlacementType::InPlace(
                crate::client_server_contract::client_server_contract::NewPanePlacementInPlace {
                    pane_id_to_replace: pane_id_to_replace.map(|id| id.into()),
                    close_replaced_pane,
                },
            ),
            crate::data::NewPanePlacement::Stacked(pane_id) => {
                PlacementType::Stacked(pane_id.map(|id| id.into()).unwrap_or_default())
            },
        };
        Self {
            placement_type: Some(placement_type),
        }
    }
}

// Reverse NewPanePlacement conversion
impl TryFrom<crate::client_server_contract::client_server_contract::NewPanePlacement>
    for crate::data::NewPanePlacement
{
    type Error = anyhow::Error;
    fn try_from(
        placement: crate::client_server_contract::client_server_contract::NewPanePlacement,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::new_pane_placement::PlacementType;
        match placement
            .placement_type
            .ok_or_else(|| anyhow!("NewPanePlacement missing placement_type"))?
        {
            PlacementType::NoPreference(_) => Ok(crate::data::NewPanePlacement::NoPreference),
            PlacementType::Tiled(direction) => {
                let direction = if direction == 0 {
                    None
                } else {
                    Some(proto_i32_to_direction(direction)?)
                };
                Ok(crate::data::NewPanePlacement::Tiled(direction))
            },
            PlacementType::Floating(coords) => {
                let coords = if coords == Default::default() {
                    None
                } else {
                    Some(coords.try_into()?)
                };
                Ok(crate::data::NewPanePlacement::Floating(coords))
            },
            PlacementType::InPlace(in_place) => Ok(crate::data::NewPanePlacement::InPlace {
                pane_id_to_replace: in_place
                    .pane_id_to_replace
                    .map(|id| id.try_into())
                    .transpose()?,
                close_replaced_pane: in_place.close_replaced_pane,
            }),
            PlacementType::Stacked(pane_id) => {
                let pane_id = if pane_id == Default::default() {
                    None
                } else {
                    Some(pane_id.try_into()?)
                };
                Ok(crate::data::NewPanePlacement::Stacked(pane_id))
            },
        }
    }
}

// MouseEvent conversion
impl From<crate::input::mouse::MouseEvent>
    for crate::client_server_contract::client_server_contract::MouseEvent
{
    fn from(event: crate::input::mouse::MouseEvent) -> Self {
        use crate::client_server_contract::client_server_contract::{
            MouseEventType as ProtoMouseEventType, Position,
        };

        let position = Position {
            line: event.position.line.0 as i32,
            column: event.position.column.0 as u64,
        };

        let event_type = match event.event_type {
            crate::input::mouse::MouseEventType::Press => ProtoMouseEventType::Press as i32,
            crate::input::mouse::MouseEventType::Release => ProtoMouseEventType::Release as i32,
            crate::input::mouse::MouseEventType::Motion => ProtoMouseEventType::Motion as i32,
        };

        Self {
            event_type,
            left: event.left,
            right: event.right,
            middle: event.middle,
            wheel_up: event.wheel_up,
            wheel_down: event.wheel_down,
            shift: event.shift,
            alt: event.alt,
            ctrl: event.ctrl,
            position: Some(position),
        }
    }
}

// RunCommandAction conversion
impl From<crate::input::command::RunCommandAction>
    for crate::client_server_contract::client_server_contract::RunCommandAction
{
    fn from(action: crate::input::command::RunCommandAction) -> Self {
        Self {
            command: action.command.to_string_lossy().to_string(),
            args: action.args,
            cwd: action.cwd.map(|p| p.to_string_lossy().to_string()),
            direction: action.direction.map(|d| direction_to_proto_i32(d)),
            hold_on_close: action.hold_on_close,
            hold_on_start: action.hold_on_start,
            originating_plugin: action.originating_plugin.map(|op| op.into()),
            use_terminal_title: action.use_terminal_title,
        }
    }
}

// OriginatingPlugin conversion
impl From<crate::data::OriginatingPlugin>
    for crate::client_server_contract::client_server_contract::OriginatingPlugin
{
    fn from(orig: crate::data::OriginatingPlugin) -> Self {
        use std::collections::HashMap;
        let context: HashMap<String, String> =
            orig.context.into_iter().map(|(k, v)| (k, v)).collect();

        Self {
            plugin_id: orig.plugin_id,
            client_id: orig.client_id as u32,
            context,
        }
    }
}

// OriginatingPlugin reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::OriginatingPlugin>
    for crate::data::OriginatingPlugin
{
    type Error = anyhow::Error;

    fn try_from(
        orig: crate::client_server_contract::client_server_contract::OriginatingPlugin,
    ) -> Result<Self> {
        use std::collections::BTreeMap;
        let context: BTreeMap<String, String> = orig.context.into_iter().collect();

        Ok(Self {
            plugin_id: orig.plugin_id,
            client_id: orig.client_id as u16,
            context,
        })
    }
}

// SplitDirection conversion helper
fn split_direction_to_proto_i32(direction: crate::input::layout::SplitDirection) -> i32 {
    use crate::client_server_contract::client_server_contract::SplitDirection as ProtoSplitDirection;
    match direction {
        crate::input::layout::SplitDirection::Horizontal => ProtoSplitDirection::Horizontal as i32,
        crate::input::layout::SplitDirection::Vertical => ProtoSplitDirection::Vertical as i32,
    }
}

// SplitSize conversion
impl From<crate::input::layout::SplitSize>
    for crate::client_server_contract::client_server_contract::SplitSize
{
    fn from(size: crate::input::layout::SplitSize) -> Self {
        use crate::client_server_contract::client_server_contract::split_size::SizeType;
        match size {
            crate::input::layout::SplitSize::Percent(p) => Self {
                size_type: Some(SizeType::Percent(p as u32)),
            },
            crate::input::layout::SplitSize::Fixed(f) => Self {
                size_type: Some(SizeType::Fixed(f as u32)),
            },
        }
    }
}

// PercentOrFixed conversion
impl From<crate::input::layout::PercentOrFixed>
    for crate::client_server_contract::client_server_contract::PercentOrFixed
{
    fn from(size: crate::input::layout::PercentOrFixed) -> Self {
        use crate::client_server_contract::client_server_contract::percent_or_fixed::SizeType;
        match size {
            crate::input::layout::PercentOrFixed::Percent(p) => Self {
                size_type: Some(SizeType::Percent(p as u32)),
            },
            crate::input::layout::PercentOrFixed::Fixed(f) => Self {
                size_type: Some(SizeType::Fixed(f as u32)),
            },
        }
    }
}

// Run conversion
impl From<crate::input::layout::Run>
    for crate::client_server_contract::client_server_contract::Run
{
    fn from(run: crate::input::layout::Run) -> Self {
        use crate::client_server_contract::client_server_contract::run::RunType;
        match run {
            crate::input::layout::Run::Command(cmd) => Self {
                run_type: Some(RunType::Command(
                    crate::client_server_contract::client_server_contract::RunCommandAction {
                        command: cmd.command.to_string_lossy().to_string(),
                        args: cmd.args,
                        cwd: cmd.cwd.map(|p| p.to_string_lossy().to_string()),
                        direction: None, // RunCommand doesn't have direction field
                        hold_on_close: cmd.hold_on_close,
                        hold_on_start: cmd.hold_on_start,
                        originating_plugin: cmd.originating_plugin.map(|op| op.into()),
                        use_terminal_title: cmd.use_terminal_title,
                    },
                )),
            },
            crate::input::layout::Run::Plugin(plugin) => Self {
                run_type: Some(RunType::Plugin(plugin.into())),
            },
            crate::input::layout::Run::EditFile(path, line_number, cwd) => Self {
                run_type: Some(RunType::EditFile(
                    crate::client_server_contract::client_server_contract::RunEditFileAction {
                        file_path: path.to_string_lossy().to_string(),
                        line_number: line_number.map(|n| n as u32),
                        cwd: cwd.map(|p| p.to_string_lossy().to_string()),
                    },
                )),
            },
            crate::input::layout::Run::Cwd(path) => Self {
                run_type: Some(RunType::Cwd(path.to_string_lossy().to_string())),
            },
        }
    }
}

// TiledPaneLayout conversion
impl From<crate::input::layout::TiledPaneLayout>
    for crate::client_server_contract::client_server_contract::TiledPaneLayout
{
    fn from(layout: crate::input::layout::TiledPaneLayout) -> Self {
        Self {
            children_split_direction: split_direction_to_proto_i32(layout.children_split_direction),
            name: layout.name,
            children: layout.children.into_iter().map(|c| c.into()).collect(),
            split_size: layout.split_size.map(|s| s.into()),
            run: layout.run.map(|r| r.into()),
            borderless: layout.borderless,
            focus: layout.focus.map(|f| f.to_string()),
            exclude_from_sync: layout.exclude_from_sync,
            children_are_stacked: layout.children_are_stacked,
            external_children_index: layout.external_children_index.map(|l| l as u32),
            is_expanded_in_stack: layout.is_expanded_in_stack,
            hide_floating_panes: layout.hide_floating_panes,
            pane_initial_contents: layout.pane_initial_contents,
        }
    }
}

impl From<crate::input::layout::FloatingPaneLayout>
    for crate::client_server_contract::client_server_contract::FloatingPaneLayout
{
    fn from(layout: crate::input::layout::FloatingPaneLayout) -> Self {
        Self {
            name: layout.name,
            height: layout.height.map(|h| h.into()),
            width: layout.width.map(|w| w.into()),
            x: layout.x.map(|x| x.into()),
            y: layout.y.map(|y| y.into()),
            pinned: layout.pinned,
            run: layout.run.map(|r| r.into()),
            focus: layout.focus,
            already_running: layout.already_running,
            pane_initial_contents: layout.pane_initial_contents,
            logical_position: layout.logical_position.map(|l| l as u32),
        }
    }
}

impl From<crate::input::layout::SwapTiledLayout>
    for crate::client_server_contract::client_server_contract::SwapTiledLayout
{
    fn from(layout: crate::input::layout::SwapTiledLayout) -> Self {
        use crate::client_server_contract::client_server_contract::LayoutConstraintTiledPair;

        let constraint_map = layout
            .0
            .into_iter()
            .map(|(constraint, tiled_layout)| LayoutConstraintTiledPair {
                constraint: Some(constraint.into()),
                layout: Some(tiled_layout.into()),
            })
            .collect();

        Self {
            constraint_map,
            name: layout.1,
        }
    }
}

impl From<crate::input::layout::SwapFloatingLayout>
    for crate::client_server_contract::client_server_contract::SwapFloatingLayout
{
    fn from(layout: crate::input::layout::SwapFloatingLayout) -> Self {
        use crate::client_server_contract::client_server_contract::LayoutConstraintFloatingPair;

        let constraint_map = layout
            .0
            .into_iter()
            .map(
                |(constraint, floating_layouts)| LayoutConstraintFloatingPair {
                    constraint: Some(constraint.into()),
                    layouts: floating_layouts.into_iter().map(|l| l.into()).collect(),
                },
            )
            .collect();

        Self {
            constraint_map,
            name: layout.1,
        }
    }
}

// PluginUserConfiguration conversion
impl From<crate::input::layout::PluginUserConfiguration>
    for crate::client_server_contract::client_server_contract::PluginUserConfiguration
{
    fn from(config: crate::input::layout::PluginUserConfiguration) -> Self {
        Self {
            configuration: config.inner().clone().into_iter().collect(), // Convert BTreeMap to HashMap
        }
    }
}

// LayoutConstraint conversion
impl From<crate::input::layout::LayoutConstraint>
    for crate::client_server_contract::client_server_contract::LayoutConstraintWithValue
{
    fn from(constraint: crate::input::layout::LayoutConstraint) -> Self {
        use crate::client_server_contract::client_server_contract::LayoutConstraint as ProtoLayoutConstraint;
        match constraint {
            crate::input::layout::LayoutConstraint::MaxPanes(n) => Self {
                constraint_type: ProtoLayoutConstraint::MaxPanes as i32,
                value: Some(n as u32),
            },
            crate::input::layout::LayoutConstraint::MinPanes(n) => Self {
                constraint_type: ProtoLayoutConstraint::MinPanes as i32,
                value: Some(n as u32),
            },
            crate::input::layout::LayoutConstraint::ExactPanes(n) => Self {
                constraint_type: ProtoLayoutConstraint::ExactPanes as i32,
                value: Some(n as u32),
            },
            crate::input::layout::LayoutConstraint::NoConstraint => Self {
                constraint_type: ProtoLayoutConstraint::NoConstraint as i32,
                value: None,
            },
        }
    }
}

// RunPlugin conversion
impl From<crate::input::layout::RunPlugin>
    for crate::client_server_contract::client_server_contract::RunPlugin
{
    fn from(plugin: crate::input::layout::RunPlugin) -> Self {
        Self {
            allow_exec_host_cmd: plugin._allow_exec_host_cmd,
            location: Some(plugin.location.into()),
            configuration: Some(plugin.configuration.into()),
            initial_cwd: plugin.initial_cwd.map(|p| p.display().to_string()),
        }
    }
}

// PluginAlias conversion
impl From<crate::input::layout::PluginAlias>
    for crate::client_server_contract::client_server_contract::PluginAlias
{
    fn from(plugin: crate::input::layout::PluginAlias) -> Self {
        Self {
            name: plugin.name,
            configuration: plugin.configuration.map(|c| c.into()),
            initial_cwd: plugin.initial_cwd.map(|i| i.display().to_string()),
            run_plugin: plugin.run_plugin.map(|r| r.into()),
        }
    }
}

// RunPluginLocation conversion
impl From<crate::input::layout::RunPluginLocation>
    for crate::client_server_contract::client_server_contract::RunPluginLocationData
{
    fn from(location: crate::input::layout::RunPluginLocation) -> Self {
        use crate::client_server_contract::client_server_contract::{
            run_plugin_location_data::LocationData, RunPluginLocation as ProtoRunPluginLocation,
        };
        match location {
            crate::input::layout::RunPluginLocation::File(path) => Self {
                location_type: ProtoRunPluginLocation::File as i32,
                location_data: Some(LocationData::FilePath(path.to_string_lossy().to_string())),
            },
            crate::input::layout::RunPluginLocation::Zellij(tag) => Self {
                location_type: ProtoRunPluginLocation::Zellij as i32,
                location_data: Some(LocationData::ZellijTag(
                    crate::client_server_contract::client_server_contract::PluginTag {
                        tag: tag.to_string(),
                    },
                )),
            },
            crate::input::layout::RunPluginLocation::Remote(url) => Self {
                location_type: ProtoRunPluginLocation::Remote as i32,
                location_data: Some(LocationData::RemoteUrl(url)),
            },
        }
    }
}

// RunPluginOrAlias conversion
impl From<crate::input::layout::RunPluginOrAlias>
    for crate::client_server_contract::client_server_contract::RunPluginOrAlias
{
    fn from(plugin: crate::input::layout::RunPluginOrAlias) -> Self {
        use crate::client_server_contract::client_server_contract::run_plugin_or_alias::PluginType;
        match plugin {
            crate::input::layout::RunPluginOrAlias::RunPlugin(run_plugin) => Self {
                plugin_type: Some(PluginType::Plugin(run_plugin.into())),
            },
            crate::input::layout::RunPluginOrAlias::Alias(alias) => Self {
                plugin_type: Some(PluginType::Alias(alias.into())),
            },
        }
    }
}

// CommandOrPlugin conversion
impl From<crate::data::CommandOrPlugin>
    for crate::client_server_contract::client_server_contract::CommandOrPlugin
{
    fn from(cmd_or_plugin: crate::data::CommandOrPlugin) -> Self {
        use crate::client_server_contract::client_server_contract::command_or_plugin::CommandOrPluginType;
        match cmd_or_plugin {
            crate::data::CommandOrPlugin::Command(cmd) => Self {
                command_or_plugin_type: Some(CommandOrPluginType::Command(cmd.into())),
            },
            crate::data::CommandOrPlugin::Plugin(plugin) => Self {
                command_or_plugin_type: Some(CommandOrPluginType::Plugin(plugin.into())),
            },
        }
    }
}

impl TryFrom<crate::client_server_contract::client_server_contract::CommandOrPlugin>
    for crate::data::CommandOrPlugin
{
    type Error = anyhow::Error;

    fn try_from(
        proto: crate::client_server_contract::client_server_contract::CommandOrPlugin,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::command_or_plugin::CommandOrPluginType;

        let cmd_or_plugin_type = proto
            .command_or_plugin_type
            .ok_or_else(|| anyhow!("CommandOrPlugin missing command_or_plugin_type"))?;
        match cmd_or_plugin_type {
            CommandOrPluginType::Command(cmd) => {
                Ok(crate::data::CommandOrPlugin::Command(cmd.try_into()?))
            },
            CommandOrPluginType::Plugin(plugin) => {
                Ok(crate::data::CommandOrPlugin::Plugin(plugin.try_into()?))
            },
        }
    }
}

// Run reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::Run>
    for crate::input::layout::Run
{
    type Error = anyhow::Error;

    fn try_from(run: crate::client_server_contract::client_server_contract::Run) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::run::RunType;

        let run_type = run
            .run_type
            .ok_or_else(|| anyhow!("Run missing run_type"))?;
        match run_type {
            RunType::Command(cmd) => Ok(crate::input::layout::Run::Command(
                crate::input::command::RunCommand {
                    command: std::path::PathBuf::from(cmd.command),
                    args: cmd.args,
                    cwd: cmd.cwd.map(std::path::PathBuf::from),
                    hold_on_close: cmd.hold_on_close,
                    hold_on_start: cmd.hold_on_start,
                    originating_plugin: cmd
                        .originating_plugin
                        .map(|op| op.try_into())
                        .transpose()?,
                    use_terminal_title: cmd.use_terminal_title,
                },
            )),
            RunType::EditFile(edit) => Ok(crate::input::layout::Run::EditFile(
                std::path::PathBuf::from(edit.file_path),
                edit.line_number.map(|n| n as usize),
                edit.cwd.map(std::path::PathBuf::from),
            )),
            RunType::Cwd(cwd_str) => Ok(crate::input::layout::Run::Cwd(std::path::PathBuf::from(
                cwd_str,
            ))),
            RunType::Plugin(plugin) => Ok(crate::input::layout::Run::Plugin(plugin.try_into()?)),
        }
    }
}

// PercentOrFixed reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::PercentOrFixed>
    for crate::input::layout::PercentOrFixed
{
    type Error = anyhow::Error;

    fn try_from(
        value: crate::client_server_contract::client_server_contract::PercentOrFixed,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::percent_or_fixed::SizeType;

        let size_type = value
            .size_type
            .ok_or_else(|| anyhow!("PercentOrFixed missing size_type"))?;
        match size_type {
            SizeType::Percent(percent) => Ok(crate::input::layout::PercentOrFixed::Percent(
                percent as usize,
            )),
            SizeType::Fixed(fixed) => {
                Ok(crate::input::layout::PercentOrFixed::Fixed(fixed as usize))
            },
        }
    }
}

// ===== REVERSE CONVERSIONS =====

// MouseEvent reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::MouseEvent>
    for crate::input::mouse::MouseEvent
{
    type Error = anyhow::Error;

    fn try_from(
        event: crate::client_server_contract::client_server_contract::MouseEvent,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::MouseEventType as ProtoMouseEventType;

        let event_type = match event.event_type {
            x if x == ProtoMouseEventType::Press as i32 => {
                crate::input::mouse::MouseEventType::Press
            },
            x if x == ProtoMouseEventType::Release as i32 => {
                crate::input::mouse::MouseEventType::Release
            },
            x if x == ProtoMouseEventType::Motion as i32 => {
                crate::input::mouse::MouseEventType::Motion
            },
            _ => return Err(anyhow!("Invalid MouseEventType: {}", event.event_type)),
        };

        let position = event
            .position
            .ok_or_else(|| anyhow!("MouseEvent missing position"))?
            .try_into()?;

        Ok(crate::input::mouse::MouseEvent {
            event_type,
            left: event.left,
            right: event.right,
            middle: event.middle,
            wheel_up: event.wheel_up,
            wheel_down: event.wheel_down,
            shift: event.shift,
            alt: event.alt,
            ctrl: event.ctrl,
            position,
        })
    }
}

// RunCommandAction reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::RunCommandAction>
    for crate::input::command::RunCommandAction
{
    type Error = anyhow::Error;

    fn try_from(
        action: crate::client_server_contract::client_server_contract::RunCommandAction,
    ) -> Result<Self> {
        Ok(crate::input::command::RunCommandAction {
            command: std::path::PathBuf::from(action.command),
            args: action.args,
            cwd: action.cwd.map(std::path::PathBuf::from),
            direction: action.direction.map(proto_i32_to_direction).transpose()?,
            hold_on_close: action.hold_on_close,
            hold_on_start: action.hold_on_start,
            originating_plugin: action
                .originating_plugin
                .map(|op| op.try_into())
                .transpose()?,
            use_terminal_title: action.use_terminal_title,
        })
    }
}

// TiledPaneLayout reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::TiledPaneLayout>
    for crate::input::layout::TiledPaneLayout
{
    type Error = anyhow::Error;

    fn try_from(
        layout: crate::client_server_contract::client_server_contract::TiledPaneLayout,
    ) -> Result<Self> {
        use crate::input::layout::{SplitDirection, SplitSize, TiledPaneLayout};

        let children_split_direction = match layout.children_split_direction {
            x if x
                == crate::client_server_contract::client_server_contract::SplitDirection::Horizontal
                    as i32 =>
            {
                SplitDirection::Horizontal
            },
            x if x
                == crate::client_server_contract::client_server_contract::SplitDirection::Vertical
                    as i32 =>
            {
                SplitDirection::Vertical
            },
            _ => SplitDirection::Horizontal, // default
        };

        let children: Result<Vec<_>> = layout.children.into_iter().map(|c| c.try_into()).collect();
        let run = layout.run.map(|r| r.try_into()).transpose()?;

        let split_size = layout.split_size.and_then(|size| {
            use crate::client_server_contract::client_server_contract::split_size::SizeType;
            match size.size_type {
                Some(SizeType::Percent(percent)) => Some(SplitSize::Percent(percent as usize)),
                Some(SizeType::Fixed(fixed)) => Some(SplitSize::Fixed(fixed as usize)),
                None => None,
            }
        });

        Ok(TiledPaneLayout {
            children_split_direction,
            name: layout.name,
            children: children?,
            split_size,
            run,
            borderless: layout.borderless,
            focus: layout.focus.map(|f| f == "true"), // Convert string to bool
            external_children_index: layout.external_children_index.map(|l| l as usize),
            children_are_stacked: layout.children_are_stacked,
            is_expanded_in_stack: layout.is_expanded_in_stack,
            exclude_from_sync: layout.exclude_from_sync,
            run_instructions_to_ignore: vec![], // not represented in protobuf
            hide_floating_panes: layout.hide_floating_panes,
            pane_initial_contents: layout.pane_initial_contents,
        })
    }
}

// FloatingPaneLayout reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::FloatingPaneLayout>
    for crate::input::layout::FloatingPaneLayout
{
    type Error = anyhow::Error;

    fn try_from(
        layout: crate::client_server_contract::client_server_contract::FloatingPaneLayout,
    ) -> Result<Self> {
        let run = layout.run.map(|r| r.try_into()).transpose()?;
        let height = layout.height.map(|h| h.try_into()).transpose()?;
        let width = layout.width.map(|w| w.try_into()).transpose()?;
        let x = layout.x.map(|x| x.try_into()).transpose()?;
        let y = layout.y.map(|y| y.try_into()).transpose()?;

        Ok(crate::input::layout::FloatingPaneLayout {
            name: layout.name,
            height,
            width,
            x,
            y,
            pinned: layout.pinned,
            run,
            focus: layout.focus,
            already_running: layout.already_running,
            pane_initial_contents: layout.pane_initial_contents,
            logical_position: layout.logical_position.map(|p| p as usize),
        })
    }
}

// SwapTiledLayout reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::SwapTiledLayout>
    for crate::input::layout::SwapTiledLayout
{
    type Error = anyhow::Error;

    fn try_from(
        layout: crate::client_server_contract::client_server_contract::SwapTiledLayout,
    ) -> Result<Self> {
        let constraint_map: Result<BTreeMap<_, _>> = layout
            .constraint_map
            .into_iter()
            .map(|pair| {
                Ok((
                    pair.constraint
                        .ok_or_else(|| anyhow!("Missing constraint"))?
                        .try_into()?,
                    pair.layout
                        .ok_or_else(|| anyhow!("Missing layout"))?
                        .try_into()?,
                ))
            })
            .collect();
        Ok((constraint_map?, layout.name))
    }
}

// SwapFloatingLayout reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::SwapFloatingLayout>
    for crate::input::layout::SwapFloatingLayout
{
    type Error = anyhow::Error;

    fn try_from(
        layout: crate::client_server_contract::client_server_contract::SwapFloatingLayout,
    ) -> Result<Self> {
        let constraint_map: Result<BTreeMap<_, _>> = layout
            .constraint_map
            .into_iter()
            .map(|pair| {
                let floating_layouts: Result<Vec<_>> =
                    pair.layouts.into_iter().map(|l| l.try_into()).collect();
                Ok((
                    pair.constraint
                        .ok_or_else(|| anyhow!("Missing constraint"))?
                        .try_into()?,
                    floating_layouts?,
                ))
            })
            .collect();

        Ok((constraint_map?, layout.name))
    }
}

// PluginUserConfiguration reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::PluginUserConfiguration>
    for crate::input::layout::PluginUserConfiguration
{
    type Error = anyhow::Error;

    fn try_from(
        config: crate::client_server_contract::client_server_contract::PluginUserConfiguration,
    ) -> Result<Self> {
        let btree_map: BTreeMap<String, String> = config.configuration.into_iter().collect();
        Ok(crate::input::layout::PluginUserConfiguration::new(
            btree_map,
        ))
    }
}

// LayoutConstraint reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::LayoutConstraintWithValue>
    for crate::input::layout::LayoutConstraint
{
    type Error = anyhow::Error;

    fn try_from(
        constraint: crate::client_server_contract::client_server_contract::LayoutConstraintWithValue,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::LayoutConstraint as ProtoLayoutConstraint;
        match constraint.constraint_type {
            x if x == ProtoLayoutConstraint::MaxPanes as i32 => {
                let value = constraint
                    .value
                    .ok_or_else(|| anyhow!("MaxPanes constraint missing value"))?
                    as usize;
                Ok(crate::input::layout::LayoutConstraint::MaxPanes(value))
            },
            x if x == ProtoLayoutConstraint::MinPanes as i32 => {
                let value = constraint
                    .value
                    .ok_or_else(|| anyhow!("MinPanes constraint missing value"))?
                    as usize;
                Ok(crate::input::layout::LayoutConstraint::MinPanes(value))
            },
            x if x == ProtoLayoutConstraint::ExactPanes as i32 => {
                let value = constraint
                    .value
                    .ok_or_else(|| anyhow!("ExactPanes constraint missing value"))?
                    as usize;
                Ok(crate::input::layout::LayoutConstraint::ExactPanes(value))
            },
            x if x == ProtoLayoutConstraint::NoConstraint as i32 => {
                Ok(crate::input::layout::LayoutConstraint::NoConstraint)
            },
            _ => Err(anyhow!(
                "Invalid LayoutConstraint type: {}",
                constraint.constraint_type
            )),
        }
    }
}

// RunPlugin reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::RunPlugin>
    for crate::input::layout::RunPlugin
{
    type Error = anyhow::Error;

    fn try_from(
        plugin: crate::client_server_contract::client_server_contract::RunPlugin,
    ) -> Result<Self> {
        let location = plugin
            .location
            .ok_or_else(|| anyhow!("RunPlugin missing location"))?
            .try_into()?;
        let configuration = plugin
            .configuration
            .ok_or_else(|| anyhow!("RunPlugin missing configuration"))?
            .try_into()?;
        let initial_cwd = plugin.initial_cwd.map(std::path::PathBuf::from);

        Ok(crate::input::layout::RunPlugin {
            _allow_exec_host_cmd: plugin.allow_exec_host_cmd,
            location,
            configuration,
            initial_cwd,
        })
    }
}

// PluginAlias reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::PluginAlias>
    for crate::input::layout::PluginAlias
{
    type Error = anyhow::Error;

    fn try_from(
        plugin_alias: crate::client_server_contract::client_server_contract::PluginAlias,
    ) -> Result<Self> {
        let run_plugin = plugin_alias.run_plugin.and_then(|r| r.try_into().ok());
        let configuration = plugin_alias.configuration.and_then(|c| c.try_into().ok());
        let initial_cwd = plugin_alias.initial_cwd.map(std::path::PathBuf::from);
        Ok(crate::input::layout::PluginAlias {
            name: plugin_alias.name,
            configuration,
            initial_cwd,
            run_plugin,
        })
    }
}

// RunPluginLocation reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::RunPluginLocationData>
    for crate::input::layout::RunPluginLocation
{
    type Error = anyhow::Error;

    fn try_from(
        location: crate::client_server_contract::client_server_contract::RunPluginLocationData,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::{
            run_plugin_location_data::LocationData, RunPluginLocation as ProtoRunPluginLocation,
        };

        let location_data = location
            .location_data
            .ok_or_else(|| anyhow!("RunPluginLocationData missing location_data"))?;
        match location.location_type {
            x if x == ProtoRunPluginLocation::File as i32 => {
                if let LocationData::FilePath(path) = location_data {
                    Ok(crate::input::layout::RunPluginLocation::File(
                        std::path::PathBuf::from(path),
                    ))
                } else {
                    Err(anyhow!("File location type but wrong data variant"))
                }
            },
            x if x == ProtoRunPluginLocation::Zellij as i32 => {
                if let LocationData::ZellijTag(tag) = location_data {
                    Ok(crate::input::layout::RunPluginLocation::Zellij(
                        crate::data::PluginTag::new(tag.tag),
                    ))
                } else {
                    Err(anyhow!("Zellij location type but wrong data variant"))
                }
            },
            x if x == ProtoRunPluginLocation::Remote as i32 => {
                if let LocationData::RemoteUrl(url) = location_data {
                    Ok(crate::input::layout::RunPluginLocation::Remote(url))
                } else {
                    Err(anyhow!("Remote location type but wrong data variant"))
                }
            },
            _ => Err(anyhow!(
                "Invalid RunPluginLocation type: {}",
                location.location_type
            )),
        }
    }
}

// RunPluginOrAlias reverse conversion
impl TryFrom<crate::client_server_contract::client_server_contract::RunPluginOrAlias>
    for crate::input::layout::RunPluginOrAlias
{
    type Error = anyhow::Error;

    fn try_from(
        plugin: crate::client_server_contract::client_server_contract::RunPluginOrAlias,
    ) -> Result<Self> {
        use crate::client_server_contract::client_server_contract::run_plugin_or_alias::PluginType;

        let plugin_type = plugin
            .plugin_type
            .ok_or_else(|| anyhow!("RunPluginOrAlias missing plugin_type"))?;
        match plugin_type {
            PluginType::Plugin(run_plugin) => Ok(
                crate::input::layout::RunPluginOrAlias::RunPlugin(run_plugin.try_into()?),
            ),
            PluginType::Alias(plugin_alias) => Ok(crate::input::layout::RunPluginOrAlias::Alias(
                plugin_alias.try_into()?,
            )),
        }
    }
}
