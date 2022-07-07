//! The `[fake_client]` is used to attach to a running server session
//! and dispatch actions, that are specified through the command line.
//! Multiple actions at the same time can be dispatched.
use log::debug;
use std::sync::{Arc, Mutex};
use std::{fs, path::PathBuf, thread};
use zellij_tile::prelude::{ClientId, Style};
use zellij_utils::errors::ContextType;

use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_actions,
    os_input_output::ClientOsApi, stdin_ansi_parser::StdinAnsiParser, stdin_handler::stdin_loop,
    ClientInfo, ClientInstruction, InputInstruction,
};
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    cli::CliArgs,
    input::{actions::Action, config::Config, layout::LayoutFromYaml, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ServerToClientMsg},
};

pub fn start_fake_client(
    os_input: Box<dyn ClientOsApi>,
    _opts: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
    _layout: Option<LayoutFromYaml>,
    actions: Vec<Action>,
) {
    debug!("Starting fake Zellij client!");
    let session_name = info.get_session_name();

    // TODO: Ideally the `fake_client` would not need to specify these options,
    // but the `[NewTab:]` action depends on this state being
    // even in this client.
    let palette = config.themes.clone().map_or_else(
        || os_input.load_palette(),
        |t| {
            t.theme_config(&config_options)
                .unwrap_or_else(|| os_input.load_palette())
        },
    );

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style {
            colors: palette,
            rounded_corners: config.ui.unwrap_or_default().pane_frames.rounded_corners,
        },
    };

    let first_msg = ClientToServerMsg::AttachClient(client_attributes, config_options.clone());

    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir, 0o700).unwrap();
        sock_dir.push(session_name);
        sock_dir
    };
    os_input.connect_to_server(&*zellij_ipc_pipe);
    os_input.send_to_server(first_msg);

    let mut command_is_executing = CommandIsExecuting::new();

    let (send_client_instructions, receive_client_instructions): ChannelWithContext<
        ClientInstruction,
    > = channels::bounded(50);
    let send_client_instructions = SenderWithContext::new(send_client_instructions);

    let (send_input_instructions, receive_input_instructions): ChannelWithContext<
        InputInstruction,
    > = channels::bounded(50);
    let send_input_instructions = SenderWithContext::new(send_input_instructions);

    std::panic::set_hook({
        use zellij_utils::errors::handle_panic;
        let send_client_instructions = send_client_instructions.clone();
        Box::new(move |info| {
            handle_panic(info, &send_client_instructions);
        })
    });

    let stdin_ansi_parser = Arc::new(Mutex::new(StdinAnsiParser::new()));
    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_input_instructions = send_input_instructions.clone();
            let stdin_ansi_parser = stdin_ansi_parser.clone();
            move || stdin_loop(os_input, send_input_instructions, stdin_ansi_parser)
        });

    let clients: Vec<ClientId>;
    os_input.send_to_server(ClientToServerMsg::ListClients);
    #[allow(clippy::collapsible_match)]
    loop {
        if let Some((msg, _)) = os_input.recv_from_server() {
            if let ServerToClientMsg::ActiveClients(active_clients) = msg {
                clients = active_clients;
                break;
            }
        }
    }
    debug!("The connected client id's are: {:?}.", clients);

    let _input_thread = thread::Builder::new()
        .name("input_handler".to_string())
        .spawn({
            let send_client_instructions = send_client_instructions.clone();
            let command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let default_mode = config_options.default_mode.unwrap_or_default();
            let session_name = session_name.to_string();
            move || {
                input_actions(
                    os_input,
                    config,
                    config_options,
                    command_is_executing,
                    clients,
                    send_client_instructions,
                    default_mode,
                    receive_input_instructions,
                    actions,
                    session_name,
                )
            }
        });

    let router_thread = thread::Builder::new()
        .name("router".to_string())
        .spawn({
            let os_input = os_input.clone();
            let mut should_break = false;
            move || loop {
                if let Some((instruction, err_ctx)) = os_input.recv_from_server() {
                    err_ctx.update_thread_ctx();
                    if let ServerToClientMsg::Exit(_) = instruction {
                        should_break = true;
                    }
                    send_client_instructions.send(instruction.into()).unwrap();
                    if should_break {
                        break;
                    }
                }
            }
        })
        .unwrap();

    loop {
        let (client_instruction, mut err_ctx) = receive_client_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::Client((&client_instruction).into()));
        match client_instruction {
            ClientInstruction::Exit(_) => {
                os_input.send_to_server(ClientToServerMsg::ClientExited);
                break;
            },
            ClientInstruction::Error(_) => {
                let _ = os_input.send_to_server(ClientToServerMsg::Action(Action::Quit, None));
                // handle_error(backtrace);
            },
            ClientInstruction::Render(_) => {
                // This is a fake client, that doesn't render, but
                // dispatches actions.
            },
            ClientInstruction::UnblockInputThread => {
                command_is_executing.unblock_input_thread();
            },
            ClientInstruction::SwitchToMode(input_mode) => {
                send_input_instructions
                    .send(InputInstruction::SwitchToMode(input_mode))
                    .unwrap();
            },
            _ => {},
        }
    }
    router_thread.join().unwrap();
}
