use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) struct ProgressPhrases {
    pub(crate) nothing: &'static str,
    pub(crate) kept: &'static str,
}

pub(crate) const CLIENT_SCREEN_PROGRESS: ProgressPhrases = ProgressPhrases {
    nothing: "nothing was written to the client screen",
    kept: "output kept arriving",
};

pub(crate) const NESTED_FRAME_PROGRESS: ProgressPhrases = ProgressPhrases {
    nothing: "no nested frame was decoded",
    kept: "nested frames kept arriving",
};

pub(crate) const FAKE_PTY_PROGRESS: ProgressPhrases = ProgressPhrases {
    nothing: "nothing changed in the fake pty registry",
    kept: "the fake pty registry kept changing",
};

pub(crate) const SERIALIZATION_PROGRESS: ProgressPhrases = ProgressPhrases {
    nothing: "the serialized session on disk stopped growing",
    kept: "the serialized session on disk kept growing",
};

pub(crate) enum ExpiredTier {
    NoProgress,
    HardCap,
}

pub(crate) struct ProgressDeadline {
    phrases: ProgressPhrases,
    no_progress_budget: Duration,
    hard_cap_budget: Duration,
    last_progress_at: Instant,
    hard_cap_at: Instant,
}

impl ProgressDeadline {
    pub(crate) fn starting_now(phrases: ProgressPhrases) -> Self {
        Self::with_budgets(phrases, crate::default_timeout(), crate::hard_cap_timeout())
    }

    pub(crate) fn with_budgets(
        phrases: ProgressPhrases,
        no_progress_budget: Duration,
        hard_cap_budget: Duration,
    ) -> Self {
        let now = Instant::now();
        let hard_cap_budget = hard_cap_budget.max(no_progress_budget);
        ProgressDeadline {
            phrases,
            no_progress_budget,
            hard_cap_budget,
            last_progress_at: now,
            hard_cap_at: now + hard_cap_budget,
        }
    }

    pub(crate) fn note_progress(&mut self, now: Instant) {
        self.last_progress_at = now;
    }

    fn no_progress_at(&self) -> Instant {
        self.last_progress_at + self.no_progress_budget
    }

    pub(crate) fn expired(&self, now: Instant) -> Option<ExpiredTier> {
        if now >= self.no_progress_at() {
            Some(ExpiredTier::NoProgress)
        } else if now >= self.hard_cap_at {
            Some(ExpiredTier::HardCap)
        } else {
            None
        }
    }

    pub(crate) fn remaining(&self, now: Instant) -> Duration {
        self.no_progress_at()
            .min(self.hard_cap_at)
            .saturating_duration_since(now)
    }

    pub(crate) fn explain(&self, tier: ExpiredTier, now: Instant) -> String {
        match tier {
            ExpiredTier::NoProgress => format!(
                "no-progress deadline tripped: {} for {:.1?}",
                self.phrases.nothing,
                now.saturating_duration_since(self.last_progress_at),
            ),
            ExpiredTier::HardCap => format!(
                "hard cap tripped: {} (most recently {:.1?} ago) but the condition never held within {:.1?}",
                self.phrases.kept,
                now.saturating_duration_since(self.last_progress_at),
                self.hard_cap_budget,
            ),
        }
    }

    pub(crate) fn tripped(&self, now: Instant) -> Option<String> {
        self.expired(now).map(|tier| self.explain(tier, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deadline(no_progress: u64, hard_cap: u64) -> ProgressDeadline {
        ProgressDeadline::with_budgets(
            CLIENT_SCREEN_PROGRESS,
            Duration::from_millis(no_progress),
            Duration::from_millis(hard_cap),
        )
    }

    #[test]
    fn silence_trips_the_no_progress_tier_and_names_it() {
        let deadline = deadline(100, 10_000);
        let now = deadline.last_progress_at + Duration::from_millis(150);
        let tripped = deadline.tripped(now).expect("the deadline to have expired");
        assert!(
            tripped.starts_with(
                "no-progress deadline tripped: nothing was written to the client screen for"
            ),
            "{}",
            tripped
        );
    }

    #[test]
    fn progress_postpones_the_no_progress_tier_without_moving_the_hard_cap() {
        let mut deadline = deadline(100, 250);
        let started_at = deadline.last_progress_at;
        for step in 1..=4 {
            let now = started_at + Duration::from_millis(step * 50);
            assert!(
                deadline.expired(now).is_none() || step * 50 >= 250,
                "step {} expired while progress was being made",
                step
            );
            deadline.note_progress(now);
        }
        let past_the_cap = started_at + Duration::from_millis(275);
        let tripped = deadline
            .tripped(past_the_cap)
            .expect("the hard cap to have expired");
        assert!(
            tripped.starts_with("hard cap tripped: output kept arriving"),
            "{}",
            tripped
        );
    }

    #[test]
    fn the_wait_never_exceeds_the_nearer_of_the_two_tiers() {
        let deadline = deadline(100, 250);
        let now = deadline.last_progress_at + Duration::from_millis(10);
        assert_eq!(deadline.remaining(now), Duration::from_millis(90));
        let past_everything = deadline.last_progress_at + Duration::from_millis(400);
        assert_eq!(deadline.remaining(past_everything), Duration::ZERO);
    }

    #[test]
    fn a_hard_cap_below_the_no_progress_budget_cannot_invert_the_tiers() {
        let deadline = ProgressDeadline::with_budgets(
            CLIENT_SCREEN_PROGRESS,
            Duration::from_millis(100),
            Duration::from_millis(10),
        );
        assert_eq!(deadline.hard_cap_budget, Duration::from_millis(100));
        let inside = deadline.last_progress_at + Duration::from_millis(50);
        assert!(deadline.expired(inside).is_none());
    }
}
