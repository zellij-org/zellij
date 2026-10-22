use super::Screen;
use crate::background_jobs::BackgroundJob;
use crate::pane_mirror::{MirrorOutbox, MirrorSubscriberId};
use crate::panes::PaneId;
use crate::tab::Tab;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use zellij_utils::data::MirrorClosedReason;
use zellij_utils::input::mouse::MouseEventType;
use zellij_utils::pane_mirror::{
    capabilities, clamp_mirror_size, mirror_pin_lease, MirrorAttach, MirrorFramePacer, MirrorInput,
    MirrorScreen, SourceMessage, PANE_MIRROR_VERSION,
};
use zellij_utils::pane_size::Size;

const MIRROR_SEEN_AFTER: Duration = Duration::from_millis(400);

#[derive(Debug)]
pub(crate) struct MirrorSubscriber {
    pane_id: PaneId,
    view_only: bool,
    resize_allowed: bool,
    focused: bool,
    yielded: bool,
    viewer_size: Size,
    outbox: MirrorOutbox,
    pacer: MirrorFramePacer,
    pin_order: u64,
    lease_until: Option<Instant>,
    reported_pin: Option<(bool, Size)>,
    parked: bool,
    kept: Option<Size>,
}

impl MirrorSubscriber {
    fn asks_for_pin(&self) -> bool {
        self.resize_allowed && !self.view_only && !self.yielded
    }
    fn holds_lease(&self, now: Instant) -> bool {
        self.asks_for_pin()
            && self
                .lease_until
                .map(|lease_until| lease_until > now)
                .unwrap_or(false)
    }
}

#[derive(Debug)]
pub(crate) struct PaneMirrors {
    subscribers: HashMap<MirrorSubscriberId, MirrorSubscriber>,
    next_pin_order: u64,
    pinned_panes: HashMap<PaneId, Size>,
    pub(crate) resizes_source_pane: bool,
    tick_at: Option<Instant>,
    attached_at: HashMap<MirrorSubscriberId, Instant>,
    viewed_panes: HashSet<PaneId>,
    focused_panes: HashSet<PaneId>,
}

impl Default for PaneMirrors {
    fn default() -> Self {
        PaneMirrors {
            subscribers: HashMap::new(),
            next_pin_order: 0,
            pinned_panes: HashMap::new(),
            resizes_source_pane: true,
            tick_at: None,
            attached_at: HashMap::new(),
            viewed_panes: HashSet::new(),
            focused_panes: HashSet::new(),
        }
    }
}

impl PaneMirrors {
    fn update_wish(
        &mut self,
        subscriber_id: MirrorSubscriberId,
        size: Size,
        focused: bool,
        resize_allowed: bool,
        now: Instant,
    ) {
        let next_pin_order = &mut self.next_pin_order;
        let Some(subscriber) = self.subscribers.get_mut(&subscriber_id) else {
            return;
        };
        if subscriber.parked {
            return;
        }
        let held_lease = subscriber.holds_lease(now);
        let claimed = focused && !subscriber.focused;
        if subscriber.resize_allowed && !resize_allowed {
            subscriber.kept = None;
        }
        subscriber.viewer_size = clamp_mirror_size(size);
        subscriber.focused = focused;
        subscriber.resize_allowed = resize_allowed;
        if claimed {
            subscriber.yielded = false;
        }
        if subscriber.asks_for_pin() {
            if !held_lease || claimed {
                *next_pin_order += 1;
                subscriber.pin_order = *next_pin_order;
            }
            subscriber.lease_until = Some(now + mirror_pin_lease());
        } else {
            subscriber.lease_until = None;
        }
    }

    fn reclaim(&mut self, subscriber_id: MirrorSubscriberId, now: Instant) -> bool {
        let next_pin_order = &mut self.next_pin_order;
        let Some(subscriber) = self.subscribers.get_mut(&subscriber_id) else {
            return false;
        };
        if !subscriber.yielded {
            return false;
        }
        subscriber.yielded = false;
        if !subscriber.asks_for_pin() {
            return false;
        }
        *next_pin_order += 1;
        subscriber.pin_order = *next_pin_order;
        subscriber.lease_until = Some(now + mirror_pin_lease());
        true
    }

    pub(crate) fn yield_panes(&mut self, pane_ids: &HashSet<PaneId>) -> bool {
        let mut yielded_any = false;
        for subscriber in self.subscribers.values_mut() {
            if !pane_ids.contains(&subscriber.pane_id)
                || !self.pinned_panes.contains_key(&subscriber.pane_id)
                || subscriber.yielded
            {
                continue;
            }
            subscriber.yielded = true;
            subscriber.lease_until = None;
            yielded_any = true;
        }
        yielded_any
    }

    pub(crate) fn forget_kept_sizes<'a>(&mut self, pane_ids: impl Iterator<Item = &'a PaneId>) {
        let pane_ids: HashSet<PaneId> = pane_ids.copied().collect();
        for subscriber in self.subscribers.values_mut() {
            if pane_ids.contains(&subscriber.pane_id) {
                subscriber.kept = None;
            }
        }
    }

    pub(crate) fn forget_pane(&mut self, pane_id: PaneId) {
        self.pinned_panes.remove(&pane_id);
        self.viewed_panes.remove(&pane_id);
        self.forget_kept_sizes(std::iter::once(&pane_id));
    }

    fn park(&mut self, subscriber_id: MirrorSubscriberId) -> bool {
        let Some(subscriber) = self.subscribers.get_mut(&subscriber_id) else {
            return false;
        };
        if subscriber.parked {
            return false;
        }
        subscriber.parked = true;
        subscriber.focused = false;
        subscriber.resize_allowed = false;
        subscriber.lease_until = None;
        self.attached_at.remove(&subscriber_id);
        true
    }

    fn unpark(&mut self, subscriber_id: MirrorSubscriberId, resize_allowed: bool, now: Instant) -> bool {
        let Some(subscriber) = self.subscribers.get_mut(&subscriber_id) else {
            return false;
        };
        if !subscriber.parked {
            return false;
        }
        subscriber.parked = false;
        if !resize_allowed {
            subscriber.kept = None;
        }
        subscriber.reported_pin = None;
        subscriber.pacer.request_full_repaint();
        self.attached_at.insert(subscriber_id, now);
        true
    }

    fn kept_claims(&self) -> HashMap<PaneId, Size> {
        let mut claims: HashMap<PaneId, (u64, MirrorSubscriberId, Size)> = HashMap::new();
        for (subscriber_id, subscriber) in &self.subscribers {
            let Some(size) = subscriber.kept else {
                continue;
            };
            let candidate = (subscriber.pin_order, *subscriber_id, size);
            claims
                .entry(subscriber.pane_id)
                .and_modify(|current| {
                    if (candidate.0, candidate.1) > (current.0, current.1) {
                        *current = candidate;
                    }
                })
                .or_insert(candidate);
        }
        claims
            .into_iter()
            .map(|(pane_id, (_, _, size))| (pane_id, size))
            .collect()
    }

    fn next_lease_expiry(&self, now: Instant) -> Option<Instant> {
        self.subscribers
            .values()
            .filter(|subscriber| subscriber.holds_lease(now))
            .filter_map(|subscriber| subscriber.lease_until)
            .min()
    }

    fn forget_subscribers(&mut self, removals: Vec<(MirrorSubscriberId, MirrorRemoval)>) -> bool {
        let mut removed_any = false;
        for (subscriber_id, removal) in removals {
            self.attached_at.remove(&subscriber_id);
            let Some(subscriber) = self.subscribers.remove(&subscriber_id) else {
                continue;
            };
            removed_any = true;
            match removal {
                MirrorRemoval::Finished => subscriber.outbox.finish(),
                MirrorRemoval::Closed(reason) => subscriber.outbox.close(reason),
                MirrorRemoval::Unreachable => {},
            }
        }
        removed_any
    }

    pub(crate) fn settle_pins(&mut self, host: &mut dyn PinHost, now: Instant) -> PinsSettled {
        let mut settled = PinsSettled::default();
        loop {
            let winners = choose_pins(&self.subscribers, self.resizes_source_pane, now);
            if !self.resizes_source_pane {
                for subscriber in self.subscribers.values_mut() {
                    subscriber.kept = None;
                }
            }
            for (subscriber_id, size) in winners.values() {
                if let Some(subscriber) = self.subscribers.get_mut(subscriber_id) {
                    subscriber.kept = Some(*size);
                }
            }
            let kept = self.kept_claims();
            let mut panes: HashSet<PaneId> = self.pinned_panes.keys().copied().collect();
            panes.extend(winners.keys().copied());
            panes.extend(kept.keys().copied());
            for pane_id in panes {
                let wanted = winners
                    .get(&pane_id)
                    .map(|(_, size)| *size)
                    .or_else(|| kept.get(&pane_id).copied());
                if self.pinned_panes.get(&pane_id).copied() == wanted {
                    continue;
                }
                match (host.set_pinned_size(pane_id, wanted), wanted) {
                    (Some(applied), Some(size)) => {
                        settled.changed |= applied;
                        self.pinned_panes.insert(pane_id, size);
                    },
                    (Some(applied), None) => {
                        settled.changed |= applied;
                        self.pinned_panes.remove(&pane_id);
                    },
                    (None, _) => {
                        self.pinned_panes.remove(&pane_id);
                    },
                }
            }
            let mut unreachable = vec![];
            for (subscriber_id, subscriber) in self.subscribers.iter_mut() {
                if subscriber.parked {
                    continue;
                }
                let Some(pane_size) = host.program_size(subscriber.pane_id) else {
                    continue;
                };
                let pinned = winners
                    .get(&subscriber.pane_id)
                    .map(|(winner, _)| winner == subscriber_id)
                    .unwrap_or(false);
                if subscriber.reported_pin == Some((pinned, pane_size)) {
                    continue;
                }
                subscriber.reported_pin = Some((pinned, pane_size));
                if !subscriber
                    .outbox
                    .send(SourceMessage::PinState { pinned, pane_size })
                {
                    unreachable.push((*subscriber_id, MirrorRemoval::Unreachable));
                }
            }
            if !self.forget_subscribers(unreachable) {
                return settled;
            }
            settled.removed_subscribers = true;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MirrorRemoval {
    Finished,
    Closed(MirrorClosedReason),
    Unreachable,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PinsSettled {
    pub changed: bool,
    pub removed_subscribers: bool,
}

pub(crate) trait PinHost {
    fn program_size(&self, pane_id: PaneId) -> Option<Size>;
    fn set_pinned_size(&mut self, pane_id: PaneId, size: Option<Size>) -> Option<bool>;
}

impl PinHost for BTreeMap<usize, Tab> {
    fn program_size(&self, pane_id: PaneId) -> Option<Size> {
        program_size_of(self, pane_id)
    }
    fn set_pinned_size(&mut self, pane_id: PaneId, size: Option<Size>) -> Option<bool> {
        self.values_mut()
            .find(|tab| tab.has_pane_with_pid(&pane_id))
            .map(|tab| tab.set_mirror_pinned_size(pane_id, size))
    }
}

pub(crate) fn choose_pins(
    subscribers: &HashMap<MirrorSubscriberId, MirrorSubscriber>,
    source_allows_pins: bool,
    now: Instant,
) -> HashMap<PaneId, (MirrorSubscriberId, Size)> {
    let mut winners: HashMap<PaneId, (u64, MirrorSubscriberId, Size)> = HashMap::new();
    if !source_allows_pins {
        return HashMap::new();
    }
    for (subscriber_id, subscriber) in subscribers {
        if subscriber.parked || !subscriber.holds_lease(now) {
            continue;
        }
        let candidate = (subscriber.pin_order, *subscriber_id, subscriber.viewer_size);
        winners
            .entry(subscriber.pane_id)
            .and_modify(|current| {
                if (candidate.0, candidate.1) > (current.0, current.1) {
                    *current = candidate;
                }
            })
            .or_insert(candidate);
    }
    winners
        .into_iter()
        .map(|(pane_id, (_, subscriber_id, size))| (pane_id, (subscriber_id, size)))
        .collect()
}

fn find_pane<'a>(
    tabs: &'a BTreeMap<usize, Tab>,
    pane_id: PaneId,
) -> Option<&'a dyn crate::tab::Pane> {
    tabs.values().find_map(|tab| tab.get_pane_with_id(pane_id))
}

pub(crate) fn guest_is_a_viewer(guest: Option<String>, viewer_sessions: &[String]) -> bool {
    guest
        .map(|guest| viewer_sessions.iter().any(|viewer| *viewer == guest))
        .unwrap_or(false)
}

fn pane_contains_viewer(
    tabs: &BTreeMap<usize, Tab>,
    pane_id: PaneId,
    viewer_sessions: &[String],
) -> bool {
    guest_is_a_viewer(
        find_pane(tabs, pane_id).and_then(|pane| pane.guest_session_name()),
        viewer_sessions,
    )
}

fn pane_exists(tabs: &BTreeMap<usize, Tab>, pane_id: PaneId) -> bool {
    find_pane(tabs, pane_id).is_some()
}

fn mirror_screen_of(tabs: &mut BTreeMap<usize, Tab>, pane_id: PaneId) -> Option<MirrorScreen> {
    tabs.values_mut()
        .find_map(|tab| {
            tab.get_pane_with_id_mut(pane_id)
                .map(|pane| pane.mirror_screen())
        })
        .flatten()
}

fn change_counter_of(tabs: &BTreeMap<usize, Tab>, pane_id: PaneId) -> Option<u64> {
    find_pane(tabs, pane_id).map(|pane| pane.mirror_change_counter())
}

fn program_size_of(tabs: &BTreeMap<usize, Tab>, pane_id: PaneId) -> Option<Size> {
    find_pane(tabs, pane_id).map(|pane| Size {
        cols: pane.program_columns(),
        rows: pane.program_rows(),
    })
}

fn send_frame(subscriber: &mut MirrorSubscriber, change: u64, screen: &MirrorScreen) -> bool {
    match subscriber.pacer.next_frame(change, screen) {
        Some(frame) => subscriber.outbox.send(SourceMessage::Frame {
            frame: frame.bytes,
            alternate_screen: frame.alternate_screen,
        }),
        None => true,
    }
}

impl Screen {
    pub(crate) fn mirror_attach(
        &mut self,
        subscriber_id: MirrorSubscriberId,
        attach: MirrorAttach,
        outbox: MirrorOutbox,
    ) {
        let pane_id: PaneId = attach.target_pane_id.into();
        if attach.version != PANE_MIRROR_VERSION {
            outbox.close(MirrorClosedReason::VersionMismatch);
            return;
        }
        if matches!(pane_id, PaneId::Plugin(_)) {
            outbox.close(MirrorClosedReason::Rejected);
            return;
        }
        let Some(pane_size) = program_size_of(&self.tabs, pane_id) else {
            outbox.close(MirrorClosedReason::PaneNotFound);
            return;
        };
        if pane_contains_viewer(&self.tabs, pane_id, &attach.viewer_sessions) {
            outbox.close(MirrorClosedReason::ContainsViewer);
            return;
        }
        let Some(change) = change_counter_of(&self.tabs, pane_id) else {
            outbox.close(MirrorClosedReason::PaneNotFound);
            return;
        };
        let Some(snapshot) = mirror_screen_of(&mut self.tabs, pane_id) else {
            outbox.close(MirrorClosedReason::Rejected);
            return;
        };
        if !outbox.send(SourceMessage::Attached {
            version: PANE_MIRROR_VERSION,
            capabilities: capabilities(),
            pane_size,
        }) {
            return;
        }
        let mut subscriber = MirrorSubscriber {
            pane_id,
            view_only: attach.view_only,
            resize_allowed: false,
            focused: false,
            yielded: false,
            viewer_size: clamp_mirror_size(attach.viewer_size),
            outbox,
            pacer: MirrorFramePacer::default(),
            pin_order: 0,
            lease_until: None,
            reported_pin: None,
            parked: false,
            kept: None,
        };
        if !send_frame(&mut subscriber, change, &snapshot) {
            return;
        }
        self.pane_mirrors
            .subscribers
            .insert(subscriber_id, subscriber);
        self.pane_mirrors
            .attached_at
            .insert(subscriber_id, Instant::now());
        self.pane_mirrors.update_wish(
            subscriber_id,
            attach.viewer_size,
            attach.focused,
            attach.resize_allowed,
            Instant::now(),
        );
        self.apply_mirror_pins_and_render();
    }

    pub(crate) fn mirror_detach(&mut self, subscriber_id: MirrorSubscriberId) {
        self.remove_mirror_subscribers(vec![(subscriber_id, MirrorRemoval::Finished)]);
    }

    pub(crate) fn mirror_park(&mut self, subscriber_id: MirrorSubscriberId) {
        if self.pane_mirrors.park(subscriber_id) {
            self.apply_mirror_pins_and_render();
        }
    }

    pub(crate) fn mirror_unpark(&mut self, subscriber_id: MirrorSubscriberId, resize_allowed: bool) {
        if !self
            .pane_mirrors
            .unpark(subscriber_id, resize_allowed, Instant::now())
        {
            return;
        }
        let pane_id = match self.pane_mirrors.subscribers.get(&subscriber_id) {
            Some(subscriber) => subscriber.pane_id,
            None => return,
        };
        let change = change_counter_of(&self.tabs, pane_id);
        let snapshot = mirror_screen_of(&mut self.tabs, pane_id);
        let alive = match (change, snapshot) {
            (Some(change), Some(snapshot)) => self
                .pane_mirrors
                .subscribers
                .get_mut(&subscriber_id)
                .map(|subscriber| send_frame(subscriber, change, &snapshot))
                .unwrap_or(true),
            _ => {
                self.remove_mirror_subscribers(vec![(
                    subscriber_id,
                    MirrorRemoval::Closed(MirrorClosedReason::PaneClosed),
                )]);
                return;
            },
        };
        if !alive {
            self.remove_mirror_subscribers(vec![(subscriber_id, MirrorRemoval::Unreachable)]);
            return;
        }
        self.apply_mirror_pins_and_render();
    }

    fn remove_mirror_subscribers(&mut self, removals: Vec<(MirrorSubscriberId, MirrorRemoval)>) {
        if self.pane_mirrors.forget_subscribers(removals) {
            self.apply_mirror_pins_and_render();
        }
    }

    pub(crate) fn mirror_resize(
        &mut self,
        subscriber_id: MirrorSubscriberId,
        size: Size,
        focused: bool,
        resize_allowed: bool,
    ) {
        let parked = match self.pane_mirrors.subscribers.get(&subscriber_id) {
            Some(subscriber) => subscriber.parked,
            None => return,
        };
        if parked {
            return;
        }
        self.pane_mirrors
            .update_wish(subscriber_id, size, focused, resize_allowed, Instant::now());
        self.apply_mirror_pins_and_render();
    }

    pub(crate) fn mirror_frame_ack(&mut self, subscriber_id: MirrorSubscriberId, seq: u64) {
        let Some(subscriber) = self.pane_mirrors.subscribers.get_mut(&subscriber_id) else {
            return;
        };
        if !subscriber.pacer.acknowledge(seq) || subscriber.parked {
            return;
        }
        let pane_id = subscriber.pane_id;
        let Some(change) = change_counter_of(&self.tabs, pane_id) else {
            return;
        };
        let Some(snapshot) = mirror_screen_of(&mut self.tabs, pane_id) else {
            return;
        };
        let alive = self
            .pane_mirrors
            .subscribers
            .get_mut(&subscriber_id)
            .map(|subscriber| send_frame(subscriber, change, &snapshot))
            .unwrap_or(true);
        if !alive {
            self.remove_mirror_subscribers(vec![(subscriber_id, MirrorRemoval::Unreachable)]);
        }
    }

    pub(crate) fn mirror_input(&mut self, subscriber_id: MirrorSubscriberId, input: MirrorInput) {
        let Some(subscriber) = self.pane_mirrors.subscribers.get(&subscriber_id) else {
            return;
        };
        if subscriber.view_only || subscriber.parked {
            return;
        }
        let pane_id = subscriber.pane_id;
        let Some(tab) = self
            .tabs
            .values_mut()
            .find(|tab| tab.has_pane_with_pid(&pane_id))
        else {
            return;
        };
        let typed = !matches!(input, MirrorInput::Mouse(_));
        let claims = match &input {
            MirrorInput::Mouse(event) => event.event_type == MouseEventType::Press,
            _ => true,
        };
        let result = match input {
            MirrorInput::Key {
                key,
                raw_bytes,
                is_kitty_keyboard_protocol,
            } => tab
                .write_to_pane_id(
                    &Some(key),
                    raw_bytes,
                    is_kitty_keyboard_protocol,
                    pane_id,
                    None,
                    None,
                )
                .map(|_| ()),
            MirrorInput::Bytes(bytes) => tab
                .write_to_pane_id(&None, bytes, false, pane_id, None, None)
                .map(|_| ()),
            MirrorInput::Paste(bytes) => tab.paste_to_pane_id(bytes, pane_id, None),
            MirrorInput::Mouse(event) => tab.send_mouse_event_to_pane_id(pane_id, &event),
        };
        match result {
            Ok(()) if typed => tab.note_typed_input(pane_id),
            Ok(()) => {},
            Err(e) => log::error!("Failed to write mirror input to {:?}: {}", pane_id, e),
        }
        if claims && self.pane_mirrors.reclaim(subscriber_id, Instant::now()) {
            self.apply_mirror_pins_and_render();
        }
    }

    pub(crate) fn deliver_to_mirrors(&mut self) {
        if self.pane_mirrors.subscribers.is_empty() {
            return;
        }
        let mut snapshots: HashMap<PaneId, Option<MirrorScreen>> = HashMap::new();
        let mut removals = vec![];
        let gone = MirrorRemoval::Closed(MirrorClosedReason::PaneClosed);
        for (subscriber_id, subscriber) in self.pane_mirrors.subscribers.iter_mut() {
            let Some(change) = change_counter_of(&self.tabs, subscriber.pane_id) else {
                removals.push((*subscriber_id, gone));
                continue;
            };
            if subscriber.parked || !subscriber.pacer.wants_frame(change) {
                continue;
            }
            let snapshot = snapshots
                .entry(subscriber.pane_id)
                .or_insert_with(|| mirror_screen_of(&mut self.tabs, subscriber.pane_id));
            match snapshot.as_ref() {
                Some(snapshot) => {
                    if !send_frame(subscriber, change, snapshot) {
                        removals.push((*subscriber_id, MirrorRemoval::Unreachable));
                    }
                },
                None => removals.push((*subscriber_id, gone)),
            }
        }
        self.remove_mirror_subscribers(removals);
    }

    pub(crate) fn notify_mirrors_pane_closed(&mut self, pane_id: PaneId) {
        if pane_exists(&self.tabs, pane_id) {
            return;
        }
        let closed: Vec<(MirrorSubscriberId, MirrorRemoval)> = self
            .pane_mirrors
            .subscribers
            .iter()
            .filter(|(_, subscriber)| subscriber.pane_id == pane_id)
            .map(|(subscriber_id, _)| {
                (
                    *subscriber_id,
                    MirrorRemoval::Closed(MirrorClosedReason::PaneClosed),
                )
            })
            .collect();
        self.pane_mirrors.forget_pane(pane_id);
        if self.pane_mirrors.forget_subscribers(closed) {
            self.schedule_mirror_tick();
        }
    }

    pub(crate) fn close_all_mirror_subscribers(&mut self, reason: MirrorClosedReason) {
        for (_, subscriber) in self.pane_mirrors.subscribers.drain() {
            subscriber.outbox.close(reason);
        }
        self.pane_mirrors.attached_at.clear();
        self.pane_mirrors.viewed_panes.clear();
    }

    pub(crate) fn set_mirror_resizes_source_pane(&mut self, resizes_source_pane: bool) {
        if self.pane_mirrors.resizes_source_pane == resizes_source_pane {
            return;
        }
        self.pane_mirrors.resizes_source_pane = resizes_source_pane;
        self.apply_mirror_pins_and_render();
    }

    fn apply_mirror_pins_and_render(&mut self) {
        if self.apply_mirror_pins() {
            let _ = self.render(None);
        }
        self.sync_viewed_panes(Instant::now());
        self.schedule_mirror_tick();
    }

    fn viewed_panes_at(&self, now: Instant) -> HashSet<PaneId> {
        self.pane_mirrors
            .subscribers
            .iter()
            .filter(|(subscriber_id, _)| {
                self.pane_mirrors
                    .attached_at
                    .get(subscriber_id)
                    .map(|at| now.saturating_duration_since(*at) >= MIRROR_SEEN_AFTER)
                    .unwrap_or(false)
            })
            .map(|(_, subscriber)| subscriber.pane_id)
            .collect()
    }

    fn next_seen_deadline(&self, now: Instant) -> Option<Instant> {
        if self.viewed_panes_at(now) != self.pane_mirrors.viewed_panes {
            return Some(now);
        }
        self.pane_mirrors
            .attached_at
            .values()
            .map(|at| *at + MIRROR_SEEN_AFTER)
            .filter(|deadline| *deadline > now)
            .min()
    }

    pub(crate) fn sync_viewed_panes(&mut self, now: Instant) {
        let viewed = self.viewed_panes_at(now);
        if viewed == self.pane_mirrors.viewed_panes {
            return;
        }
        let at = crate::attention::now_ms();
        let started: Vec<PaneId> = viewed
            .difference(&self.pane_mirrors.viewed_panes)
            .copied()
            .collect();
        let ended: Vec<PaneId> = self
            .pane_mirrors
            .viewed_panes
            .difference(&viewed)
            .copied()
            .collect();
        for pane_id in started {
            self.attention
                .set_viewed_elsewhere(pane_id.into(), true, at);
        }
        for pane_id in ended {
            self.attention
                .set_viewed_elsewhere(pane_id.into(), false, at);
        }
        self.pane_mirrors.viewed_panes = viewed;
        self.flush_attention(true);
    }

    pub(crate) fn apply_mirror_pins(&mut self) -> bool {
        let local_input: HashSet<PaneId> = self
            .tabs
            .values_mut()
            .flat_map(|tab| tab.take_local_input())
            .collect();
        let focused: HashSet<PaneId> = self
            .active_tab_ids
            .iter()
            .filter_map(|(client_id, tab_index)| {
                self.tabs
                    .get(tab_index)
                    .and_then(|tab| tab.get_active_pane_id(*client_id))
            })
            .collect();
        let newly_focused: Vec<PaneId> = focused
            .difference(&self.pane_mirrors.focused_panes)
            .copied()
            .collect();
        self.pane_mirrors.focused_panes = focused;
        if self.pane_mirrors.subscribers.is_empty() && self.pane_mirrors.pinned_panes.is_empty() {
            return false;
        }
        self.pane_mirrors
            .forget_kept_sizes(local_input.iter().chain(newly_focused.iter()));
        self.pane_mirrors.yield_panes(&local_input);
        let settled = self
            .pane_mirrors
            .settle_pins(&mut self.tabs, Instant::now());
        if settled.removed_subscribers {
            self.schedule_mirror_tick();
        }
        settled.changed
    }

    fn next_mirror_deadline(&self, now: Instant) -> Option<Instant> {
        let view_deadline = self.next_live_view_deadline();
        let lease_deadline = self.pane_mirrors.next_lease_expiry(now);
        let seen_deadline = self.next_seen_deadline(now);
        [view_deadline, lease_deadline, seen_deadline]
            .into_iter()
            .flatten()
            .min()
    }

    pub(crate) fn schedule_mirror_tick(&mut self) {
        let now = Instant::now();
        let Some(deadline) = self.next_mirror_deadline(now) else {
            return;
        };
        if let Some(tick_at) = self.pane_mirrors.tick_at {
            if tick_at <= deadline {
                return;
            }
        }
        self.pane_mirrors.tick_at = Some(deadline);
        let delay = deadline.saturating_duration_since(now) + Duration::from_millis(1);
        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::MirrorTickAfter {
                delay_ms: delay.as_millis().min(u64::MAX as u128) as u64,
            });
    }

    pub(crate) fn mirror_tick(&mut self) {
        let now = Instant::now();
        if self
            .pane_mirrors
            .tick_at
            .map(|tick_at| tick_at <= now)
            .unwrap_or(true)
        {
            self.pane_mirrors.tick_at = None;
        }
        let redraw = self.tick_live_views(now);
        if self.apply_mirror_pins() || redraw {
            let _ = self.render(None);
        }
        self.sync_viewed_panes(now);
        self.schedule_mirror_tick();
    }
}

#[cfg(test)]
#[path = "./unit/mirror_tests.rs"]
mod mirror_tests;
