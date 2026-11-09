use crate::tab::Pane;

use crate::{
    os_input_output::ServerOsApi, panes::PaneId, pty_writer::PtyWriteInstruction, ClientId,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use zellij_utils::channels::SenderWithContext;

#[derive(Clone)]
pub struct ActivePanes {
    active_panes: HashMap<ClientId, PaneId>,
    last_panes: HashMap<ClientId, PaneId>,
    os_api: Box<dyn ServerOsApi>,
    pty_writer: Option<SenderWithContext<PtyWriteInstruction>>,
}

impl std::fmt::Debug for ActivePanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.active_panes)
    }
}

impl ActivePanes {
    pub fn new(os_api: &Box<dyn ServerOsApi>) -> Self {
        let os_api = os_api.clone();
        ActivePanes {
            active_panes: HashMap::new(),
            last_panes: HashMap::new(),
            os_api,
            pty_writer: None,
        }
    }
    pub fn with_pty_writer(
        os_api: &Box<dyn ServerOsApi>,
        pty_writer: SenderWithContext<PtyWriteInstruction>,
    ) -> Self {
        let mut active_panes = Self::new(os_api);
        active_panes.pty_writer = Some(pty_writer);
        active_panes
    }
    pub fn get(&self, client_id: &ClientId) -> Option<&PaneId> {
        self.active_panes.get(client_id)
    }
    pub fn get_last(&self, client_id: &ClientId) -> Option<&PaneId> {
        self.last_panes.get(client_id)
    }
    pub fn insert(
        &mut self,
        client_id: ClientId,
        pane_id: PaneId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) {
        self.unfocus_pane_for_client(client_id, panes);
        self.active_panes.insert(client_id, pane_id);
        self.focus_pane(pane_id, panes);
    }
    pub fn set_last_pane(&mut self, client_id: ClientId, pane_id: PaneId) {
        self.last_panes.insert(client_id, pane_id);
    }
    pub fn clear(&mut self, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        for pane_id in self.active_panes.values() {
            self.unfocus_pane(*pane_id, panes);
        }
        self.active_panes.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.active_panes.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&ClientId, &PaneId)> {
        self.active_panes.iter()
    }
    pub fn values(&self) -> impl Iterator<Item = &PaneId> {
        self.active_panes.values()
    }
    pub fn remove(
        &mut self,
        client_id: &ClientId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) -> Option<PaneId> {
        if let Some(pane_id_to_unfocus) = self.active_panes.get(&client_id) {
            self.unfocus_pane(*pane_id_to_unfocus, panes);
        }
        self.active_panes.remove(client_id)
    }
    pub fn unfocus_all_panes(
        &self,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
        connected_clients: &HashSet<ClientId>,
    ) {
        for pane_id in self.panes_focused_by(connected_clients) {
            self.unfocus_pane(pane_id, panes);
        }
    }
    pub fn focus_all_panes(
        &self,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
        connected_clients: &HashSet<ClientId>,
    ) {
        for pane_id in self.panes_focused_by(connected_clients) {
            self.focus_pane(pane_id, panes);
        }
    }
    fn panes_focused_by(&self, connected_clients: &HashSet<ClientId>) -> BTreeSet<PaneId> {
        self.active_panes
            .iter()
            .filter(|(client_id, _)| connected_clients.contains(client_id))
            .map(|(_, pane_id)| *pane_id)
            .collect()
    }
    pub fn retarget_last_pane(&mut self, from_pane_id: PaneId, to_pane_id: PaneId) {
        for last_pane_id in self.last_panes.values_mut() {
            if *last_pane_id == from_pane_id {
                *last_pane_id = to_pane_id;
            }
        }
    }
    pub fn clone_active_panes(&self) -> HashMap<ClientId, PaneId> {
        self.active_panes.clone()
    }
    pub fn contains_key(&self, client_id: &ClientId) -> bool {
        self.active_panes.contains_key(client_id)
    }
    fn unfocus_pane_for_client(
        &self,
        client_id: ClientId,
        panes: &mut BTreeMap<PaneId, Box<dyn Pane>>,
    ) {
        if let Some(pane_id_to_unfocus) = self.active_panes.get(&client_id) {
            self.unfocus_pane(*pane_id_to_unfocus, panes);
        }
    }
    fn unfocus_pane(&self, pane_id: PaneId, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        if let PaneId::Terminal(terminal_id) = pane_id {
            if let Some(focus_event) = panes.get(&pane_id).and_then(|p| p.unfocus_event()) {
                self.send_focus_event(terminal_id, focus_event);
            }
        }
    }
    fn focus_pane(&self, pane_id: PaneId, panes: &mut BTreeMap<PaneId, Box<dyn Pane>>) {
        if let PaneId::Terminal(terminal_id) = pane_id {
            if let Some(focus_event) = panes.get(&pane_id).and_then(|p| p.focus_event()) {
                self.send_focus_event(terminal_id, focus_event);
            }
        }
    }
    fn send_focus_event(&self, terminal_id: u32, focus_event: String) {
        if let Some(pty_writer) = &self.pty_writer {
            let _ = pty_writer.send(PtyWriteInstruction::Write(
                focus_event.into_bytes(),
                terminal_id,
                None,
            ));
        } else {
            let _ = self
                .os_api
                .write_to_tty_stdin(terminal_id, focus_event.as_bytes());
        }
    }
    pub fn pane_id_is_focused(&self, pane_id: &PaneId) -> bool {
        self.active_panes
            .values()
            .find(|p_id| **p_id == *pane_id)
            .is_some()
    }
    /// Focus reported to queries / list-panes: only currently connected
    /// clients count. When nobody is connected, remembered focus is kept so a
    /// background tab can restore it on the next attach.
    pub fn pane_id_is_focused_for_query(
        &self,
        pane_id: &PaneId,
        connected_clients: &HashSet<ClientId>,
    ) -> bool {
        if connected_clients.is_empty() {
            return self.pane_id_is_focused(pane_id);
        }
        self.active_panes
            .iter()
            .any(|(client_id, focused)| connected_clients.contains(client_id) && focused == pane_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::os_input_output::AsyncReader;
    use crate::panes::kitty_graphics::KittyImageStore;
    use crate::panes::sixel::SixelImageStore;
    use crate::panes::terminal_pane::TerminalPane;
    use crate::tab::test_panes::FocusReportingPane;
    use interprocess::local_socket::Stream as LocalSocketStream;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::{Arc, Mutex};
    use zellij_utils::channels::{unbounded, ChannelWithContext};
    use zellij_utils::data::{Palette, Style};
    use zellij_utils::errors::prelude::*;
    use zellij_utils::input::command::{RunCommand, TerminalAction};
    use zellij_utils::ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg};
    use zellij_utils::pane_size::PaneGeom;

    #[derive(Clone, Default)]
    struct TestOsApi {
        writes: Arc<Mutex<Vec<(u32, String)>>>,
    }

    impl ServerOsApi for TestOsApi {
        fn set_terminal_size_using_terminal_id(
            &self,
            _id: u32,
            _cols: u16,
            _rows: u16,
            _width_in_pixels: Option<u16>,
            _height_in_pixels: Option<u16>,
        ) -> Result<()> {
            Ok(())
        }
        fn spawn_terminal(
            &self,
            _terminal_action: TerminalAction,
            _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
            _default_editor: Option<PathBuf>,
            _pane_env: &crate::os_input_output::PaneEnv,
        ) -> Result<(u32, Box<dyn AsyncReader>, Option<u32>)> {
            unimplemented!()
        }
        fn write_to_tty_stdin(&self, terminal_id: u32, buf: &[u8]) -> Result<usize> {
            self.writes
                .lock()
                .unwrap()
                .push((terminal_id, String::from_utf8_lossy(buf).into_owned()));
            Ok(buf.len())
        }
        fn tcdrain(&self, _terminal_id: u32) -> Result<()> {
            Ok(())
        }
        fn kill(&self, _pid: u32) -> Result<()> {
            Ok(())
        }
        fn force_kill(&self, _pid: u32) -> Result<()> {
            Ok(())
        }
        fn send_sigint(&self, _pid: u32) -> Result<()> {
            Ok(())
        }
        fn box_clone(&self) -> Box<dyn ServerOsApi> {
            Box::new(self.clone())
        }
        fn send_to_client(&self, _client_id: ClientId, _msg: ServerToClientMsg) -> Result<()> {
            Ok(())
        }
        fn register_client(
            &mut self,
            _client_id: ClientId,
            _receiver: &IpcReceiverWithContext<ClientToServerMsg>,
        ) -> Result<()> {
            Ok(())
        }
        fn register_client_with_reply(
            &mut self,
            _client_id: ClientId,
            _reply_stream: LocalSocketStream,
        ) -> Result<()> {
            Ok(())
        }
        fn remove_client(&mut self, _client_id: ClientId) -> Result<()> {
            Ok(())
        }
        fn load_palette(&self) -> Palette {
            Palette::default()
        }
        fn get_cwd(&self, _pid: u32) -> Option<PathBuf> {
            None
        }
        fn write_to_file(&mut self, _buf: String, _file: Option<String>) -> Result<()> {
            Ok(())
        }
        fn re_run_command_in_terminal(
            &self,
            _terminal_id: u32,
            _run_command: RunCommand,
            _quit_cb: Box<dyn Fn(PaneId, Option<i32>, RunCommand) + Send>,
            _pane_env: &crate::os_input_output::PaneEnv,
        ) -> Result<(Box<dyn AsyncReader>, Option<u32>)> {
            unimplemented!()
        }
        fn clear_terminal_id(&self, _terminal_id: u32) -> Result<()> {
            Ok(())
        }
    }

    fn focus_reporting_pane(id: u32) -> Box<dyn Pane> {
        let mut geom = PaneGeom::default();
        geom.cols.set_inner(20);
        geom.rows.set_inner(5);
        Box::new(FocusReportingPane {
            inner: Box::new(TerminalPane::new(
                id,
                geom,
                Style::default(),
                0,
                String::new(),
                Rc::new(RefCell::new(None)),
                Rc::new(RefCell::new(SixelImageStore::default())),
                Rc::new(RefCell::new(KittyImageStore::default())),
                Rc::new(RefCell::new(Palette::default())),
                Rc::new(RefCell::new(HashMap::new())),
                None,
                None,
                false,
                true,
                true,
                true,
                false,
                None,
            )),
        })
    }

    #[test]
    fn focus_events_use_the_configured_pty_writer_and_fall_back_without_one() {
        let os_api = TestOsApi::default();
        let writes = os_api.writes.clone();
        let boxed_os_api: Box<dyn ServerOsApi> = Box::new(os_api);
        let mut panes = BTreeMap::from([(PaneId::Terminal(2), focus_reporting_pane(2))]);
        let (to_pty_writer, pty_writer_instructions): ChannelWithContext<PtyWriteInstruction> =
            unbounded();
        let mut active_panes =
            ActivePanes::with_pty_writer(&boxed_os_api, SenderWithContext::new(to_pty_writer));

        active_panes.insert(1, PaneId::Terminal(2), &mut panes);
        let (instruction, _) = pty_writer_instructions.try_recv().unwrap();
        assert!(
            matches!(
                instruction,
                PtyWriteInstruction::Write(bytes, 2, None) if bytes == b"focus-in"
            ),
            "focus event did not go through the pty writer"
        );
        assert!(writes.lock().unwrap().is_empty());

        active_panes.clear(&mut panes);
        let (instruction, _) = pty_writer_instructions.try_recv().unwrap();
        assert!(matches!(
            instruction,
            PtyWriteInstruction::Write(bytes, 2, None) if bytes == b"focus-out"
        ));
        assert!(writes.lock().unwrap().is_empty());

        let mut writerless = ActivePanes::new(&boxed_os_api);
        writerless.insert(1, PaneId::Terminal(2), &mut panes);
        assert_eq!(*writes.lock().unwrap(), vec![(2, "focus-in".to_owned())]);
    }

    #[test]
    fn query_focus_ignores_disconnected_clients_but_remembers_when_alone() {
        let boxed_os_api: Box<dyn ServerOsApi> = Box::new(TestOsApi::default());
        let mut panes = BTreeMap::from([
            (PaneId::Terminal(2), focus_reporting_pane(2)),
            (PaneId::Terminal(3), focus_reporting_pane(3)),
        ]);
        let mut active_panes = ActivePanes::new(&boxed_os_api);
        active_panes.insert(1, PaneId::Terminal(2), &mut panes);
        active_panes.insert(2, PaneId::Terminal(3), &mut panes);

        let only_client_one: HashSet<ClientId> = [1].into_iter().collect();
        assert!(active_panes.pane_id_is_focused_for_query(&PaneId::Terminal(2), &only_client_one));
        assert!(!active_panes.pane_id_is_focused_for_query(&PaneId::Terminal(3), &only_client_one));

        let nobody: HashSet<ClientId> = HashSet::new();
        assert!(
            active_panes.pane_id_is_focused_for_query(&PaneId::Terminal(3), &nobody),
            "with no connected clients, remembered focus is kept for reattach"
        );
    }
}
