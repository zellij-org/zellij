use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::common::{
	ApiCommand, AppInstruction, ChannelWithContext, IpcSenderWithContext, SenderType,
	SenderWithContext,
};
use crate::errors::{ContextType, ErrorContext, PtyContext};
use crate::layout::Layout;
use crate::os_input_output::OsApi;
use crate::panes::PaneId;
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use crate::utils::consts::ZELLIJ_IPC_PIPE;
use crate::wasm_vm::PluginInstruction;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;

pub fn start_server(
	os_input: Box<dyn OsApi>,
	opts: CliArgs,
	command_is_executing: CommandIsExecuting,
	mut send_app_instructions: SenderWithContext<AppInstruction>,
) -> thread::JoinHandle<()> {
	let (send_pty_instructions, receive_pty_instructions): ChannelWithContext<PtyInstruction> =
		channel();
	let mut send_pty_instructions = SenderWithContext::new(
		ErrorContext::new(),
		SenderType::Sender(send_pty_instructions),
	);

	std::fs::remove_file(ZELLIJ_IPC_PIPE).ok();
	let listener = std::os::unix::net::UnixListener::bind(ZELLIJ_IPC_PIPE)
		.expect("could not listen on ipc socket");

	// Don't use default layouts in tests, but do everywhere else
	#[cfg(not(test))]
	let default_layout = Some(PathBuf::from("default"));
	#[cfg(test)]
	let default_layout = None;
	let maybe_layout = opts.layout.or(default_layout);

	let send_server_instructions = IpcSenderWithContext::new();

	let mut pty_bus = PtyBus::new(
		receive_pty_instructions,
		os_input.clone(),
		send_server_instructions,
		opts.debug,
	);

	let pty_thread = thread::Builder::new()
		.name("pty".to_string())
		.spawn({
			let mut command_is_executing = command_is_executing.clone();
			send_pty_instructions.send(PtyInstruction::NewTab).unwrap();
			move || loop {
				let (event, mut err_ctx) = pty_bus
					.receive_pty_instructions
					.recv()
					.expect("failed to receive event on channel");
				err_ctx.add_call(ContextType::Pty(PtyContext::from(&event)));
				match event {
					PtyInstruction::SpawnTerminal(file_to_open) => {
						let pid = pty_bus.spawn_terminal(file_to_open);
						pty_bus
							.send_server_instructions
							.send(ApiCommand::ToScreen(ScreenInstruction::NewPane(
								PaneId::Terminal(pid),
							)))
							.unwrap();
					}
					PtyInstruction::SpawnTerminalVertically(file_to_open) => {
						let pid = pty_bus.spawn_terminal(file_to_open);
						pty_bus
							.send_server_instructions
							.send(ApiCommand::ToScreen(ScreenInstruction::VerticalSplit(
								PaneId::Terminal(pid),
							)))
							.unwrap();
					}
					PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
						let pid = pty_bus.spawn_terminal(file_to_open);
						pty_bus
							.send_server_instructions
							.send(ApiCommand::ToScreen(ScreenInstruction::HorizontalSplit(
								PaneId::Terminal(pid),
							)))
							.unwrap();
					}
					PtyInstruction::NewTab => {
						//if let Some(layout) = maybe_layout.clone() {
						//    pty_bus.spawn_terminals_for_layout(layout, err_ctx);
						//} else {
						let pid = pty_bus.spawn_terminal(None);
						pty_bus
							.send_server_instructions
							.send(ApiCommand::ToScreen(ScreenInstruction::NewTab(pid)))
							.unwrap();
						//}
					}
					PtyInstruction::ClosePane(id) => {
						pty_bus.close_pane(id, err_ctx);
						command_is_executing.done_closing_pane();
					}
					PtyInstruction::CloseTab(ids) => {
						pty_bus.close_tab(ids, err_ctx);
						command_is_executing.done_closing_pane();
					}
					PtyInstruction::Quit => {
						break;
					}
				}
			}
		})
		.unwrap();

	thread::Builder::new()
		.name("ipc_server".to_string())
		.spawn({
			move || {
				let mut threads = vec![];
				for stream in listener.incoming() {
					match stream {
						Ok(stream) => {
							let send_app_instructions = send_app_instructions.clone();
							let send_pty_instructions = send_pty_instructions.clone();
							threads.push(thread::spawn(move || {
								handle_stream(send_pty_instructions, send_app_instructions, stream);
							}));
						}
						Err(err) => {
							panic!("err {:?}", err);
						}
					}
				}

				let _ = pty_thread.join();
				for t in threads {
					t.join();
				}
			}
		})
		.unwrap()
}

fn handle_stream(
	mut send_pty_instructions: SenderWithContext<PtyInstruction>,
	mut send_app_instructions: SenderWithContext<AppInstruction>,
	mut stream: std::os::unix::net::UnixStream,
) {
	//let mut buffer = [0; 65535]; // TODO: more accurate
	let mut buffer = String::new();
	loop {
		let bytes = stream
			.read_to_string(&mut buffer)
			.expect("failed to parse ipc message");
		//let astream = stream.try_clone().unwrap();
		let (mut err_ctx, decoded): (ErrorContext, ApiCommand) =
			bincode::deserialize(buffer.as_bytes()).expect("failed to deserialize ipc message");
		err_ctx.add_call(ContextType::IPCServer);
		send_pty_instructions.update(err_ctx);
		send_app_instructions.update(err_ctx);

		eprintln!("Server received {:?}", decoded);

		match decoded {
			ApiCommand::OpenFile(file_name) => {
				let path = PathBuf::from(file_name);
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminal(Some(path)))
					.unwrap();
			}
			ApiCommand::SplitHorizontally => {
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminalHorizontally(None))
					.unwrap();
			}
			ApiCommand::SplitVertically => {
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminalVertically(None))
					.unwrap();
			}
			ApiCommand::MoveFocus => {
				send_app_instructions
					.send(AppInstruction::ToScreen(ScreenInstruction::MoveFocus))
					.unwrap();
			}
			ApiCommand::ToPty(instruction) => {
				send_pty_instructions.send(instruction).unwrap();
			}
			ApiCommand::ToScreen(instruction) => {
				send_app_instructions
					.send(AppInstruction::ToScreen(instruction))
					.unwrap();
			}
			ApiCommand::ClosePluginPane(pid) => {
				send_app_instructions
					.send(AppInstruction::ToPlugin(PluginInstruction::Unload(pid)))
					.unwrap();
			}
			ApiCommand::Quit => {
				let _ = send_pty_instructions.send(PtyInstruction::Quit);
				break;
			}
		}
	}
}
