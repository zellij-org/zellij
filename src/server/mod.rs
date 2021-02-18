use crate::cli::CliArgs;
use crate::command_is_executing::CommandIsExecuting;
use crate::common::{
	AppInstruction, ChannelWithContext, IpcSenderWithContext, SenderType, SenderWithContext,
	ServerInstruction,
};
use crate::errors::{ContextType, ErrorContext, PtyContext};
use crate::layout::Layout;
use crate::os_input_output::OsApi;
use crate::panes::PaneId;
use crate::pty_bus::{PtyBus, PtyInstruction};
use crate::screen::ScreenInstruction;
use crate::utils::consts::ZELLIJ_IPC_PIPE;
use crate::wasm_vm::PluginInstruction;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use std::io::{BufReader, Read};
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
	let listener =
		LocalSocketListener::bind(ZELLIJ_IPC_PIPE).expect("could not listen on ipc socket");

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
							.send(ServerInstruction::ToScreen(ScreenInstruction::NewPane(
								PaneId::Terminal(pid),
							)))
							.unwrap();
					}
					PtyInstruction::SpawnTerminalVertically(file_to_open) => {
						let pid = pty_bus.spawn_terminal(file_to_open);
						pty_bus
							.send_server_instructions
							.send(ServerInstruction::ToScreen(
								ScreenInstruction::VerticalSplit(PaneId::Terminal(pid)),
							))
							.unwrap();
					}
					PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
						let pid = pty_bus.spawn_terminal(file_to_open);
						pty_bus
							.send_server_instructions
							.send(ServerInstruction::ToScreen(
								ScreenInstruction::HorizontalSplit(PaneId::Terminal(pid)),
							))
							.unwrap();
					}
					PtyInstruction::NewTab => {
						if let Some(layout) = maybe_layout.clone() {
							pty_bus.spawn_terminals_for_layout(layout);
						} else {
							let pid = pty_bus.spawn_terminal(None);
							pty_bus
								.send_server_instructions
								.send(ServerInstruction::ToScreen(ScreenInstruction::NewTab(pid)))
								.unwrap();
						}
					}
					PtyInstruction::ClosePane(id) => {
						pty_bus.close_pane(id);
						command_is_executing.done_closing_pane();
					}
					PtyInstruction::CloseTab(ids) => {
						pty_bus.close_tab(ids);
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
				let mut km = 0;
				for stream in listener.incoming() {
					match stream {
						Ok(stream) => {
							let send_app_instructions = send_app_instructions.clone();
							let send_pty_instructions = send_pty_instructions.clone();
							let nm = format!("{}", km);
							thread::Builder::new()
								.name(nm)
								.spawn(move || {
									handle_stream(
										send_pty_instructions,
										send_app_instructions,
										stream,
										km,
									);
								})
								.unwrap();
							km += 1;
						}
						Err(err) => {
							panic!("err {:?}", err);
						}
					}
				}
			}
		})
		.unwrap();
	pty_thread
}

fn handle_stream(
	mut send_pty_instructions: SenderWithContext<PtyInstruction>,
	mut send_app_instructions: SenderWithContext<AppInstruction>,
	mut stream: LocalSocketStream,
	km: u32,
) {
	let mut reader = BufReader::new(stream);
	let mut buffer = [0; 65535]; // TODO: more accurate
	loop {
		let bytes = reader
			.read(&mut buffer)
			.expect("failed to parse ipc message");
		let (mut err_ctx, decoded): (ErrorContext, ServerInstruction) =
			match bincode::deserialize(&buffer[..bytes]) {
				Ok(d) => d,
				Err(_) => break,
			};
		err_ctx.add_call(ContextType::IPCServer);
		send_pty_instructions.update(err_ctx);
		send_app_instructions.update(err_ctx);

		match decoded {
			ServerInstruction::OpenFile(file_name) => {
				let path = PathBuf::from(file_name);
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminal(Some(path)))
					.unwrap();
			}
			ServerInstruction::SplitHorizontally => {
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminalHorizontally(None))
					.unwrap();
			}
			ServerInstruction::SplitVertically => {
				send_pty_instructions
					.send(PtyInstruction::SpawnTerminalVertically(None))
					.unwrap();
			}
			ServerInstruction::MoveFocus => {
				send_app_instructions
					.send(AppInstruction::ToScreen(ScreenInstruction::MoveFocus))
					.unwrap();
			}
			ServerInstruction::ToPty(instruction) => {
				send_pty_instructions.send(instruction).unwrap();
			}
			ServerInstruction::ToScreen(instruction) => {
				send_app_instructions
					.send(AppInstruction::ToScreen(instruction))
					.unwrap();
			}
			ServerInstruction::ClosePluginPane(pid) => {
				send_app_instructions
					.send(AppInstruction::ToPlugin(PluginInstruction::Unload(pid)))
					.unwrap();
			}
			ServerInstruction::Quit => {
				let _ = send_pty_instructions.send(PtyInstruction::Quit);
				break;
			}
		}
	}
}
