use crate::panes::PaneId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const PING_INTERVAL_MS: u64 = 2000;
pub const PONG_TIMEOUT_MS: u64 = 5000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestPingTickAction {
    SendPing,
    Expired,
    Unknown,
}

#[derive(Debug, Default)]
pub struct NestedGuestTracker {
    last_heard_from: HashMap<PaneId, Instant>,
}

impl NestedGuestTracker {
    pub fn on_announce(&mut self, pane_id: PaneId, now: Instant) {
        self.last_heard_from.insert(pane_id, now);
    }
    pub fn on_pong(&mut self, pane_id: PaneId, now: Instant) {
        if let Some(last_heard_from) = self.last_heard_from.get_mut(&pane_id) {
            *last_heard_from = now;
        }
    }
    pub fn on_tick(&mut self, pane_id: PaneId, now: Instant) -> GuestPingTickAction {
        match self.last_heard_from.get(&pane_id) {
            Some(last_heard_from)
                if now.duration_since(*last_heard_from)
                    > Duration::from_millis(PONG_TIMEOUT_MS) =>
            {
                self.last_heard_from.remove(&pane_id);
                GuestPingTickAction::Expired
            },
            Some(_) => GuestPingTickAction::SendPing,
            None => GuestPingTickAction::Unknown,
        }
    }
    pub fn remove(&mut self, pane_id: PaneId) {
        self.last_heard_from.remove(&pane_id);
    }
    pub fn is_tracked(&self, pane_id: PaneId) -> bool {
        self.last_heard_from.contains_key(&pane_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn millis(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn announced_guest_gets_pinged_and_pong_keeps_it_alive() {
        let mut tracker = NestedGuestTracker::default();
        let pane_id = PaneId::Terminal(1);
        let start = Instant::now();
        tracker.on_announce(pane_id, start);
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(PING_INTERVAL_MS)),
            GuestPingTickAction::SendPing
        );
        tracker.on_pong(pane_id, start + millis(4000));
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(8000)),
            GuestPingTickAction::SendPing
        );
    }

    #[test]
    fn guest_that_never_pongs_expires_after_timeout() {
        let mut tracker = NestedGuestTracker::default();
        let pane_id = PaneId::Terminal(1);
        let start = Instant::now();
        tracker.on_announce(pane_id, start);
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(PONG_TIMEOUT_MS + 1)),
            GuestPingTickAction::Expired
        );
        assert!(!tracker.is_tracked(pane_id));
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(PONG_TIMEOUT_MS + 2)),
            GuestPingTickAction::Unknown
        );
    }

    #[test]
    fn pong_refreshes_the_expiry_deadline() {
        let mut tracker = NestedGuestTracker::default();
        let pane_id = PaneId::Terminal(1);
        let start = Instant::now();
        tracker.on_announce(pane_id, start);
        tracker.on_pong(pane_id, start + millis(4000));
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(PONG_TIMEOUT_MS + 1000)),
            GuestPingTickAction::SendPing
        );
        assert_eq!(
            tracker.on_tick(pane_id, start + millis(4000 + PONG_TIMEOUT_MS + 1)),
            GuestPingTickAction::Expired
        );
    }

    #[test]
    fn pong_from_unannounced_pane_is_ignored() {
        let mut tracker = NestedGuestTracker::default();
        let pane_id = PaneId::Terminal(1);
        let now = Instant::now();
        tracker.on_pong(pane_id, now);
        assert!(!tracker.is_tracked(pane_id));
        assert_eq!(tracker.on_tick(pane_id, now), GuestPingTickAction::Unknown);
    }

    #[test]
    fn removed_guest_stops_being_tracked() {
        let mut tracker = NestedGuestTracker::default();
        let pane_id = PaneId::Terminal(1);
        let now = Instant::now();
        tracker.on_announce(pane_id, now);
        tracker.remove(pane_id);
        assert_eq!(tracker.on_tick(pane_id, now), GuestPingTickAction::Unknown);
    }
}
