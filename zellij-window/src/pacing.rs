use std::time::{Duration, Instant};

const FALLBACK_REFRESH_MILLIHERTZ: u32 = 60_000;
const SLOWEST_REFRESH_MILLIHERTZ: u32 = 20_000;
const FASTEST_REFRESH_MILLIHERTZ: u32 = 500_000;

pub fn interval_of(millihertz: Option<u32>) -> Duration {
    let millihertz = match millihertz {
        Some(rate) if (SLOWEST_REFRESH_MILLIHERTZ..=FASTEST_REFRESH_MILLIHERTZ).contains(&rate) => {
            rate
        },
        _ => FALLBACK_REFRESH_MILLIHERTZ,
    };
    Duration::from_nanos(1_000_000_000_000 / millihertz as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Idle,
    Now,
    At(Instant),
}

pub struct Pacer {
    interval: Duration,
    last_draw: Option<Instant>,
    pending: bool,
}

impl Pacer {
    pub fn new(millihertz: Option<u32>) -> Self {
        Self {
            interval: interval_of(millihertz),
            last_draw: None,
            pending: false,
        }
    }

    #[cfg(test)]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    pub fn follow_refresh(&mut self, millihertz: Option<u32>) {
        self.interval = interval_of(millihertz);
    }

    pub fn schedule(&mut self) {
        self.pending = true;
    }

    pub fn drawn(&mut self, at: Instant) {
        self.pending = false;
        self.last_draw = Some(at);
    }

    pub fn decide(&self, now: Instant) -> Decision {
        if !self.pending {
            return Decision::Idle;
        }
        let Some(last) = self.last_draw else {
            return Decision::Now;
        };
        let due = last + self.interval;
        if now >= due {
            Decision::Now
        } else {
            Decision::At(due)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_sixty() -> Pacer {
        Pacer::new(Some(60_000))
    }

    #[test]
    fn nothing_scheduled_asks_for_nothing() {
        let pacer = at_sixty();
        assert_eq!(pacer.decide(Instant::now()), Decision::Idle);
    }

    #[test]
    fn the_first_scheduled_draw_happens_at_once() {
        let mut pacer = at_sixty();
        pacer.schedule();
        assert_eq!(pacer.decide(Instant::now()), Decision::Now);
    }

    #[test]
    fn a_draw_older_than_one_interval_is_not_made_to_wait() {
        let mut pacer = at_sixty();
        let start = Instant::now();
        pacer.drawn(start);
        pacer.schedule();
        assert_eq!(pacer.decide(start + pacer.interval()), Decision::Now);
        assert_eq!(
            pacer.decide(start + pacer.interval() + Duration::from_millis(5)),
            Decision::Now
        );
    }

    #[test]
    fn a_draw_inside_the_interval_waits_for_the_boundary_and_no_longer() {
        let mut pacer = at_sixty();
        let start = Instant::now();
        pacer.drawn(start);
        pacer.schedule();
        let boundary = start + pacer.interval();
        assert_eq!(
            pacer.decide(start + Duration::from_micros(700)),
            Decision::At(boundary)
        );
        assert_eq!(
            pacer.decide(boundary - Duration::from_micros(1)),
            Decision::At(boundary)
        );
    }

    #[test]
    fn many_scheduled_frames_between_two_draws_produce_one_draw() {
        let mut pacer = at_sixty();
        let start = Instant::now();
        pacer.drawn(start);
        let mut draws = 0;
        for step in 0..100u32 {
            let now = start + Duration::from_millis(step as u64);
            pacer.schedule();
            if pacer.decide(now) == Decision::Now {
                pacer.drawn(now);
                draws += 1;
            }
        }
        assert_eq!(
            draws, 5,
            "100 ms of frames at 60 Hz is a draw per refresh, not one per frame"
        );
    }

    #[test]
    fn a_draw_clears_what_was_scheduled() {
        let mut pacer = at_sixty();
        let start = Instant::now();
        pacer.schedule();
        pacer.drawn(start);
        assert_eq!(pacer.decide(start + Duration::from_secs(1)), Decision::Idle);
    }

    #[test]
    fn a_keystroke_after_an_idle_stretch_draws_without_waiting() {
        let mut pacer = at_sixty();
        let start = Instant::now();
        pacer.drawn(start);
        let typed = start + Duration::from_secs(3);
        pacer.schedule();
        assert_eq!(pacer.decide(typed), Decision::Now);
    }

    #[test]
    fn the_interval_comes_from_the_display() {
        assert_eq!(interval_of(Some(60_000)), Duration::from_nanos(16_666_666));
        assert_eq!(interval_of(Some(144_000)), Duration::from_nanos(6_944_444));
        assert_eq!(interval_of(Some(59_940)), Duration::from_nanos(16_683_350));
    }

    #[test]
    fn an_unusable_refresh_rate_falls_back_to_sixty_hertz() {
        let sixty = interval_of(Some(60_000));
        for reported in [
            None,
            Some(0),
            Some(1),
            Some(19_999),
            Some(500_001),
            Some(u32::MAX),
        ] {
            assert_eq!(
                interval_of(reported),
                sixty,
                "{:?} is not a refresh rate to pace by",
                reported
            );
        }
    }

    #[test]
    fn following_the_display_replaces_the_interval() {
        let mut pacer = at_sixty();
        pacer.follow_refresh(Some(120_000));
        assert_eq!(pacer.interval(), interval_of(Some(120_000)));
        pacer.follow_refresh(None);
        assert_eq!(pacer.interval(), interval_of(Some(60_000)));
    }
}
