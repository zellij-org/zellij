use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use interprocess::local_socket::Stream as LocalSocketStream;
use zellij_server::os_input_output::{AsyncReader, ServerOsApi};
use zellij_server::panes::PaneId;
use zellij_server::ClientId;
use zellij_utils::data::Palette;
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::{RunCommand, TerminalAction};
use zellij_utils::ipc::{
    ClientToServerMsg, IpcReceiverWithContext, IpcSenderWithContext, ServerToClientMsg,
};
use zellij_utils::shared::default_palette;

use crate::fake_pty::{SharedPtys, FAKE_PID_BASE};

#[derive(Clone)]
struct NonBlockingClientSender {
    buffer_tx: crossbeam::channel::Sender<ServerToClientMsg>,
}

impl NonBlockingClientSender {
    fn new(mut ipc_sender: IpcSenderWithContext<ServerToClientMsg>) -> Self {
        let (buffer_tx, buffer_rx) = crossbeam::channel::bounded::<ServerToClientMsg>(5000);
        std::thread::Builder::new()
            .name("non_blocking_client_sender".to_string())
            .spawn(move || {
                for msg in buffer_rx.iter() {
                    if ipc_sender.send_server_msg(msg).is_err() {
                        break;
                    }
                }
            })
            .unwrap();
        NonBlockingClientSender { buffer_tx }
    }
}

#[derive(Clone, Default)]
pub struct FakeServerOsApi {
    pub shared_ptys: SharedPtys,
    non_blocking_client_senders: Arc<Mutex<HashMap<ClientId, NonBlockingClientSender>>>,
    fake_filesystem: Arc<Mutex<HashMap<String, String>>>,
}

impl FakeServerOsApi {
    pub fn written_files(&self) -> HashMap<String, String> {
        self.fake_filesystem.lock().unwrap().clone()
    }
}

impl ServerOsApi for FakeServerOsApi {
    fn set_terminal_size_using_terminal_id(
        &self,
        id: u32,
        cols: u16,
        rows: u16,
        _width_in_pixels: Option<u16>,
        _height_in_pixels: Option<u16>,
    ) -> Result<()> {
        self.shared_ptys.set_size(id, cols, rows);
        Ok(())
    }
    fn spawn_terminal(
        &self,
        terminal_action: TerminalAction,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
        _default_editor: Option<PathBuf>,
    ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
        let terminal_id = self.shared_ptys.next_terminal_id();
        let opened_file_contents = match &terminal_action {
            TerminalAction::OpenFile(payload) => self
                .fake_filesystem
                .lock()
                .unwrap()
                .get(&payload.path.to_string_lossy().to_string())
                .cloned(),
            _ => None,
        };
        let fake_async_reader =
            self.shared_ptys
                .register(terminal_id, Some(terminal_action), Some(quit_cb));
        if let Some(contents) = opened_file_contents {
            let contents_with_carriage_returns = contents.replace("\r\n", "\n").replace('\n', "\r\n");
            self.shared_ptys
                .write_output(terminal_id, contents_with_carriage_returns.as_bytes());
        }
        Ok((
            terminal_id,
            fake_async_reader,
            Some(FAKE_PID_BASE + terminal_id),
        ))
    }
    fn reserve_terminal_id(&self) -> Result<u32> {
        let terminal_id = self.shared_ptys.next_terminal_id();
        let _ = self.shared_ptys.register(terminal_id, None, None);
        Ok(terminal_id)
    }
    fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
        if self.shared_ptys.append_stdin(terminal_id, buf) {
            Ok(buf.len())
        } else {
            Err(anyhow!("no fake pty for terminal id {terminal_id}"))
        }
    }
    fn tcdrain(&self, _terminal_id: u32) -> Result<()> {
        Ok(())
    }
    fn kill(&self, pid: u32) -> Result<()> {
        self.shared_ptys
            .exit_terminal(pid.saturating_sub(FAKE_PID_BASE), None);
        Ok(())
    }
    fn force_kill(&self, pid: u32) -> Result<()> {
        self.shared_ptys
            .exit_terminal(pid.saturating_sub(FAKE_PID_BASE), None);
        Ok(())
    }
    fn send_sigint(&self, pid: u32) -> Result<()> {
        self.shared_ptys
            .exit_terminal(pid.saturating_sub(FAKE_PID_BASE), None);
        Ok(())
    }
    fn box_clone(&self) -> Box<dyn ServerOsApi> {
        Box::new(self.clone())
    }
    fn send_to_client(&self, client_id: ClientId, msg: ServerToClientMsg) -> Result<()> {
        if let Some(non_blocking_client_sender) = self
            .non_blocking_client_senders
            .lock()
            .unwrap()
            .get(&client_id)
        {
            non_blocking_client_sender
                .buffer_tx
                .try_send(msg)
                .with_context(|| format!("failed to buffer message for client {client_id}"))?;
        }
        Ok(())
    }
    fn new_client(
        &mut self,
        client_id: ClientId,
        stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        let ipc_receiver = IpcReceiverWithContext::new(stream);
        let non_blocking_client_sender = NonBlockingClientSender::new(ipc_receiver.get_sender());
        self.non_blocking_client_senders
            .lock()
            .unwrap()
            .insert(client_id, non_blocking_client_sender);
        Ok(ipc_receiver)
    }
    fn new_client_with_reply(
        &mut self,
        _client_id: ClientId,
        _stream: LocalSocketStream,
        _reply_stream: LocalSocketStream,
    ) -> Result<IpcReceiverWithContext<ClientToServerMsg>> {
        unimplemented!("windows dual-pipe IPC is not used by the test harness")
    }
    fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        self.non_blocking_client_senders
            .lock()
            .unwrap()
            .remove(&client_id);
        Ok(())
    }
    fn load_palette(&self) -> Palette {
        default_palette()
    }
    fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
        None
    }
    fn write_to_file(&mut self, buf: String, file: Option<String>) -> Result<()> {
        if let Some(file) = file {
            self.fake_filesystem.lock().unwrap().insert(file, buf);
        }
        Ok(())
    }
    fn re_run_command_in_terminal(
        &self,
        terminal_id: u32,
        _run_command: RunCommand,
        quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
    ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
        let fake_async_reader = self.shared_ptys.rerun(terminal_id, quit_cb);
        Ok((fake_async_reader, Some(FAKE_PID_BASE + terminal_id)))
    }
    fn clear_terminal_id(&self, terminal_id: u32) -> Result<()> {
        self.shared_ptys.remove(terminal_id);
        Ok(())
    }
}
