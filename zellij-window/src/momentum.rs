use std::collections::VecDeque;
use std::time::{Duration, Instant};

use winit::event::TouchPhase;
use winit::keyboard::ModifiersState;
use zellij_utils::position::Position;
use zellij_utils::structured_render::{GeometryRecord, PaneRect, ScrollRecord};

pub const HISTORY: Duration = Duration::from_millis(100);
pub const START_LINES_PER_SECOND: f64 = 20.0;
pub const STOP_LINES_PER_SECOND: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub pane: usize,
    pub outline: PaneRect,
}

pub fn target_at(geometry: &GeometryRecord, cell: Option<(u16, u16)>) -> Option<Target> {
    let (x, y) = cell?;
    let pane = geometry
        .panes
        .iter()
        .rposition(|pane| pane.content_contains(x, y))?;
    Some(Target {
        pane,
        outline: geometry.panes[pane],
    })
}

#[derive(Debug, Default)]
pub struct Speed {
    samples: VecDeque<(Instant, f64)>,
}

impl Speed {
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn push(&mut self, at: Instant, pixels: f64) {
        if let Some(&(last, _)) = self.samples.back() {
            if at.saturating_duration_since(last) > HISTORY {
                self.samples.clear();
            }
        }
        self.samples.push_back((at, pixels));
        while let Some(&(first, _)) = self.samples.front() {
            if at.saturating_duration_since(first) > HISTORY {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn per_second(&self, now: Instant) -> f64 {
        let (Some(&(first, _)), Some(&(last, _))) = (self.samples.front(), self.samples.back())
        else {
            return 0.0;
        };
        if now.saturating_duration_since(last) > HISTORY {
            return 0.0;
        }
        let span = last.saturating_duration_since(first).as_secs_f64();
        if span <= 0.0 {
            return 0.0;
        }
        let moved: f64 = self.samples.iter().skip(1).map(|(_, pixels)| pixels).sum();
        moved / span
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Touch {
    pub phase: TouchPhase,
    pub pixels: f64,
    pub at: Instant,
    pub target: Option<Target>,
    pub position: Position,
    pub modifiers: ModifiersState,
    pub cell_height: f64,
    pub friction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    pub pixels: f64,
    pub position: Position,
    pub modifiers: ModifiersState,
}

#[derive(Debug, Clone, Copy)]
struct Swipe {
    target: Option<Target>,
    hinted: bool,
}

#[derive(Debug, Clone, Copy)]
struct Coast {
    target: Target,
    position: Position,
    modifiers: ModifiersState,
    speed: f64,
    friction: f64,
    since: Instant,
    reached: f64,
    lasts: f64,
    awaiting: bool,
}

impl Coast {
    fn new(touch: &Touch, target: Target, speed: f64) -> Option<Self> {
        let stop = STOP_LINES_PER_SECOND * touch.cell_height;
        if !(touch.friction.is_finite() && touch.friction > 0.0) || speed.abs() <= stop {
            return None;
        }
        Some(Self {
            target,
            position: touch.position,
            modifiers: touch.modifiers,
            speed,
            friction: touch.friction,
            since: touch.at,
            reached: 0.0,
            lasts: (speed.abs() / stop).ln() / touch.friction,
            awaiting: false,
        })
    }

    fn step(&mut self, now: Instant) -> f64 {
        let elapsed = now
            .saturating_duration_since(self.since)
            .as_secs_f64()
            .min(self.lasts)
            .max(self.reached);
        let k = self.friction;
        let distance = self.speed / k * ((-k * self.reached).exp() - (-k * elapsed).exp());
        self.reached = elapsed;
        distance
    }

    fn finished(&self) -> bool {
        self.reached >= self.lasts
    }

    #[cfg(test)]
    fn speed_now(&self) -> f64 {
        self.speed * (-self.friction * self.reached).exp()
    }
}

#[derive(Debug, Default)]
pub struct Momentum {
    speed: Speed,
    swipe: Option<Swipe>,
    coast: Option<Coast>,
}

impl Momentum {
    pub fn is_coasting(&self) -> bool {
        self.coast.is_some()
    }

    pub fn watches_frames(&self) -> bool {
        self.swipe.is_some() || self.coast.is_some()
    }

    pub fn stop(&mut self) {
        self.speed.clear();
        self.swipe = None;
        self.coast = None;
    }

    #[cfg(test)]
    pub fn current_speed(&self) -> Option<f64> {
        self.coast.as_ref().map(Coast::speed_now)
    }

    pub fn touchpad(&mut self, touch: Touch) {
        match touch.phase {
            TouchPhase::Started => {
                self.coast = None;
                self.speed.clear();
                self.speed.push(touch.at, touch.pixels);
                self.swipe = Some(Swipe {
                    target: touch.target,
                    hinted: false,
                });
            },
            TouchPhase::Moved => {
                self.coast = None;
                let same = self
                    .swipe
                    .map(|swipe| swipe.target == touch.target)
                    .unwrap_or(false);
                if !same {
                    self.speed.clear();
                    self.swipe = Some(Swipe {
                        target: touch.target,
                        hinted: false,
                    });
                }
                self.speed.push(touch.at, touch.pixels);
            },
            TouchPhase::Ended => {
                if touch.pixels != 0.0 {
                    self.speed.push(touch.at, touch.pixels);
                }
                let speed = self.speed.per_second(touch.at);
                let swipe = self.swipe.take();
                self.speed.clear();
                self.coast = None;
                let (Some(swipe), Some(target)) = (swipe, touch.target) else {
                    return;
                };
                if !swipe.hinted || swipe.target != Some(target) {
                    return;
                }
                if touch.modifiers.control_key() || touch.modifiers.alt_key() {
                    return;
                }
                if speed.abs() < START_LINES_PER_SECOND * touch.cell_height {
                    return;
                }
                self.coast = Coast::new(&touch, target, speed);
            },
            TouchPhase::Cancelled => self.stop(),
        }
    }

    pub fn note_frame(&mut self, hints: &ScrollRecord, geometry: &GeometryRecord) {
        if let Some(swipe) = self.swipe.as_mut() {
            if let Some(target) = swipe.target {
                if geometry.panes.get(target.pane) != Some(&target.outline) {
                    swipe.target = None;
                    swipe.hinted = false;
                } else if hints.lines_for(target.pane as u16).is_some() {
                    swipe.hinted = true;
                }
            }
        }
        let Some(coast) = self.coast.as_mut() else {
            return;
        };
        if geometry.panes.get(coast.target.pane) != Some(&coast.target.outline) {
            self.coast = None;
            return;
        }
        if !coast.awaiting {
            return;
        }
        if hints.lines_for(coast.target.pane as u16).is_some() {
            coast.awaiting = false;
        } else {
            self.coast = None;
        }
    }

    pub fn pointer_moved(&mut self, target: Option<Target>) {
        if let Some(coast) = &self.coast {
            if target != Some(coast.target) {
                self.coast = None;
            }
        }
    }

    pub fn tick(&mut self, now: Instant) -> Option<Glide> {
        let coast = self.coast.as_mut()?;
        let glide = Glide {
            pixels: coast.step(now),
            position: coast.position,
            modifiers: coast.modifiers,
        };
        if coast.finished() {
            self.coast = None;
        }
        Some(glide)
    }

    pub fn sent(&mut self) {
        if let Some(coast) = self.coast.as_mut() {
            coast.awaiting = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::structured_render::{ScrollEntry, PANE_FRAMED};

    const CELL: f64 = 20.0;
    const FRICTION: f64 = 2.0;

    fn pane(x: u16) -> PaneRect {
        PaneRect {
            x,
            y: 0,
            cols: 60,
            rows: 40,
            top: 1,
            bottom: 1,
            left: 1,
            right: 1,
            flags: PANE_FRAMED,
        }
    }

    fn geometry() -> GeometryRecord {
        GeometryRecord {
            panes: vec![pane(0), pane(60)],
        }
    }

    fn left() -> Option<Target> {
        target_at(&geometry(), Some((10, 10)))
    }

    fn hints(pane: u16) -> ScrollRecord {
        ScrollRecord {
            entries: vec![ScrollEntry { pane, lines: 2 }],
        }
    }

    fn touch(phase: TouchPhase, pixels: f64, at: Instant) -> Touch {
        Touch {
            phase,
            pixels,
            at,
            target: left(),
            position: Position::new(10, 10),
            modifiers: ModifiersState::empty(),
            cell_height: CELL,
            friction: FRICTION,
        }
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn swipe(momentum: &mut Momentum, start: Instant, pixels: f64, moves: u64, hinted: bool) {
        momentum.touchpad(touch(TouchPhase::Started, pixels, start));
        for step in 1..=moves {
            momentum.touchpad(touch(TouchPhase::Moved, pixels, start + ms(10 * step)));
        }
        if hinted {
            momentum.note_frame(&hints(0), &geometry());
        }
        momentum.touchpad(touch(TouchPhase::Ended, 0.0, start + ms(10 * moves + 5)));
    }

    fn glide_total(momentum: &mut Momentum, from: Instant, interval: Duration) -> f64 {
        let mut total = 0.0;
        let mut now = from;
        while momentum.is_coasting() {
            now += interval;
            total += momentum.tick(now).unwrap().pixels;
            momentum.sent();
            momentum.note_frame(&hints(0), &geometry());
        }
        total
    }

    #[test]
    fn the_pane_under_the_pointer_is_found_by_its_content() {
        assert_eq!(left().unwrap().pane, 0);
        assert_eq!(target_at(&geometry(), Some((70, 10))).unwrap().pane, 1);
        assert_eq!(target_at(&geometry(), Some((0, 0))), None);
        assert_eq!(target_at(&geometry(), None), None);
    }

    #[test]
    fn the_speed_is_the_movement_over_the_time_it_took() {
        let mut speed = Speed::default();
        let start = Instant::now();
        speed.push(start, 40.0);
        assert_eq!(speed.per_second(start), 0.0, "one sample has no speed");
        speed.push(start + ms(10), 40.0);
        speed.push(start + ms(20), 40.0);
        let measured = speed.per_second(start + ms(20));
        assert!((measured - 4000.0).abs() < 1e-6, "{}", measured);
        speed.push(start + ms(30), -20.0);
        let measured = speed.per_second(start + ms(30));
        assert!((measured - 2000.0).abs() < 1e-6, "{}", measured);
    }

    #[test]
    fn only_the_last_tenth_of_a_second_counts() {
        let mut speed = Speed::default();
        let start = Instant::now();
        speed.push(start, 400.0);
        for step in 1..=20 {
            speed.push(start + ms(10 * step), 10.0);
        }
        let measured = speed.per_second(start + ms(200));
        assert!((measured - 1000.0).abs() < 1e-6, "{}", measured);
    }

    #[test]
    fn a_long_gap_forgets_the_earlier_movement() {
        let mut speed = Speed::default();
        let start = Instant::now();
        speed.push(start, 40.0);
        speed.push(start + ms(10), 40.0);
        speed.push(start + ms(200), 40.0);
        assert_eq!(speed.per_second(start + ms(200)), 0.0);
        assert_eq!(
            Speed::default().per_second(start),
            0.0,
            "no movement is no speed"
        );
        let mut resting = Speed::default();
        resting.push(start, 40.0);
        resting.push(start + ms(10), 40.0);
        assert_eq!(
            resting.per_second(start + ms(150)),
            0.0,
            "fingers that rested before lifting carry no speed"
        );
    }

    #[test]
    fn a_new_swipe_starts_its_speed_afresh() {
        let mut momentum = Momentum::default();
        let start = Instant::now();
        momentum.touchpad(touch(TouchPhase::Started, 40.0, start));
        momentum.touchpad(touch(TouchPhase::Moved, 40.0, start + ms(10)));
        momentum.touchpad(touch(TouchPhase::Started, 1.0, start + ms(20)));
        momentum.touchpad(touch(TouchPhase::Moved, 1.0, start + ms(30)));
        assert!((momentum.speed.per_second(start + ms(30)) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn a_fast_swipe_that_scrolled_the_pane_keeps_going() {
        let mut momentum = Momentum::default();
        swipe(&mut momentum, Instant::now(), 40.0, 5, true);
        assert!(momentum.is_coasting());
        assert!((momentum.current_speed().unwrap() - 4000.0).abs() < 1e-6);
    }

    #[test]
    fn a_slow_swipe_stops_where_the_fingers_lift() {
        let mut momentum = Momentum::default();
        swipe(&mut momentum, Instant::now(), 3.0, 5, true);
        assert!(!momentum.is_coasting());
    }

    #[test]
    fn a_lift_with_no_movement_before_it_starts_nothing() {
        let mut momentum = Momentum::default();
        let start = Instant::now();
        momentum.note_frame(&hints(0), &geometry());
        momentum.touchpad(touch(TouchPhase::Ended, 0.0, start));
        assert!(!momentum.is_coasting());
        momentum.touchpad(touch(TouchPhase::Ended, 400.0, start + ms(5)));
        assert!(!momentum.is_coasting());
    }

    #[test]
    fn a_swipe_that_produced_no_scroll_hint_starts_nothing() {
        let mut momentum = Momentum::default();
        swipe(&mut momentum, Instant::now(), 40.0, 5, false);
        assert!(!momentum.is_coasting());

        let mut momentum = Momentum::default();
        let start = Instant::now();
        momentum.touchpad(touch(TouchPhase::Started, 40.0, start));
        momentum.touchpad(touch(TouchPhase::Moved, 40.0, start + ms(10)));
        momentum.note_frame(&hints(1), &geometry());
        momentum.touchpad(touch(TouchPhase::Ended, 0.0, start + ms(15)));
        assert!(
            !momentum.is_coasting(),
            "a hint for another pane is no evidence"
        );
    }

    #[test]
    fn a_swipe_with_ctrl_or_alt_starts_nothing() {
        for modifiers in [ModifiersState::CONTROL, ModifiersState::ALT] {
            let mut momentum = Momentum::default();
            let start = Instant::now();
            momentum.touchpad(touch(TouchPhase::Started, 40.0, start));
            momentum.touchpad(touch(TouchPhase::Moved, 40.0, start + ms(10)));
            momentum.note_frame(&hints(0), &geometry());
            momentum.touchpad(Touch {
                modifiers,
                ..touch(TouchPhase::Ended, 0.0, start + ms(15))
            });
            assert!(!momentum.is_coasting());
        }
    }

    #[test]
    fn the_glide_goes_the_way_of_the_swipe_slows_down_and_stops() {
        for direction in [1.0, -1.0] {
            let mut momentum = Momentum::default();
            let start = Instant::now();
            swipe(&mut momentum, start, 40.0 * direction, 5, true);
            let end = start + ms(55);
            let mut previous = f64::INFINITY;
            let mut now = end;
            while momentum.is_coasting() {
                now += ms(16);
                let glide = momentum.tick(now).unwrap();
                assert_eq!(glide.pixels.signum(), direction);
                assert!(glide.pixels.abs() <= previous);
                previous = glide.pixels.abs();
                assert_eq!(glide.position, Position::new(10, 10));
            }
            let lasted = now.duration_since(end).as_secs_f64();
            let expected = (4000.0 / (STOP_LINES_PER_SECOND * CELL)).ln() / FRICTION;
            assert!(
                lasted >= expected && lasted < expected + 0.017,
                "{}",
                lasted
            );
            assert_eq!(momentum.tick(now + ms(16)), None);
        }
    }

    #[test]
    fn the_glide_covers_the_same_distance_at_any_refresh_rate() {
        let start = Instant::now();
        let end = start + ms(55);
        let mut sixty = Momentum::default();
        swipe(&mut sixty, start, 40.0, 5, true);
        let at_sixty = glide_total(&mut sixty, end, Duration::from_nanos(16_666_667));
        let mut fast = Momentum::default();
        swipe(&mut fast, start, 40.0, 5, true);
        let at_fast = glide_total(&mut fast, end, Duration::from_nanos(6_944_444));
        let exact = (4000.0 - STOP_LINES_PER_SECOND * CELL) / FRICTION;
        assert!((at_sixty - exact).abs() < 1e-6, "{}", at_sixty);
        assert!((at_fast - exact).abs() < 1e-6, "{}", at_fast);
    }

    #[test]
    fn higher_friction_stops_sooner() {
        let start = Instant::now();
        let mut gentle = Momentum::default();
        swipe(&mut gentle, start, 40.0, 5, true);
        let mut firm = Momentum::default();
        firm.touchpad(touch(TouchPhase::Started, 40.0, start));
        for step in 1..=5 {
            firm.touchpad(touch(TouchPhase::Moved, 40.0, start + ms(10 * step)));
        }
        firm.note_frame(&hints(0), &geometry());
        firm.touchpad(Touch {
            friction: 8.0,
            ..touch(TouchPhase::Ended, 0.0, start + ms(55))
        });
        let interval = ms(16);
        assert!(
            glide_total(&mut firm, start + ms(55), interval)
                < glide_total(&mut gentle, start + ms(55), interval)
        );
    }

    #[test]
    fn a_sent_scroll_waits_for_the_next_frame_before_judging_it() {
        let mut momentum = Momentum::default();
        let start = Instant::now();
        swipe(&mut momentum, start, 40.0, 5, true);
        momentum.tick(start + ms(70));
        momentum.sent();
        momentum.tick(start + ms(86));
        assert!(momentum.is_coasting(), "no frame has come back yet");
        momentum.note_frame(&hints(0), &geometry());
        assert!(momentum.is_coasting());
        momentum.note_frame(&ScrollRecord::default(), &geometry());
        assert!(
            momentum.is_coasting(),
            "nothing was sent since the last answered scroll"
        );
        momentum.sent();
        momentum.note_frame(&ScrollRecord::default(), &geometry());
        assert!(!momentum.is_coasting(), "the scrollback has run out");
    }

    #[test]
    fn touching_again_or_leaving_the_pane_stops_the_glide() {
        let start = Instant::now();
        for phase in [
            TouchPhase::Started,
            TouchPhase::Moved,
            TouchPhase::Cancelled,
        ] {
            let mut momentum = Momentum::default();
            swipe(&mut momentum, start, 40.0, 5, true);
            momentum.touchpad(touch(phase, 0.0, start + ms(80)));
            assert!(!momentum.is_coasting(), "{:?}", phase);
        }
        let mut momentum = Momentum::default();
        swipe(&mut momentum, start, 40.0, 5, true);
        momentum.pointer_moved(left());
        assert!(momentum.is_coasting());
        momentum.pointer_moved(target_at(&geometry(), Some((70, 10))));
        assert!(!momentum.is_coasting());

        let mut momentum = Momentum::default();
        swipe(&mut momentum, start, 40.0, 5, true);
        let mut moved = geometry();
        moved.panes[0].cols = 50;
        momentum.note_frame(&hints(0), &moved);
        assert!(!momentum.is_coasting(), "the pane changed shape");
    }
}
