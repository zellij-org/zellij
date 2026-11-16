use zellij_tile::prelude::set_timeout;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerOwner {
    TabBar,
    StatusBar,
}

#[derive(Debug, Default)]
pub struct Timers {
    pending: Vec<(u64, u64, TimerOwner)>,
    next_sequence: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Timers {
    pub fn start(&mut self, owner: TimerOwner, seconds: f64) {
        self.record(owner, seconds, now_ms());
        set_timeout(seconds);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_timer_goes_back_to_the_component_that_set_it() {
        let mut timers = Timers::default();
        timers.record(TimerOwner::StatusBar, 5.2, 1_000);
        timers.record(TimerOwner::TabBar, 5.0, 1_100);
        timers.record(TimerOwner::TabBar, 5.0, 1_150);
        assert_eq!(timers.fired(), Some(TimerOwner::TabBar));
        assert_eq!(timers.fired(), Some(TimerOwner::TabBar));
        assert_eq!(timers.fired(), Some(TimerOwner::StatusBar));
        assert_eq!(timers.fired(), None);
    }

    #[test]
    fn timers_with_the_same_deadline_fire_in_the_order_they_were_set() {
        let mut timers = Timers::default();
        timers.record(TimerOwner::StatusBar, 1.0, 0);
        timers.record(TimerOwner::TabBar, 1.0, 0);
        assert_eq!(timers.fired(), Some(TimerOwner::StatusBar));
        assert_eq!(timers.fired(), Some(TimerOwner::TabBar));
    }
}
