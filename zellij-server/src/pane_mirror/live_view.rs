use super::cells::{
    cursor_shape_csi, placeholder_chunks, screen_chunks_from, screen_offset, LiveViewRect,
};
use super::view::{MirrorRedraw, MirrorView};
use crate::output::CharacterChunk;
use crate::panes::terminal_character::LinkAnchor;
use crate::panes::LinkHandler;
use crate::ClientId;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use zellij_utils::data::{
    BareKey, Event, KeyModifier, KeyWithModifier, LiveViewRelease, LiveViewReleaseGesture,
    MirrorState, PaneAddress, DEFAULT_LIVE_VIEW_RELEASE_KEY, LIVE_VIEW_DOUBLE_PRESS_MS,
};
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::pane_mirror::MirrorInput;
use zellij_utils::pane_size::Size;
use zellij_utils::position::Position;

pub type LiveViewKey = (u32, ClientId);
pub type SharedLiveViews = Rc<RefCell<LiveViews>>;
pub type SharedLiveViewCaptures = Arc<Mutex<LiveViewCaptures>>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LiveViewReleaseKey {
    #[default]
    Bound,
    Configured(LiveViewReleaseGesture),
}

#[derive(Debug, Default)]
pub struct LiveViewCaptures {
    gestures: BTreeMap<ClientId, LiveViewReleaseKey>,
    presses: BTreeMap<ClientId, std::time::Instant>,
    bound_keys: BTreeMap<ClientId, Vec<KeyWithModifier>>,
}

impl LiveViewCaptures {
    pub fn set(&mut self, client_id: ClientId, gesture: Option<LiveViewReleaseKey>) {
        self.presses.remove(&client_id);
        match gesture {
            Some(gesture) => {
                self.gestures.insert(client_id, gesture);
            },
            None => {
                self.gestures.remove(&client_id);
            },
        }
    }

    pub fn is_capturing(&self, client_id: ClientId) -> bool {
        self.gestures.contains_key(&client_id)
    }

    pub fn forget_presses(&mut self, client_id: ClientId) {
        self.presses.remove(&client_id);
    }

    pub fn forget_client(&mut self, client_id: ClientId) {
        self.gestures.remove(&client_id);
        self.presses.remove(&client_id);
        self.bound_keys.remove(&client_id);
    }

    pub fn publish_bound_keys(
        &mut self,
        bound_keys: BTreeMap<ClientId, Vec<KeyWithModifier>>,
    ) -> bool {
        if self.bound_keys == bound_keys {
            return false;
        }
        self.bound_keys = bound_keys;
        true
    }

    pub fn bound_keys_of(&self, client_id: ClientId) -> Vec<KeyWithModifier> {
        self.bound_keys.get(&client_id).cloned().unwrap_or_default()
    }

    pub fn key_pressed(
        &mut self,
        client_id: ClientId,
        key: &KeyWithModifier,
        bound_keys: impl FnOnce() -> Vec<KeyWithModifier>,
    ) -> bool {
        if self.gestures.contains_key(&client_id) && *key == type_key() {
            self.presses.remove(&client_id);
            return true;
        }
        match self.gestures.get(&client_id) {
            Some(LiveViewReleaseKey::Configured(gesture)) => {
                is_release_press(gesture, key, &mut self.presses, client_id)
            },
            Some(LiveViewReleaseKey::Bound) => {
                self.presses.remove(&client_id);
                release_keys_or_default(bound_keys()).contains(key)
            },
            None => false,
        }
    }
}

pub fn keys_bound_to_the_inbox<'a>(
    bindings: impl IntoIterator<Item = (&'a KeyWithModifier, &'a Vec<Action>)>,
) -> Vec<KeyWithModifier> {
    let mut keys: Vec<KeyWithModifier> = bindings
        .into_iter()
        .filter(|(_, actions)| {
            actions
                .iter()
                .any(|action| matches!(action, Action::ToggleSessionCard))
        })
        .map(|(key, _)| key.clone())
        .collect();
    let primary = default_release_key().key;
    keys.sort_by(|a, b| {
        (*b == primary)
            .cmp(&(*a == primary))
            .then(a.key_modifiers.len().cmp(&b.key_modifiers.len()))
            .then(a.cmp(b))
    });
    keys.dedup();
    keys
}

pub fn release_keys_or_default(bound_keys: Vec<KeyWithModifier>) -> Vec<KeyWithModifier> {
    if bound_keys.is_empty() {
        vec![default_release_key().key]
    } else {
        bound_keys
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragButton {
    Left,
    Right,
    Middle,
}

impl DragButton {
    pub fn of(event: &MouseEvent) -> Option<Self> {
        if event.left {
            Some(DragButton::Left)
        } else if event.right {
            Some(DragButton::Right)
        } else if event.middle {
            Some(DragButton::Middle)
        } else {
            None
        }
    }

    fn release_at(&self, position: Position) -> MouseEvent {
        match self {
            DragButton::Left => MouseEvent::new_left_release_event(position),
            DragButton::Right => MouseEvent::new_right_release_event(position),
            DragButton::Middle => MouseEvent::new_middle_release_event(position),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveViewDrag {
    pub key: LiveViewKey,
    pub button: DragButton,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewReport {
    pub target: Option<PaneAddress>,
    pub state: MirrorState,
    pub input_captured: bool,
    pub input_released_by: Option<LiveViewRelease>,
    pub resize_allowed: bool,
    pub view_only: bool,
    pub pinned: bool,
    pub source_size: Option<Size>,
}

impl LiveViewReport {
    pub fn cleared(input_released_by: Option<LiveViewRelease>, view_only: bool) -> Self {
        LiveViewReport {
            target: None,
            state: MirrorState::NoTarget,
            input_captured: false,
            input_released_by,
            resize_allowed: false,
            view_only,
            pinned: false,
            source_size: None,
        }
    }

    pub fn into_event(self, release_key: String) -> Event {
        Event::LiveViewStateChanged {
            target: self.target,
            state: self.state,
            input_captured: self.input_captured,
            input_released_by: self.input_released_by,
            resize_allowed: self.resize_allowed,
            view_only: self.view_only,
            pinned: self.pinned,
            source_size: self.source_size,
            release_key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRefusal {
    NoView,
    NoTarget,
    ViewOnly,
    NotFocused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveViewKeyOutcome {
    NotCaptured,
    Sent,
}

pub struct LiveViewSlot {
    pub view: MirrorView,
    pub rect: LiveViewRect,
    pub requested_resize_allowed: Option<bool>,
    pub input_captured: bool,
    pub released_by: Option<LiveViewRelease>,
    pending: MirrorRedraw,
    drawn_at: Option<LiveViewRect>,
    drawn_offset: usize,
    reported: Option<LiveViewReport>,
    force_report: bool,
    links: HashMap<String, LinkAnchor>,
    links_of: Option<usize>,
}

impl LiveViewSlot {
    pub fn new(
        view: MirrorView,
        rect: LiveViewRect,
        requested_resize_allowed: Option<bool>,
    ) -> Self {
        LiveViewSlot {
            view,
            rect,
            requested_resize_allowed,
            input_captured: false,
            released_by: None,
            pending: MirrorRedraw::Full,
            drawn_at: None,
            drawn_offset: 0,
            reported: None,
            force_report: false,
            links: HashMap::new(),
            links_of: None,
        }
    }

    pub fn has_target(&self) -> bool {
        self.view.target().is_some()
    }

    pub fn report(&self) -> LiveViewReport {
        LiveViewReport {
            target: self.view.target().cloned(),
            state: self.view.state(),
            input_captured: self.input_captured,
            input_released_by: self.released_by,
            resize_allowed: self.view.resize_allowed(),
            view_only: self.view.is_view_only(),
            pinned: self.view.is_pinned(),
            source_size: self.view.source_size(),
        }
    }

    pub fn report_if_changed(&mut self) -> Option<LiveViewReport> {
        let report = self.report();
        if !self.force_report && self.reported.as_ref() == Some(&report) {
            return None;
        }
        self.force_report = false;
        self.reported = Some(report.clone());
        Some(report)
    }

    pub fn force_report(&mut self) {
        self.force_report = true;
    }

    pub fn redraw(&mut self, redraw: MirrorRedraw) {
        merge_redraw(&mut self.pending, redraw);
    }

    pub fn release(&mut self, reason: LiveViewRelease) -> bool {
        if !self.input_captured {
            return false;
        }
        self.input_captured = false;
        self.released_by = Some(reason);
        self.view.set_focused(false);
        true
    }

    pub fn capture(&mut self) {
        self.input_captured = true;
        self.released_by = None;
        self.view.set_focused(true);
    }

    pub fn refuse_capture(&mut self) {
        self.released_by = Some(LiveViewRelease::NotAllowed);
        self.force_report = true;
    }

    pub fn area_in(&self, content: &LiveViewRect) -> LiveViewRect {
        self.rect.placed_in(content)
    }

    pub fn draw(
        &mut self,
        content: &LiveViewRect,
        plugin_redrew: bool,
        link_handler: Option<&Rc<RefCell<LinkHandler>>>,
        styled_underlines: bool,
    ) -> Vec<CharacterChunk> {
        let area = self.area_in(content);
        let offset = self
            .view
            .shown_screen()
            .map(|screen| screen_offset(screen, area.rows))
            .unwrap_or(0);
        let moved = self.drawn_at != Some(area) || self.drawn_offset != offset;
        self.drawn_offset = offset;
        let redraw = if plugin_redrew || moved {
            MirrorRedraw::Full
        } else {
            std::mem::replace(&mut self.pending, MirrorRedraw::Nothing)
        };
        self.pending = MirrorRedraw::Nothing;
        self.drawn_at = Some(area);
        if area.is_empty() || redraw == MirrorRedraw::Nothing {
            return vec![];
        }
        let handler_id = link_handler.map(|handler| Rc::as_ptr(handler) as usize);
        if handler_id != self.links_of {
            self.links.clear();
            self.links_of = handler_id;
        }
        let links = &mut self.links;
        let mut link_for = |uri: &str| -> Option<LinkAnchor> {
            let handler = link_handler?;
            if let Some(anchor) = links.get(uri) {
                return Some(*anchor);
            }
            let anchor = handler.borrow_mut().new_link_from_url(uri.to_owned());
            links.insert(uri.to_owned(), anchor);
            Some(anchor)
        };
        match self.view.shown_screen() {
            Some(screen) => screen_chunks_from(
                screen,
                &area,
                &redraw,
                offset,
                &mut link_for,
                styled_underlines,
            ),
            None => placeholder_chunks(&self.view.placeholder_lines(), &area),
        }
    }

    pub fn cursor(&self, content: &LiveViewRect) -> (usize, usize, bool, String) {
        let area = self.area_in(content);
        match self.view.live_screen() {
            Some(screen) => {
                let cursor = screen.cursor;
                let offset = screen_offset(screen, area.rows);
                let x = cursor.x as usize;
                let y = (cursor.y as usize).saturating_sub(offset);
                let inside = x < area.cols && y < area.rows && cursor.y as usize >= offset;
                let visible = cursor.visible && inside;
                (
                    area.x + x.min(area.cols.saturating_sub(1)),
                    area.y + y.min(area.rows.saturating_sub(1)),
                    visible,
                    cursor_shape_csi(cursor.shape).to_owned(),
                )
            },
            None => (area.x, area.y, false, String::new()),
        }
    }

    pub fn send_key(
        &self,
        key: &Option<KeyWithModifier>,
        raw_bytes: &[u8],
        is_kitty_keyboard_protocol: bool,
    ) -> bool {
        self.view
            .send_key(key, raw_bytes.to_vec(), is_kitty_keyboard_protocol)
    }

    pub fn send_paste(&self, bytes: Vec<u8>) -> bool {
        self.view.send_input(MirrorInput::Paste(bytes))
    }

    pub fn send_mouse(&self, event: &MouseEvent) -> bool {
        let is_wheel = event.wheel_up || event.wheel_down;
        if self.view.wants_mouse() {
            let mut shifted = *event;
            shifted.position = zellij_utils::position::Position::new(
                (event.position.line() + self.drawn_offset as isize) as i32,
                event.position.column() as u16,
            );
            return self.view.send_mouse(&shifted);
        }
        if is_wheel && self.view.alternate_screen_active() {
            let arrow: &[u8] = if event.wheel_up { b"\x1b[A" } else { b"\x1b[B" };
            let mut sent = false;
            for _ in 0..3 {
                sent |= self.view.send_input(MirrorInput::Bytes(arrow.to_vec()));
            }
            return sent;
        }
        false
    }
}

pub fn merge_redraw(pending: &mut MirrorRedraw, redraw: MirrorRedraw) {
    match (&mut *pending, redraw) {
        (_, MirrorRedraw::Nothing) | (MirrorRedraw::Full, _) => {},
        (_, MirrorRedraw::Full) => *pending = MirrorRedraw::Full,
        (MirrorRedraw::Nothing, rows) => *pending = rows,
        (MirrorRedraw::Rows(rows), MirrorRedraw::Rows(more)) => rows.extend(more),
    }
}

pub struct LiveViews {
    views: BTreeMap<LiveViewKey, LiveViewSlot>,
    next_view_id: u32,
    pub release_key: LiveViewReleaseKey,
    pub release_key_text: Option<String>,
    pub styled_underlines: bool,
    announced_captures: BTreeMap<ClientId, LiveViewReleaseKey>,
    reported_release_keys: BTreeMap<ClientId, String>,
    mouse_drags: BTreeMap<ClientId, LiveViewDrag>,
    pub captures: SharedLiveViewCaptures,
}

impl Default for LiveViews {
    fn default() -> Self {
        LiveViews {
            views: BTreeMap::new(),
            next_view_id: 0,
            release_key: LiveViewReleaseKey::Bound,
            release_key_text: None,
            styled_underlines: true,
            announced_captures: BTreeMap::new(),
            reported_release_keys: BTreeMap::new(),
            mouse_drags: BTreeMap::new(),
            captures: Arc::new(Mutex::new(LiveViewCaptures::default())),
        }
    }
}

pub fn type_key() -> KeyWithModifier {
    KeyWithModifier::new(BareKey::Enter).with_alt_modifier()
}

pub fn default_release_key() -> LiveViewReleaseGesture {
    LiveViewReleaseGesture::parse(DEFAULT_LIVE_VIEW_RELEASE_KEY)
        .expect("the default live view release key is a key")
}

pub fn parse_release_key(release_key: Option<&str>) -> Option<(LiveViewReleaseGesture, String)> {
    let text = release_key.map(str::trim).filter(|text| !text.is_empty())?;
    match LiveViewReleaseGesture::parse(text) {
        Some(gesture) => Some((gesture, text.to_owned())),
        None => {
            log::error!(
                "The live view release key {:?} is not a key, using the key bound to ToggleSessionCard",
                text
            );
            None
        },
    }
}

pub fn release_key_text_for(bound_keys: Vec<KeyWithModifier>) -> String {
    release_keys_or_default(bound_keys)
        .first()
        .map(|key| key.to_string())
        .unwrap_or_else(|| DEFAULT_LIVE_VIEW_RELEASE_KEY.to_owned())
}

fn is_a_merged_double_press(gesture: &LiveViewReleaseGesture, key: &KeyWithModifier) -> bool {
    gesture.double
        && !gesture.key.key_modifiers.contains(&KeyModifier::Alt)
        && *key == gesture.key.clone().with_alt_modifier()
}

pub fn is_release_press(
    gesture: &LiveViewReleaseGesture,
    key: &KeyWithModifier,
    presses: &mut BTreeMap<ClientId, std::time::Instant>,
    client_id: ClientId,
) -> bool {
    if is_a_merged_double_press(gesture, key) {
        presses.remove(&client_id);
        return true;
    }
    if gesture.key != *key {
        presses.remove(&client_id);
        return false;
    }
    if !gesture.double {
        return true;
    }
    let now = std::time::Instant::now();
    let window = std::time::Duration::from_millis(LIVE_VIEW_DOUBLE_PRESS_MS);
    match presses.remove(&client_id) {
        Some(previous) if now.saturating_duration_since(previous) <= window => true,
        _ => {
            presses.insert(client_id, now);
            false
        },
    }
}

impl LiveViews {
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    pub fn allocate_view_id(&mut self) -> u32 {
        let view_id = self.next_view_id;
        self.next_view_id = self.next_view_id.wrapping_add(1);
        view_id
    }

    pub fn keys(&self) -> Vec<LiveViewKey> {
        self.views.keys().copied().collect()
    }

    pub fn get(&self, key: &LiveViewKey) -> Option<&LiveViewSlot> {
        self.views.get(key)
    }

    pub fn get_mut(&mut self, key: &LiveViewKey) -> Option<&mut LiveViewSlot> {
        self.views.get_mut(key)
    }

    pub fn insert(&mut self, key: LiveViewKey, slot: LiveViewSlot) -> Option<LiveViewSlot> {
        self.views.insert(key, slot)
    }

    pub fn remove(&mut self, key: &LiveViewKey) -> Option<LiveViewSlot> {
        self.cut_drag_of_view(key);
        self.views.remove(key)
    }

    pub fn drag_of(&self, client_id: ClientId) -> Option<LiveViewDrag> {
        self.mouse_drags.get(&client_id).copied()
    }

    pub fn start_drag(&mut self, key: LiveViewKey, button: DragButton, position: Position) {
        self.mouse_drags.insert(
            key.1,
            LiveViewDrag {
                key,
                button,
                position,
            },
        );
    }

    pub fn drag_moved(&mut self, client_id: ClientId, position: Position) {
        if let Some(drag) = self.mouse_drags.get_mut(&client_id) {
            drag.position = position;
        }
    }

    pub fn end_drag(&mut self, client_id: ClientId) {
        self.mouse_drags.remove(&client_id);
    }

    pub fn cut_drag(&mut self, client_id: ClientId) -> bool {
        let Some(drag) = self.mouse_drags.remove(&client_id) else {
            return false;
        };
        if let Some(slot) = self.views.get(&drag.key) {
            slot.send_mouse(&drag.button.release_at(drag.position));
        }
        true
    }

    pub fn cut_drag_of_view(&mut self, key: &LiveViewKey) -> bool {
        match self.mouse_drags.get(&key.1) {
            Some(drag) if drag.key == *key => self.cut_drag(key.1),
            _ => false,
        }
    }

    pub fn values(&self) -> impl Iterator<Item = &LiveViewSlot> {
        self.views.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&LiveViewKey, &mut LiveViewSlot)> {
        self.views.iter_mut()
    }

    pub fn key_of_view(&self, view_id: u32) -> Option<LiveViewKey> {
        self.views
            .iter()
            .find(|(_, slot)| slot.view.view_id() == view_id)
            .map(|(key, _)| *key)
    }

    pub fn keys_of_plugin(&self, plugin_pane_id: u32) -> Vec<LiveViewKey> {
        self.views
            .keys()
            .filter(|(pane, _)| *pane == plugin_pane_id)
            .copied()
            .collect()
    }

    pub fn keys_of_client(&self, client_id: ClientId) -> Vec<LiveViewKey> {
        self.views
            .keys()
            .filter(|(_, client)| *client == client_id)
            .copied()
            .collect()
    }

    pub fn captured_by(&self, client_id: ClientId) -> Option<LiveViewKey> {
        self.views
            .iter()
            .find(|((_, client), slot)| *client == client_id && slot.input_captured)
            .map(|(key, _)| *key)
    }

    pub fn is_captured(&self, plugin_pane_id: u32, client_id: ClientId) -> bool {
        self.views
            .get(&(plugin_pane_id, client_id))
            .map(|slot| slot.input_captured)
            .unwrap_or(false)
    }

    pub fn start_capture(
        &mut self,
        key: LiveViewKey,
        has_focus: bool,
    ) -> Result<(), CaptureRefusal> {
        let refusal = match self.views.get(&key) {
            None => Some(CaptureRefusal::NoView),
            Some(slot) if !slot.has_target() => Some(CaptureRefusal::NoTarget),
            Some(slot) if slot.view.is_view_only() => Some(CaptureRefusal::ViewOnly),
            Some(_) if !has_focus => Some(CaptureRefusal::NotFocused),
            Some(_) => None,
        };
        if let Some(refusal) = refusal {
            if let Some(slot) = self.views.get_mut(&key) {
                slot.refuse_capture();
            }
            return Err(refusal);
        }
        let (_, client_id) = key;
        for other in self.keys_of_client(client_id) {
            if other != key {
                self.cut_drag_of_view(&other);
                if let Some(slot) = self.views.get_mut(&other) {
                    slot.release(LiveViewRelease::FocusLost);
                }
            }
        }
        if let Some(slot) = self.views.get_mut(&key) {
            if !slot.input_captured {
                slot.capture();
            }
        }
        Ok(())
    }

    pub fn release(&mut self, key: &LiveViewKey, reason: LiveViewRelease) -> bool {
        self.cut_drag_of_view(key);
        if let Ok(mut captures) = self.captures.lock() {
            captures.forget_presses(key.1);
        }
        self.views
            .get_mut(key)
            .map(|slot| slot.release(reason))
            .unwrap_or(false)
    }

    pub fn release_client(&mut self, client_id: ClientId, reason: LiveViewRelease) -> bool {
        match self.captured_by(client_id) {
            Some(key) => self.release(&key, reason),
            None => false,
        }
    }

    pub fn send_key(
        &mut self,
        client_id: ClientId,
        key: &Option<KeyWithModifier>,
        raw_bytes: &[u8],
        is_kitty_keyboard_protocol: bool,
    ) -> LiveViewKeyOutcome {
        let Some(view_key) = self.captured_by(client_id) else {
            return LiveViewKeyOutcome::NotCaptured;
        };
        if let Some(slot) = self.views.get(&view_key) {
            slot.send_key(key, raw_bytes, is_kitty_keyboard_protocol);
        }
        LiveViewKeyOutcome::Sent
    }

    pub fn send_paste(&mut self, client_id: ClientId, bytes: Vec<u8>) -> bool {
        let Some(view_key) = self.captured_by(client_id) else {
            return false;
        };
        if let Some(slot) = self.views.get(&view_key) {
            slot.send_paste(bytes);
        }
        true
    }

    pub fn draw(
        &mut self,
        key: &LiveViewKey,
        content: &LiveViewRect,
        plugin_redrew: bool,
        link_handler: Option<&Rc<RefCell<LinkHandler>>>,
    ) -> Vec<CharacterChunk> {
        let styled_underlines = self.styled_underlines;
        match self.views.get_mut(key) {
            Some(slot) => slot.draw(content, plugin_redrew, link_handler, styled_underlines),
            None => vec![],
        }
    }

    pub fn cursor(
        &self,
        client_id: ClientId,
        plugin_pane_id: u32,
        content: &LiveViewRect,
    ) -> Option<(usize, usize, bool, String)> {
        self.views
            .get(&(plugin_pane_id, client_id))
            .filter(|slot| slot.input_captured)
            .map(|slot| slot.cursor(content))
    }

    pub fn announce_captures(&mut self) {
        let changes = self.capture_changes();
        if changes.is_empty() {
            return;
        }
        if let Ok(mut captures) = self.captures.lock() {
            for (client_id, gesture) in changes {
                captures.set(client_id, gesture);
            }
        }
    }

    pub fn capture_changes(&mut self) -> Vec<(ClientId, Option<LiveViewReleaseKey>)> {
        let current: BTreeSet<ClientId> = self
            .views
            .iter()
            .filter(|(_, slot)| slot.input_captured)
            .map(|((_, client_id), _)| *client_id)
            .collect();
        let mut changes = vec![];
        let announced: Vec<ClientId> = self.announced_captures.keys().copied().collect();
        for client_id in announced {
            if !current.contains(&client_id) {
                self.announced_captures.remove(&client_id);
                changes.push((client_id, None));
            }
        }
        for client_id in current {
            if self.announced_captures.get(&client_id) != Some(&self.release_key) {
                self.announced_captures
                    .insert(client_id, self.release_key.clone());
                changes.push((client_id, Some(self.release_key.clone())));
            }
        }
        changes
    }

    pub fn set_release_key(&mut self, configured: Option<(LiveViewReleaseGesture, String)>) {
        match configured {
            Some((gesture, text)) => {
                self.release_key = LiveViewReleaseKey::Configured(gesture);
                self.release_key_text = Some(text);
            },
            None => {
                self.release_key = LiveViewReleaseKey::Bound;
                self.release_key_text = None;
            },
        }
    }

    pub fn clients(&self) -> BTreeSet<ClientId> {
        self.views.keys().map(|(_, client_id)| *client_id).collect()
    }

    pub fn note_release_keys(&mut self, release_keys: BTreeMap<ClientId, String>) {
        for ((_, client_id), slot) in self.views.iter_mut() {
            if self.reported_release_keys.get(client_id) != release_keys.get(client_id) {
                slot.force_report();
            }
        }
        self.reported_release_keys = release_keys;
    }
}

#[cfg(test)]
#[path = "./unit/live_view_tests.rs"]
mod live_view_tests;
