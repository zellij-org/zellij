use super::*;
use crate::pane_mirror::MirrorOutgoing;
use crossbeam::channel::{unbounded, Receiver};

fn size(cols: usize, rows: usize) -> Size {
    Size { cols, rows }
}

fn mirrors_with(
    subscribers: &[(MirrorSubscriberId, u32, bool)],
) -> (PaneMirrors, Vec<Receiver<MirrorOutgoing>>) {
    let mut mirrors = PaneMirrors::default();
    let mut receivers = vec![];
    for (subscriber_id, terminal_id, view_only) in subscribers {
        let (sender, receiver) = unbounded();
        receivers.push(receiver);
        mirrors.subscribers.insert(
            *subscriber_id,
            MirrorSubscriber {
                pane_id: PaneId::Terminal(*terminal_id),
                view_only: *view_only,
                resize_allowed: false,
                focused: false,
                yielded: false,
                viewer_size: size(80, 24),
                outbox: MirrorOutbox::new(sender),
                pacer: MirrorFramePacer::default(),
                pin_order: 0,
                lease_until: None,
                reported_pin: None,
                parked: false,
                kept: None,
            },
        );
    }
    (mirrors, receivers)
}

fn winner_of(mirrors: &PaneMirrors, terminal_id: u32, now: Instant) -> Option<(u64, Size)> {
    choose_pins(&mirrors.subscribers, mirrors.resizes_source_pane, now)
        .get(&PaneId::Terminal(terminal_id))
        .copied()
}

#[test]
fn a_focused_mirror_that_allows_resizing_pins_its_pane_at_its_size() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    assert_eq!(winner_of(&mirrors, 5, now), None);
    mirrors.update_wish(1, size(40, 10), true, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
}

#[test]
fn a_mirror_pins_while_shown_even_without_focus_unless_it_may_not_resize() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
    mirrors.update_wish(1, size(40, 10), true, false, now);
    assert_eq!(winner_of(&mirrors, 5, now), None);
}

#[test]
fn a_view_only_mirror_never_pins() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, true)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), None);
}

#[test]
fn the_most_recently_focused_mirror_wins_and_hands_over_when_it_lets_go() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false), (3, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now);
    mirrors.update_wish(3, size(60, 14), true, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((3, size(60, 14))));
    mirrors.update_wish(1, size(41, 10), true, true, now);
    assert_eq!(
        winner_of(&mirrors, 5, now),
        Some((3, size(60, 14))),
        "a renewal does not count as a new focus"
    );
    mirrors.update_wish(3, size(60, 14), true, false, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((2, size(50, 12))));
    mirrors.subscribers.remove(&2);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(41, 10))));
    mirrors.update_wish(1, size(41, 10), true, false, now);
    assert_eq!(winner_of(&mirrors, 5, now), None);
}

#[test]
fn focusing_again_makes_a_mirror_the_most_recent() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now);
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.update_wish(1, size(40, 10), true, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
}

#[test]
fn mirrors_of_different_panes_pin_independently() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 6, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
    assert_eq!(winner_of(&mirrors, 6, now), Some((2, size(50, 12))));
}

#[test]
fn a_source_that_refuses_pins_has_no_winner() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.resizes_source_pane = false;
    assert_eq!(winner_of(&mirrors, 5, now), None);
}

#[test]
fn a_pin_that_is_not_renewed_expires() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let now = Instant::now();
    let lease = mirror_pin_lease();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now + lease / 2);
    assert_eq!(mirrors.next_lease_expiry(now), Some(now + lease));
    assert_eq!(
        winner_of(&mirrors, 5, now + lease / 2),
        Some((2, size(50, 12)))
    );
    assert_eq!(
        winner_of(&mirrors, 5, now + lease + lease / 4),
        Some((2, size(50, 12)))
    );
    assert_eq!(winner_of(&mirrors, 5, now + lease * 2), None);
    assert_eq!(mirrors.next_lease_expiry(now + lease * 2), None);
}

#[test]
fn a_renewal_keeps_the_pin() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    let lease = mirror_pin_lease();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(1, size(40, 10), true, true, now + lease / 2);
    assert_eq!(
        winner_of(&mirrors, 5, now + lease + lease / 4),
        Some((1, size(40, 10)))
    );
}

#[test]
fn a_renewal_after_the_lease_expired_counts_as_a_new_focus() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let now = Instant::now();
    let lease = mirror_pin_lease();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now);
    let later = now + lease * 2;
    mirrors.update_wish(2, size(50, 12), true, true, later);
    mirrors.update_wish(1, size(40, 10), true, true, later);
    assert_eq!(winner_of(&mirrors, 5, later), Some((1, size(40, 10))));
}

#[test]
fn viewer_sizes_are_clamped_when_stored() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(0, 100_000), true, true, now);
    assert_eq!(
        winner_of(&mirrors, 5, now),
        Some((1, size(1, zellij_utils::pane_mirror::MAX_MIRROR_ROWS)))
    );
}

#[derive(Default)]
struct FakePinHost {
    layout_sizes: HashMap<PaneId, Size>,
    pinned: HashMap<PaneId, Size>,
    applied: Vec<(PaneId, Option<Size>)>,
}

impl PinHost for FakePinHost {
    fn program_size(&self, pane_id: PaneId) -> Option<Size> {
        self.pinned
            .get(&pane_id)
            .or_else(|| self.layout_sizes.get(&pane_id))
            .copied()
    }
    fn set_pinned_size(&mut self, pane_id: PaneId, size: Option<Size>) -> Option<bool> {
        if !self.layout_sizes.contains_key(&pane_id) {
            return None;
        }
        self.applied.push((pane_id, size));
        match size {
            Some(size) => self.pinned.insert(pane_id, size),
            None => self.pinned.remove(&pane_id),
        };
        Some(true)
    }
}

#[test]
fn a_viewer_that_dies_while_its_pin_state_is_sent_is_forgotten_and_the_pane_gets_its_size_back() {
    let (mut mirrors, mut receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = FakePinHost::default();
    host.layout_sizes.insert(PaneId::Terminal(5), size(120, 40));
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    drop(receivers.remove(0));
    let settled = mirrors.settle_pins(&mut host, now);
    assert!(settled.removed_subscribers);
    assert!(mirrors.subscribers.is_empty());
    assert_eq!(mirrors.pinned_panes.get(&PaneId::Terminal(5)), None);
    assert_eq!(
        host.applied,
        vec![
            (PaneId::Terminal(5), Some(size(40, 10))),
            (PaneId::Terminal(5), None)
        ]
    );
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert_eq!(mirrors.next_lease_expiry(now), None);
}

#[test]
fn a_dead_viewer_hands_the_pin_to_the_next_viewer_in_the_same_pass() {
    let (mut mirrors, mut receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let mut host = FakePinHost::default();
    host.layout_sizes.insert(PaneId::Terminal(5), size(120, 40));
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    mirrors.update_wish(2, size(50, 12), true, true, now);
    let survivor = receivers.remove(0);
    drop(receivers.remove(0));
    let settled = mirrors.settle_pins(&mut host, now);
    assert!(settled.removed_subscribers);
    assert_eq!(mirrors.subscribers.len(), 1);
    assert_eq!(
        mirrors.pinned_panes.get(&PaneId::Terminal(5)),
        Some(&size(40, 10))
    );
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    let reported: Vec<bool> = survivor
        .try_iter()
        .filter_map(|outgoing| match outgoing {
            MirrorOutgoing::Message(SourceMessage::PinState { pinned, .. }) => Some(pinned),
            _ => None,
        })
        .collect();
    assert_eq!(reported.last(), Some(&true));
}

#[test]
fn settling_without_dead_viewers_removes_nobody() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = FakePinHost::default();
    host.layout_sizes.insert(PaneId::Terminal(5), size(120, 40));
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), true, true, now);
    let settled = mirrors.settle_pins(&mut host, now);
    assert_eq!(
        settled,
        PinsSettled {
            changed: true,
            removed_subscribers: false
        }
    );
    assert_eq!(mirrors.subscribers.len(), 1);
}

#[test]
fn local_input_on_a_pinned_pane_releases_it_until_a_viewer_claims_it_again() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 6, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.update_wish(2, size(50, 12), false, true, now);
    mirrors
        .pinned_panes
        .insert(PaneId::Terminal(5), size(40, 10));
    mirrors
        .pinned_panes
        .insert(PaneId::Terminal(6), size(50, 12));
    let typed_into: HashSet<PaneId> = [PaneId::Terminal(5)].into_iter().collect();
    assert!(mirrors.yield_panes(&typed_into));
    assert_eq!(winner_of(&mirrors, 5, now), None);
    assert_eq!(winner_of(&mirrors, 6, now), Some((2, size(50, 12))));
    mirrors.update_wish(1, size(40, 10), false, true, now);
    assert_eq!(
        winner_of(&mirrors, 5, now),
        None,
        "a renewal does not take the pane back"
    );
    mirrors.update_wish(1, size(40, 10), true, true, now);
    assert_eq!(
        winner_of(&mirrors, 5, now),
        Some((1, size(40, 10))),
        "starting to type into the view takes it back"
    );
    assert!(mirrors.yield_panes(&typed_into));
    assert!(mirrors.reclaim(1, now));
    assert_eq!(
        winner_of(&mirrors, 5, now),
        Some((1, size(40, 10))),
        "typing through the view takes it back"
    );
    assert!(!mirrors.reclaim(1, now));
}

#[test]
fn local_input_on_a_pane_that_is_not_pinned_changes_nothing() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    let typed_into: HashSet<PaneId> = [PaneId::Terminal(5)].into_iter().collect();
    assert!(!mirrors.yield_panes(&typed_into));
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
}

fn pinned_host() -> FakePinHost {
    let mut host = FakePinHost::default();
    host.layout_sizes.insert(PaneId::Terminal(5), size(120, 40));
    host
}

fn pin_then_move_on(mirrors: &mut PaneMirrors, host: &mut FakePinHost, now: Instant) {
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    assert!(mirrors.park(1));
    mirrors.settle_pins(host, now);
}

fn kept_sizes(mirrors: &PaneMirrors) -> Vec<Size> {
    mirrors
        .subscribers
        .values()
        .filter_map(|subscriber| subscriber.kept)
        .collect()
}

#[test]
fn a_pane_keeps_the_pinned_size_while_its_view_is_parked() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    assert_eq!(
        winner_of(&mirrors, 5, now),
        None,
        "a parked view does not pin"
    );
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    assert_eq!(
        mirrors.pinned_panes.get(&PaneId::Terminal(5)),
        Some(&size(40, 10))
    );
    let later = now + mirror_pin_lease() * 3;
    mirrors.settle_pins(&mut host, later);
    assert_eq!(
        host.program_size(PaneId::Terminal(5)),
        Some(size(40, 10)),
        "the kept size does not run out with the lease"
    );
}

#[test]
fn a_parked_view_that_detaches_gives_its_pane_the_layout_size_back() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    assert!(mirrors.forget_subscribers(vec![(1, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert!(mirrors.pinned_panes.is_empty());
}

#[test]
fn a_view_that_detaches_without_parking_gives_the_size_back_too() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    assert!(mirrors.forget_subscribers(vec![(1, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
}

#[test]
fn a_parked_view_sends_no_pin_state_and_ignores_resizes_until_it_is_unparked() {
    let (mut mirrors, receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    let _: Vec<MirrorOutgoing> = receivers[0].try_iter().collect();
    mirrors.update_wish(1, size(60, 20), true, true, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    assert_eq!(receivers[0].try_iter().count(), 0);
    assert!(!mirrors.park(1));
    assert!(mirrors.unpark(1, true, now));
    assert!(!mirrors.unpark(1, true, now));
    mirrors.update_wish(1, size(40, 10), false, false, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(
        host.program_size(PaneId::Terminal(5)),
        Some(size(40, 10)),
        "going back to a parked view before it settles does not resize the pane"
    );
    assert!(receivers[0].try_iter().any(|outgoing| matches!(
        outgoing,
        MirrorOutgoing::Message(SourceMessage::PinState { pinned: false, .. })
    )));
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(winner_of(&mirrors, 5, now), Some((1, size(40, 10))));
    assert_eq!(
        host.applied,
        vec![(PaneId::Terminal(5), Some(size(40, 10)))],
        "the pane was resized only once"
    );
}

#[test]
fn unparking_with_resizing_off_gives_the_size_back() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    assert!(mirrors.unpark(1, false, now));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
}

#[test]
fn local_focus_or_input_gives_back_the_size_a_parked_view_keeps() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    mirrors.forget_kept_sizes([PaneId::Terminal(5)].iter());
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert!(kept_sizes(&mirrors).is_empty());
    assert_eq!(
        mirrors.subscribers.len(),
        1,
        "the parked view stays attached"
    );
}

#[test]
fn when_one_viewer_closes_the_size_another_viewer_keeps_still_applies() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert!(mirrors.park(1));
    mirrors.update_wish(2, size(50, 12), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(50, 12)));
    assert!(mirrors.forget_subscribers(vec![(2, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(
        host.program_size(PaneId::Terminal(5)),
        Some(size(40, 10)),
        "the parked viewer still keeps its size"
    );
    assert!(mirrors.forget_subscribers(vec![(1, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
}

#[test]
fn a_shown_view_keeps_its_pane_when_a_parked_view_of_it_detaches() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert!(mirrors.park(1));
    mirrors.update_wish(2, size(50, 12), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert!(mirrors.forget_subscribers(vec![(1, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(50, 12)));
}

#[test]
fn forgetting_the_kept_size_after_local_focus_or_input_gives_back_the_layout_size() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    let other = [PaneId::Terminal(6)];
    mirrors.forget_kept_sizes(other.iter());
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    let touched = [PaneId::Terminal(5)];
    mirrors.forget_kept_sizes(touched.iter());
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert!(mirrors.pinned_panes.is_empty());
}

#[test]
fn a_closed_pane_forgets_its_kept_size() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    mirrors.forget_pane(PaneId::Terminal(5));
    assert!(kept_sizes(&mirrors).is_empty());
    assert!(mirrors.pinned_panes.is_empty());
}

#[test]
fn turning_off_resizing_in_the_source_session_gives_back_kept_sizes() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    pin_then_move_on(&mut mirrors, &mut host, now);
    mirrors.resizes_source_pane = false;
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert!(kept_sizes(&mirrors).is_empty());
    mirrors.resizes_source_pane = true;
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
}

#[test]
fn turning_off_resizing_for_a_view_drops_only_its_own_kept_size_at_once() {
    let (mut mirrors, _receivers) = mirrors_with(&[(1, 5, false), (2, 5, false)]);
    let mut host = pinned_host();
    let now = Instant::now();
    mirrors.update_wish(2, size(50, 12), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert!(mirrors.park(2));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(50, 12)));
    mirrors.update_wish(1, size(40, 10), false, false, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(
        host.program_size(PaneId::Terminal(5)),
        Some(size(50, 12)),
        "a view that never resized keeps the size another view left"
    );
    mirrors.update_wish(1, size(40, 10), false, true, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(40, 10)));
    mirrors.update_wish(1, size(40, 10), false, false, now);
    mirrors.settle_pins(&mut host, now);
    assert_eq!(
        host.program_size(PaneId::Terminal(5)),
        Some(size(50, 12)),
        "turning resizing off drops only the claim of that view"
    );
    assert!(mirrors.forget_subscribers(vec![(2, MirrorRemoval::Finished)]));
    mirrors.settle_pins(&mut host, now);
    assert_eq!(host.program_size(PaneId::Terminal(5)), Some(size(120, 40)));
    assert!(kept_sizes(&mirrors).is_empty());
}

#[test]
fn a_pane_that_runs_the_viewing_session_or_one_it_is_nested_in_is_not_mirrored() {
    let viewer = vec!["outer".to_owned(), "inner".to_owned()];
    assert!(guest_is_a_viewer(Some("inner".to_owned()), &viewer));
    assert!(guest_is_a_viewer(Some("outer".to_owned()), &viewer));
    assert!(!guest_is_a_viewer(Some("elsewhere".to_owned()), &viewer));
    assert!(!guest_is_a_viewer(None, &viewer));
    assert!(!guest_is_a_viewer(Some("inner".to_owned()), &[]));
}
