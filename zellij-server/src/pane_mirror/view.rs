use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use zellij_utils::data::{KeyWithModifier, MirrorClosedReason, MirrorState, PaneAddress};
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::pane_mirror::{
    mirror_pin_renew_interval, MirrorAttach, MirrorConnector, MirrorEventSink, MirrorInput,
    MirrorScreen, MirrorTransport, MirrorTransportEvent, SourceMessage, ViewerMessage,
    MIRROR_CONNECT_TIMEOUT, PANE_MIRROR_VERSION,
};
use zellij_utils::pane_size::Size;

pub type MirrorEventRoute = Arc<dyn Fn(u32, u64, MirrorTransportEvent) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirrorRedraw {
    Nothing,
    Full,
    Rows(BTreeSet<u16>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirrorUpdate {
    pub state_changed: bool,
    pub redraw: MirrorRedraw,
}

struct ParkedConnection {
    target: PaneAddress,
    generation: u64,
    transport: Box<dyn MirrorTransport>,
}

pub struct MirrorView {
    view_id: u32,
    target: Option<PaneAddress>,
    state: MirrorState,
    generation: u64,
    next_generation: u64,
    parked: VecDeque<ParkedConnection>,
    transport: Option<Box<dyn MirrorTransport>>,
    connector: Arc<dyn MirrorConnector>,
    route: MirrorEventRoute,
    view_only: bool,
    resize_allowed: bool,
    focused: bool,
    pinned: bool,
    source_size: Option<Size>,
    connecting_since: Option<Instant>,
    last_resize_sent: Option<Instant>,
    screen: Option<MirrorScreen>,
    held_screen: Option<MirrorScreen>,
    pin_ready_at: Option<Instant>,
    resize_flag_sent: bool,
    viewer_size: Size,
    viewer_sessions: Vec<String>,
}

pub const SWITCH_HOLD: std::time::Duration = std::time::Duration::from_millis(400);
pub const PIN_SETTLE: std::time::Duration = std::time::Duration::from_millis(300);
pub const MAX_PARKED_CONNECTIONS: usize = 8;

impl std::fmt::Debug for MirrorView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MirrorView")
            .field("view_id", &self.view_id)
            .field("target", &self.target)
            .field("state", &self.state)
            .field("generation", &self.generation)
            .field("view_only", &self.view_only)
            .field("resize_allowed", &self.resize_allowed)
            .field("focused", &self.focused)
            .field("pinned", &self.pinned)
            .finish()
    }
}

impl Drop for MirrorView {
    fn drop(&mut self) {
        self.disconnect();
        self.detach_parked();
    }
}

impl MirrorView {
    pub fn new(
        view_id: u32,
        connector: Arc<dyn MirrorConnector>,
        route: MirrorEventRoute,
        view_only: bool,
        resize_allowed: bool,
    ) -> Self {
        MirrorView {
            view_id,
            target: None,
            state: MirrorState::NoTarget,
            generation: 0,
            next_generation: 0,
            parked: VecDeque::new(),
            transport: None,
            connector,
            route,
            view_only,
            resize_allowed,
            focused: false,
            pinned: false,
            source_size: None,
            connecting_since: None,
            last_resize_sent: None,
            screen: None,
            held_screen: None,
            pin_ready_at: None,
            resize_flag_sent: false,
            viewer_size: Size::default(),
            viewer_sessions: vec![],
        }
    }

    pub fn set_viewer_sessions(&mut self, viewer_sessions: Vec<String>) {
        self.viewer_sessions = viewer_sessions;
    }

    pub fn target(&self) -> Option<&PaneAddress> {
        self.target.as_ref()
    }

    pub fn state(&self) -> MirrorState {
        self.state
    }

    pub fn view_id(&self) -> u32 {
        self.view_id
    }

    pub fn resize_allowed(&self) -> bool {
        self.resize_allowed && !self.view_only
    }

    pub fn is_view_only(&self) -> bool {
        self.view_only
    }

    pub fn is_focused(&self) -> bool {
        self.focused
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    pub fn source_size(&self) -> Option<Size> {
        self.source_size
    }

    pub fn wants_pin(&self) -> bool {
        self.resize_allowed && !self.view_only
    }

    pub fn alternate_screen_active(&self) -> bool {
        self.is_live()
            && self
                .screen
                .as_ref()
                .map(|screen| screen.alternate_screen)
                .unwrap_or(false)
    }

    fn asks_for_resize(&self, now: Instant) -> bool {
        self.resize_allowed
            && !self.view_only
            && (self.focused || self.pin_ready_at.map(|at| now >= at).unwrap_or(true))
    }

    fn send_resize(&mut self, now: Instant) {
        let resize_allowed = self.asks_for_resize(now);
        let Some(transport) = self.transport.as_ref() else {
            return;
        };
        transport.send(ViewerMessage::Resize {
            size: self.viewer_size,
            focused: self.focused,
            resize_allowed,
        });
        self.resize_flag_sent = resize_allowed;
        self.last_resize_sent = Some(now);
    }

    fn hold_until(&self) -> Option<Instant> {
        if self.held_screen.is_none() || self.state != MirrorState::Connecting {
            return None;
        }
        self.connecting_since.map(|since| since + SWITCH_HOLD)
    }

    pub fn shown_screen(&self) -> Option<&MirrorScreen> {
        self.live_screen().or_else(|| {
            self.hold_until()
                .filter(|until| Instant::now() < *until)
                .and(self.held_screen.as_ref())
        })
    }

    pub fn set_focused(&mut self, focused: bool) -> bool {
        if self.focused == focused {
            return false;
        }
        self.focused = focused;
        self.send_resize(Instant::now());
        true
    }

    pub fn set_resize_allowed(&mut self, resize_allowed: bool) -> bool {
        if self.view_only || self.resize_allowed == resize_allowed {
            return false;
        }
        self.resize_allowed = resize_allowed;
        self.send_resize(Instant::now());
        true
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        let connect_deadline = match self.state {
            MirrorState::Connecting => self
                .connecting_since
                .map(|since| since + MIRROR_CONNECT_TIMEOUT),
            _ => None,
        };
        let renew_deadline = if self.wants_pin() && self.transport.is_some() {
            let renewal = self
                .last_resize_sent
                .map(|sent| sent + mirror_pin_renew_interval())
                .unwrap_or_else(Instant::now);
            let settle = self
                .pin_ready_at
                .filter(|_| !self.resize_flag_sent && !self.focused);
            Some(settle.map(|at| at.min(renewal)).unwrap_or(renewal))
        } else {
            None
        };
        [connect_deadline, renew_deadline, self.hold_until()]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn tick(&mut self, now: Instant) -> Option<MirrorUpdate> {
        if self.state == MirrorState::Connecting
            && self
                .connecting_since
                .map(|since| now.saturating_duration_since(since) >= MIRROR_CONNECT_TIMEOUT)
                .unwrap_or(false)
        {
            log::warn!(
                "Mirror {} gave up connecting after {:?}",
                self.view_id,
                MIRROR_CONNECT_TIMEOUT
            );
            return Some(self.close_with(MirrorClosedReason::ConnectionFailed));
        }
        let renewal_due = self
            .last_resize_sent
            .map(|sent| now.saturating_duration_since(sent) >= mirror_pin_renew_interval())
            .unwrap_or(true);
        let settled = self.asks_for_resize(now) != self.resize_flag_sent;
        if self.wants_pin() && (renewal_due || settled) {
            self.send_resize(now);
        }
        if let Some(until) = self.hold_until() {
            if now >= until {
                self.held_screen = None;
                return Some(MirrorUpdate {
                    state_changed: false,
                    redraw: MirrorRedraw::Full,
                });
            }
        }
        None
    }

    pub fn is_live(&self) -> bool {
        self.state == MirrorState::Live && self.transport.is_some()
    }

    pub fn wants_mouse(&self) -> bool {
        self.is_live()
            && self
                .screen
                .as_ref()
                .map(|screen| screen.wants_mouse)
                .unwrap_or(false)
    }

    pub fn parked_targets(&self) -> Vec<PaneAddress> {
        self.parked
            .iter()
            .map(|parked| parked.target.clone())
            .collect()
    }

    fn detach_parked(&mut self) {
        for mut parked in self.parked.drain(..) {
            parked.transport.close();
        }
    }

    fn new_generation(&mut self) -> u64 {
        self.next_generation += 1;
        self.next_generation
    }

    fn park_current(&mut self) {
        let parkable = matches!(self.state, MirrorState::Connecting | MirrorState::Live);
        match (self.transport.take(), self.target.clone()) {
            (Some(transport), Some(target)) if parkable && target.is_local() => {
                if !transport.send(ViewerMessage::Park) {
                    let mut transport = transport;
                    transport.close();
                } else {
                    let stale: Vec<usize> = self
                        .parked
                        .iter()
                        .enumerate()
                        .filter(|(_, parked)| parked.target == target)
                        .map(|(index, _)| index)
                        .collect();
                    for index in stale.into_iter().rev() {
                        if let Some(mut parked) = self.parked.remove(index) {
                            parked.transport.close();
                        }
                    }
                    self.parked.push_front(ParkedConnection {
                        target,
                        generation: self.generation,
                        transport,
                    });
                    while self.parked.len() > MAX_PARKED_CONNECTIONS {
                        if let Some(mut oldest) = self.parked.pop_back() {
                            oldest.transport.close();
                        }
                    }
                }
            },
            (Some(mut transport), _) => transport.close(),
            (None, _) => {},
        }
        self.disconnect();
    }

    fn take_parked(&mut self, target: &PaneAddress) -> Option<ParkedConnection> {
        let index = self
            .parked
            .iter()
            .position(|parked| parked.target == *target)?;
        self.parked.remove(index)
    }

    fn handle_parked_event(&mut self, generation: u64, event: MirrorTransportEvent) {
        let Some(index) = self
            .parked
            .iter()
            .position(|parked| parked.generation == generation)
        else {
            return;
        };
        let keep = match event {
            MirrorTransportEvent::Message(SourceMessage::Frame { frame, .. }) => {
                match zellij_utils::structured_render::decode(&frame) {
                    Ok(view) => {
                        let seq = view.header().seq;
                        self.parked[index]
                            .transport
                            .send(ViewerMessage::FrameAck { seq })
                    },
                    Err(_) => false,
                }
            },
            MirrorTransportEvent::Message(SourceMessage::Attached { version, .. }) => {
                version == PANE_MIRROR_VERSION
            },
            MirrorTransportEvent::Message(SourceMessage::PinState { .. }) => true,
            MirrorTransportEvent::Message(SourceMessage::Closed { .. })
            | MirrorTransportEvent::Unreadable(_)
            | MirrorTransportEvent::ConnectFailed(_)
            | MirrorTransportEvent::Disconnected => false,
        };
        if !keep {
            if let Some(mut parked) = self.parked.remove(index) {
                parked.transport.close();
            }
        }
    }

    fn disconnect(&mut self) {
        if let Some(mut transport) = self.transport.take() {
            transport.close();
        }
        self.screen = None;
        self.pinned = false;
        self.source_size = None;
        self.connecting_since = None;
        self.last_resize_sent = None;
    }

    pub fn set_target(&mut self, target: Option<PaneAddress>, viewer_size: Size) -> MirrorUpdate {
        let held = if self.state == MirrorState::Live {
            self.screen.take()
        } else {
            self.held_screen.take()
        };
        self.park_current();
        self.held_screen = held;
        self.pin_ready_at = Some(Instant::now() + PIN_SETTLE);
        self.viewer_size = viewer_size;
        let previous_state = self.state;
        let previous_target = self.target.clone();
        self.target = target;
        let wanted = self.target.clone();
        let parked = wanted.as_ref().and_then(|target| self.take_parked(target));
        self.state = match (wanted, parked) {
            (_, Some(parked)) => {
                self.generation = parked.generation;
                let resize_allowed = self.wants_pin();
                let unparked = parked
                    .transport
                    .send(ViewerMessage::Unpark { resize_allowed });
                self.transport = Some(parked.transport);
                let now = Instant::now();
                self.connecting_since = Some(now);
                if unparked {
                    self.send_resize(now);
                    MirrorState::Connecting
                } else {
                    self.close_with(MirrorClosedReason::ConnectionLost);
                    self.state
                }
            },
            (None, None) => {
                self.generation = self.new_generation();
                MirrorState::NoTarget
            },
            (Some(target), None) if !target.is_local() => {
                self.generation = self.new_generation();
                MirrorState::Closed(MirrorClosedReason::UnsupportedHost)
            },
            (Some(target), None) => {
                self.generation = self.new_generation();
                let generation = self.generation;
                let view_id = self.view_id;
                let route = self.route.clone();
                let sink: MirrorEventSink =
                    Arc::new(move |event| route(view_id, generation, event));
                let resize_allowed = self.asks_for_resize(Instant::now());
                self.resize_flag_sent = resize_allowed;
                let attach = MirrorAttach {
                    view_only: self.view_only,
                    focused: self.focused,
                    resize_allowed,
                    viewer_sessions: self.viewer_sessions.clone(),
                    ..MirrorAttach::new(target.pane_id, viewer_size)
                };
                self.transport = Some(self.connector.connect(&target, attach, sink));
                let now = Instant::now();
                self.connecting_since = Some(now);
                self.last_resize_sent = Some(now);
                MirrorState::Connecting
            },
        };
        MirrorUpdate {
            state_changed: previous_state != self.state || previous_target != self.target,
            redraw: MirrorRedraw::Full,
        }
    }

    fn close_with(&mut self, reason: MirrorClosedReason) -> MirrorUpdate {
        let was_pinned = self.pinned;
        self.disconnect();
        self.held_screen = None;
        let state_changed = self.state != MirrorState::Closed(reason) || was_pinned;
        self.state = MirrorState::Closed(reason);
        MirrorUpdate {
            state_changed,
            redraw: MirrorRedraw::Full,
        }
    }

    pub fn handle_event(
        &mut self,
        generation: u64,
        event: MirrorTransportEvent,
    ) -> Option<MirrorUpdate> {
        if generation != self.generation || self.transport.is_none() {
            self.handle_parked_event(generation, event);
            return None;
        }
        match event {
            MirrorTransportEvent::Message(SourceMessage::Attached { version, .. }) => {
                if version != PANE_MIRROR_VERSION {
                    return Some(self.close_with(MirrorClosedReason::VersionMismatch));
                }
                None
            },
            MirrorTransportEvent::Message(SourceMessage::Frame {
                frame,
                alternate_screen,
            }) => {
                let screen = self.screen.get_or_insert_with(|| MirrorScreen::blank(0, 0));
                let applied = match screen.apply_frame(&frame) {
                    Ok(applied) => applied,
                    Err(e) => {
                        log::error!("Mirror frame could not be read: {}", e);
                        return Some(self.close_with(MirrorClosedReason::ProtocolError));
                    },
                };
                screen.alternate_screen = alternate_screen;
                self.connecting_since = None;
                if let Some(transport) = self.transport.as_ref() {
                    transport.send(ViewerMessage::FrameAck { seq: applied.seq });
                }
                let became_live = self.state != MirrorState::Live;
                self.state = MirrorState::Live;
                self.held_screen = None;
                Some(MirrorUpdate {
                    state_changed: became_live,
                    redraw: if applied.full || became_live {
                        MirrorRedraw::Full
                    } else {
                        MirrorRedraw::Rows(applied.changed_rows)
                    },
                })
            },
            MirrorTransportEvent::Message(SourceMessage::Closed { reason, .. }) => {
                Some(self.close_with(reason))
            },
            MirrorTransportEvent::Message(SourceMessage::PinState { pinned, pane_size }) => {
                let pin_changed = self.pinned != pinned;
                let size_changed = self.source_size != Some(pane_size);
                self.pinned = pinned;
                self.source_size = Some(pane_size);
                Some(MirrorUpdate {
                    state_changed: pin_changed || size_changed,
                    redraw: MirrorRedraw::Nothing,
                })
            },
            MirrorTransportEvent::Unreadable(e) => {
                log::error!("Mirror message could not be read: {}", e);
                Some(self.close_with(MirrorClosedReason::ProtocolError))
            },
            MirrorTransportEvent::ConnectFailed(e) => {
                log::warn!("Mirror connection failed: {}", e);
                Some(self.close_with(MirrorClosedReason::ConnectionFailed))
            },
            MirrorTransportEvent::Disconnected => {
                Some(self.close_with(MirrorClosedReason::ConnectionLost))
            },
        }
    }

    pub fn send_input(&self, input: MirrorInput) -> bool {
        if self.view_only || !self.is_live() {
            return false;
        }
        self.transport
            .as_ref()
            .map(|transport| transport.send(ViewerMessage::Input(input)))
            .unwrap_or(false)
    }

    pub fn send_key(
        &self,
        key: &Option<KeyWithModifier>,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    ) -> bool {
        match key {
            Some(key) => self.send_input(MirrorInput::Key {
                key: key.clone(),
                raw_bytes,
                is_kitty_keyboard_protocol,
            }),
            None if raw_bytes.is_empty() => false,
            None => self.send_input(MirrorInput::Bytes(raw_bytes)),
        }
    }

    pub fn send_mouse(&self, event: &MouseEvent) -> bool {
        if !self.wants_mouse() {
            return false;
        }
        let inside = self
            .screen
            .as_ref()
            .map(|screen| {
                event.position.line() >= 0
                    && (event.position.line() as usize) < screen.rows as usize
                    && event.position.column() < screen.cols as usize
            })
            .unwrap_or(false);
        inside && self.send_input(MirrorInput::Mouse(*event))
    }

    pub fn resized(&mut self, viewer_size: Size) -> bool {
        if self.viewer_size == viewer_size {
            return false;
        }
        self.viewer_size = viewer_size;
        self.send_resize(Instant::now());
        true
    }

    pub fn live_screen(&self) -> Option<&MirrorScreen> {
        match self.state {
            MirrorState::Live => self.screen.as_ref(),
            _ => None,
        }
    }

    pub fn placeholder_lines(&self) -> Vec<String> {
        let target = self
            .target
            .as_ref()
            .map(|target| format!("{} / {}", target.session, target.pane_id));
        let headline = match self.state {
            MirrorState::NoTarget => "No pane selected".to_owned(),
            MirrorState::Connecting | MirrorState::Live => "Connecting...".to_owned(),
            MirrorState::Closed(MirrorClosedReason::PaneClosed) => "The pane was closed".to_owned(),
            MirrorState::Closed(MirrorClosedReason::PaneNotFound) => {
                "The pane was not found".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::SessionEnding) => {
                "The session ended".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::Rejected) => {
                "The session refused to mirror this pane".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::VersionMismatch) => {
                "Preview not available".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::UnsupportedHost) => {
                let host = self
                    .target
                    .as_ref()
                    .and_then(|target| target.host.as_ref())
                    .map(|host| host.0.clone())
                    .unwrap_or_default();
                format!("Unsupported host: {}", host)
            },
            MirrorState::Closed(MirrorClosedReason::ConnectionFailed)
            | MirrorState::Closed(MirrorClosedReason::ConnectionLost) => {
                "Preview not available".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::ProtocolError) => {
                "The session sent data that could not be read".to_owned()
            },
            MirrorState::Closed(MirrorClosedReason::ContainsViewer) => {
                "This pane runs this session".to_owned()
            },
        };
        let mut lines = vec![headline];
        if matches!(
            self.state,
            MirrorState::Closed(
                MirrorClosedReason::ConnectionFailed
                    | MirrorClosedReason::ConnectionLost
                    | MirrorClosedReason::VersionMismatch
            )
        ) {
            lines.push(
                "The session may be running an older or unsupported version of Zellij".to_owned(),
            );
        }
        if let Some(target) = target {
            lines.push(target);
        }
        lines
    }
}

#[cfg(test)]
#[path = "./unit/view_tests.rs"]
mod view_tests;
