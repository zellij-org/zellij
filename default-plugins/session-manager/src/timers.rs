use std::cell::RefCell;
use zellij_tile::prelude::set_timeout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerOwner {
    SessionsPreview,
    Clock,
    Spinner,
}

#[derive(Debug, Default)]
pub struct Timers {
    pending: Vec<(u64, u64, TimerOwner)>,
    next_sequence: u64,
}

impl Timers {
    pub fn record(&mut self, owner: TimerOwner, seconds: f64, now_ms: u64) {
        let deadline = now_ms + (seconds.max(0.0) * 1000.0) as u64;
        self.pending.push((deadline, self.next_sequence, owner));
        self.next_sequence += 1;
    }

    pub fn fired(&mut self) -> Option<TimerOwner> {
        let earliest = self
            .pending
            .iter()
            .enumerate()
            .min_by_key(|(_, (deadline, sequence, _))| (*deadline, *sequence))
            .map(|(index, _)| index)?;
        Some(self.pending.remove(earliest).2)
    }
}

thread_local! {
    static TIMERS: RefCell<Timers> = RefCell::new(Timers::default());
}

pub fn start(owner: TimerOwner, seconds: f64) {
    TIMERS.with(|timers| {
        timers
            .borrow_mut()
            .record(owner, seconds, crate::inbox::now_ms())
    });
    set_timeout(seconds);
}

pub fn fired() -> Option<TimerOwner> {
    TIMERS.with(|timers| timers.borrow_mut().fired())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_timer_event_goes_to_the_earliest_timer() {
        let mut timers = Timers::default();
        timers.record(TimerOwner::Clock, 1.0, 0);
        timers.record(TimerOwner::SessionsPreview, 0.3, 100);
        timers.record(TimerOwner::Clock, 0.3, 100);
        assert_eq!(timers.fired(), Some(TimerOwner::SessionsPreview));
        assert_eq!(timers.fired(), Some(TimerOwner::Clock));
        assert_eq!(timers.fired(), Some(TimerOwner::Clock));
        assert_eq!(timers.fired(), None);
    }
}
