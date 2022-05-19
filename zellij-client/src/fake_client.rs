use log::info;
use std::io::Write;
use std::path::PathBuf;
use std::{fs, thread};
use zellij_tile::prelude::Style;
use zellij_utils::errors::ContextType;

use crate::input_handler::input_actions;
use crate::{
    command_is_executing::CommandIsExecuting, input_handler::input_loop,
    os_input_output::ClientOsApi, stdin_handler::stdin_loop,
};
use crate::{ClientInfo, ClientInstruction, InputInstruction};
use zellij_utils::{
    channels::{self, ChannelWithContext, SenderWithContext},
    consts::ZELLIJ_IPC_PIPE,
    input::{actions::Action, config::Config, options::Options},
    ipc::{ClientAttributes, ClientToServerMsg, ExitReason, ServerToClientMsg},
};
use zellij_utils::{cli::CliArgs, input::layout::LayoutFromYaml};

// keep the args for a little while,
// it could turn out that we actually
// want them also for the fake client
pub fn start_fake_client(
    os_input: Box<dyn ClientOsApi>,
    _opts: CliArgs,
    config: Config,
    config_options: Options,
    info: ClientInfo,
    _layout: Option<LayoutFromYaml>,
    actions: Vec<Action>,
) {
    info!("Starting fake Zellij client!");

    let session_name = info.get_session_name();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    let client_attributes = ClientAttributes {
        size: full_screen_ws,
        style: Style::default(),
    };

    let first_msg = ClientToServerMsg::AttachClient(client_attributes, config_options.clone());

    let zellij_ipc_pipe: PathBuf = {
        let mut sock_dir = zellij_utils::consts::ZELLIJ_SOCK_DIR.clone();
        fs::create_dir_all(&sock_dir).unwrap();
        zellij_utils::shared::set_permissions(&sock_dir).unwrap();
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

    let on_force_close = config_options.on_force_close.unwrap_or_default();

    let _stdin_thread = thread::Builder::new()
        .name("stdin_handler".to_string())
        .spawn({
            let os_input = os_input.clone();
            let send_input_instructions = send_input_instructions.clone();
            move || stdin_loop(os_input, send_input_instructions)
        });

    // get client ids
    // os_input.connect_to_server(&*zellij_ipc_pipe);
    os_input.send_to_server(ClientToServerMsg::ListClients);
    let (clients, _) =  os_input.recv_from_server();
    log::error!("{:?}", clients);


    let session_name = session_name.to_string().clone();
    let _input_thread = thread::Builder::new()
        .name("input_handler".to_string())
        .spawn({
            let send_client_instructions = send_client_instructions.clone();
            let command_is_executing = command_is_executing.clone();
            let os_input = os_input.clone();
            let default_mode = config_options.default_mode.unwrap_or_default();
            move || {
                input_actions(
                    os_input,
                    config,
                    config_options,
                    command_is_executing,
                    send_client_instructions,
                    default_mode,
                    receive_input_instructions,
                    actions,
                    session_name,
                )
            }
        });

    let _signal_thread = thread::Builder::new()
        .name("signal_listener".to_string())
        .spawn({
            let os_input = os_input.clone();
            move || {
                os_input.handle_signals(
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::TerminalResize(
                                os_api.get_terminal_size_using_fd(0),
                            ));
                        }
                    }),
                    Box::new({
                        let os_api = os_input.clone();
                        move || {
                            os_api.send_to_server(ClientToServerMsg::Action(on_force_close.into()));
                        }
                    }),
                );
            }
        })
        .unwrap();

    let router_thread = thread::Builder::new()
        .name("router".to_string())
        .spawn({
            let os_input = os_input.clone();
            let mut should_break = false;
            move || loop {
                let (instruction, err_ctx) = os_input.recv_from_server();
                err_ctx.update_thread_ctx();
                if let ServerToClientMsg::Exit(_) = instruction {
                    should_break = true;
                }
                send_client_instructions.send(instruction.into()).unwrap();
                if should_break {
                    break;
                }
            }
        })
        .unwrap();

    let handle_error = |backtrace: String| {
        //os_input.unset_raw_mode(0);
        //let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.rows, 1);
        //let restore_snapshot = "\u{1b}[?1049l";
        //os_input.disable_mouse();
        let error = format!(
            "{}",
            //"{}\n{}{}",
            //restore_snapshot, goto_start_of_last_line,
            backtrace
        );
        let _ = os_input
            .get_stdout_writer()
            .write(error.as_bytes())
            .unwrap();
        let _ = os_input.get_stdout_writer().flush().unwrap();
        std::process::exit(1);
    };

    //let exit_msg: String;

    loop {
        let (client_instruction, mut err_ctx) = receive_client_instructions
            .recv()
            .expect("failed to receive app instruction on channel");

        err_ctx.add_call(ContextType::Client((&client_instruction).into()));
        match client_instruction {
            ClientInstruction::Exit(reason) => {
                os_input.send_to_server(ClientToServerMsg::ClientExited);

                if let ExitReason::Error(_) = reason {
                    handle_error(reason.to_string());
                }
                //exit_msg = reason.to_string();
                break;
            }
            ClientInstruction::Error(backtrace) => {
                let _ = os_input.send_to_server(ClientToServerMsg::Action(Action::Quit));
                handle_error(backtrace);
            }
            ClientInstruction::Render(_) => {
                // we are a fake client
            }
            ClientInstruction::UnblockInputThread => {
                command_is_executing.unblock_input_thread();
            }
            ClientInstruction::SwitchToMode(input_mode) => {
                send_input_instructions
                    .send(InputInstruction::SwitchToMode(input_mode))
                    .unwrap();
            }
            _ => {}
        }
    }

    router_thread.join().unwrap();

    //os_input.disable_mouse();
    //info!("{}", exit_msg);
    //os_input.unset_raw_mode(0);
    //let mut stdout = os_input.get_stdout_writer();
    //let _ = stdout.write(goodbye_message.as_bytes()).unwrap();
    //stdout.flush().unwrap();
}
