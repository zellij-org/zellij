use super::*;
use crate::attention::program_status::parse;
use zellij_utils::attention::BlockedKind;

const PANE: PaneId = PaneId::Terminal(1);
const OTHER: PaneId = PaneId::Terminal(2);

fn report(body: &str) -> PaneSignal {
    let mut params: Vec<&[u8]> = vec![b"7501"];
    params.push(body.as_bytes());
    PaneSignal::ProgramStatus(parse(&params, false).expect("valid report"))
}

fn tracker() -> AttentionTracker {
    let mut tracker = AttentionTracker::new(AttentionConfig::default());
    tracker.sync_panes(
        &[(PANE, 0), (OTHER, 0)],
        &HashSet::new(),
        &HashSet::new(),
        1_000,
    );
    tracker
}

fn hidden(tracker: &mut AttentionTracker, now: u64) {
    tracker.sync_panes(
        &[(PANE, 0), (OTHER, 0)],
        &HashSet::new(),
        &HashSet::new(),
        now,
    );
}

fn visible(tracker: &mut AttentionTracker, now: u64) {
    let set: HashSet<PaneId> = [PANE].into_iter().collect();
    tracker.sync_panes(&[(PANE, 0), (OTHER, 0)], &set, &HashSet::new(), now);
}

fn focused(tracker: &mut AttentionTracker, now: u64) {
    let set: HashSet<PaneId> = [PANE].into_iter().collect();
    tracker.sync_panes(&[(PANE, 0), (OTHER, 0)], &set, &set, now);
}

fn status(tracker: &AttentionTracker) -> AttentionStatus {
    tracker.attention(&PANE).unwrap().status
}

fn frame(tracker: &AttentionTracker) -> Option<FrameMarker> {
    tracker.frame_marker(&PANE)
}

fn run_command(tracker: &mut AttentionTracker, text: &str, start: u64) {
    tracker.on_signals(
        PANE,
        vec![
            PaneSignal::PromptShown { aid: None },
            PaneSignal::InputStarted,
            PaneSignal::CommandStarted {
                command: Some(CommandText {
                    text: text.to_owned(),
                    source: CommandTextSource::Shell,
                }),
                aid: None,
            },
        ],
        start,
    );
}

fn finish(tracker: &mut AttentionTracker, exit_code: Option<i32>, err: Option<&str>, now: u64) {
    tracker.on_signals(
        PANE,
        vec![PaneSignal::CommandFinished {
            exit_code,
            err: err.map(|e| e.to_owned()),
            aid: None,
        }],
        now,
    );
}

#[test]
fn program_status_sequence_working_blocked_working_done() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=working:app=claude")], 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Working);
    tracker.on_signals(
        PANE,
        vec![report(
            "state=blocked:kind=permission:msg=QWxsb3cgZWRpdCB0byBtYWluLnJzPw==",
        )],
        3_000,
    );
    let attention = tracker.attention(&PANE).unwrap().clone();
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    assert_eq!(attention.text, "needs approval: Allow edit to main.rs?");
    assert_eq!(attention.detail.kind, Some(BlockedKind::Permission));
    assert_eq!(attention.source, Some(AttentionSource::ProgramStatus));
    assert_eq!(
        frame(&tracker).unwrap().text,
        "needs approval: Allow edit to main.rs?"
    );
    tracker.on_signals(PANE, vec![report("state=working")], 4_000);
    assert_eq!(status(&tracker), AttentionStatus::Working);
    tracker.on_signals(PANE, vec![report("state=done")], 5_000);
    assert_eq!(status(&tracker), AttentionStatus::Done);
    visible(&mut tracker, 6_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert!(frame(&tracker).is_some());
    focused(&mut tracker, 7_000);
    assert!(frame(&tracker).is_none());
}

#[test]
fn blocked_stays_after_being_seen_but_is_marked_seen() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=blocked:kind=question")], 2_000);
    visible(&mut tracker, 3_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    assert!(attention.seen);
}

#[test]
fn main_prompt_drops_transient_records_but_keeps_done() {
    let mut tracker = tracker();
    tracker.on_signals(
        PANE,
        vec![report("state=blocked"), report("state=done:id=job")],
        2_000,
    );
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert_eq!(tracker.program_status_records(&PANE).len(), 1);
    tracker.on_signals(PANE, vec![report("state=done")], 4_000);
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 5_000);
    assert_eq!(status(&tracker), AttentionStatus::Done);
}

#[test]
fn pane_exit_drops_transient_records() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=working")], 2_000);
    tracker.pane_exited(PANE, Some(0), 3_000);
    assert!(tracker.program_status_records(&PANE).is_empty());
}

#[test]
fn reset_clears_all_records() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=error")], 2_000);
    tracker.on_signals(PANE, vec![PaneSignal::Reset], 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn bell_needs_you_until_seen_and_frame_until_focused() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_000);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    assert_eq!(
        tracker.attention(&PANE).unwrap().source,
        Some(AttentionSource::Bell)
    );
    visible(&mut tracker, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert_eq!(frame(&tracker).unwrap().status, AttentionStatus::NeedsYou);
    focused(&mut tracker, 4_000);
    assert!(frame(&tracker).is_none());
}

#[test]
fn bell_on_focused_pane_is_seen_immediately() {
    let mut tracker = tracker();
    focused(&mut tracker, 1_500);
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert!(frame(&tracker).is_none());
}

#[test]
fn notification_needs_you_with_text() {
    let mut tracker = tracker();
    tracker.on_notification(PANE, "Build".to_owned(), "finished".to_owned(), 2_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    assert_eq!(attention.text, "Build: finished");
    assert_eq!(attention.source, Some(AttentionSource::Notification));
}

#[test]
fn failed_command_always_counts() {
    let mut tracker = tracker();
    run_command(&mut tracker, "cargo build", 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    finish(&mut tracker, Some(1), None, 2_500);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::Failed);
    assert_eq!(attention.text, "exited 1: cargo build");
    assert_eq!(attention.detail.exit_code, Some(1));
    let last = tracker.command_state(&PANE).unwrap().last.clone().unwrap();
    assert_eq!(last.result, CommandResult::Failed);
    assert_eq!(last.duration_ms, Some(500));
}

#[test]
fn err_option_means_failed() {
    let mut tracker = tracker();
    run_command(&mut tracker, "x", 2_000);
    finish(&mut tracker, Some(0), Some("CANCEL"), 2_100);
    assert_eq!(status(&tracker), AttentionStatus::Failed);
}

#[test]
fn success_counts_as_done_only_after_threshold() {
    let mut tracker = tracker();
    run_command(&mut tracker, "ls", 2_000);
    finish(&mut tracker, Some(0), None, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    run_command(&mut tracker, "make", 4_000);
    finish(&mut tracker, Some(0), None, 15_000);
    assert_eq!(status(&tracker), AttentionStatus::Done);
    assert_eq!(tracker.attention(&PANE).unwrap().text, "done · 11s: make");
}

#[test]
fn unknown_exit_is_done_after_threshold_never_failed() {
    let mut tracker = tracker();
    run_command(&mut tracker, "sleep 20", 2_000);
    finish(&mut tracker, None, None, 30_000);
    assert_eq!(status(&tracker), AttentionStatus::Done);
    let last = tracker.command_state(&PANE).unwrap().last.clone().unwrap();
    assert_eq!(last.result, CommandResult::Unknown);
}

#[test]
fn prompt_without_end_finishes_with_unknown_result() {
    let mut tracker = tracker();
    run_command(&mut tracker, "sleep 20", 2_000);
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 30_000);
    let last = tracker.command_state(&PANE).unwrap().last.clone().unwrap();
    assert_eq!(last.result, CommandResult::Unknown);
    assert_eq!(status(&tracker), AttentionStatus::Done);
}

#[test]
fn end_without_start_is_ignored() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 2_000);
    finish(&mut tracker, Some(1), None, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn needs_you_wins_over_failed() {
    let mut tracker = tracker();
    run_command(&mut tracker, "x", 2_000);
    finish(&mut tracker, Some(2), None, 2_100);
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_200);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    visible(&mut tracker, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn program_status_hides_shell_running_state() {
    let mut tracker = tracker();
    run_command(&mut tracker, "claude", 2_000);
    tracker.on_signals(PANE, vec![report("state=idle")], 2_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn mute_hides_everything_and_is_reported_as_muted() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_000);
    tracker.set_pane_muted(PANE, true, 2_100);
    let attention = tracker.attention(&PANE).unwrap().clone();
    assert_eq!(attention.muted, Some(MuteScope::Pane));
    assert_eq!(attention.effective_status(), AttentionStatus::Idle);
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    let frame = frame(&tracker).unwrap();
    assert!(frame.muted);
    assert_eq!(frame.text, "muted");
    let tab = tracker.tab_attention(0, &[PANE, OTHER]);
    assert_eq!(tab.counts.needs_you, 0);
    assert_eq!(tab.counts.muted, 1);
    assert_eq!(tab.status, AttentionStatus::Idle);
}

#[test]
fn tab_and_session_mute() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_000);
    tracker.set_tab_muted(0, true, 2_100);
    assert_eq!(
        tracker.attention(&PANE).unwrap().muted,
        Some(MuteScope::Tab)
    );
    tracker.set_tab_muted(0, false, 2_200);
    tracker.set_session_muted(true, 2_300);
    assert_eq!(
        tracker.attention(&PANE).unwrap().muted,
        Some(MuteScope::Session)
    );
    let session =
        tracker.session_attention(&[(0, "tab".to_owned(), vec![(PANE, "pane".to_owned())])]);
    assert_eq!(session.top, None);
    assert_eq!(session.counts.muted, 1);
    assert_eq!(session.items.len(), 1);
}

#[test]
fn ignore_output_hides_only_new_output() {
    let mut tracker = tracker();
    tracker.on_output(PANE, 10_000);
    assert_eq!(status(&tracker), AttentionStatus::NewOutput);
    tracker.set_pane_ignore_output(PANE, true, 10_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 10_200);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
}

#[test]
fn output_during_startup_is_not_new_output() {
    let mut tracker = tracker();
    tracker.on_output(PANE, 1_500);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn new_output_clears_when_visible_and_is_not_on_frames() {
    let mut tracker = tracker();
    tracker.on_output(PANE, 10_000);
    assert_eq!(status(&tracker), AttentionStatus::NewOutput);
    assert!(frame(&tracker).is_none());
    visible(&mut tracker, 11_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

fn steady_output(tracker: &mut AttentionTracker, from: u64, to: u64) {
    let mut t = from;
    while t <= to {
        tracker.on_output(PANE, t);
        t += 500;
    }
}

#[test]
fn idle_rule_fires_after_steady_output_then_silence() {
    let mut tracker = tracker();
    steady_output(&mut tracker, 10_000, 21_000);
    assert!(!tracker.take_idle_checks().is_empty());
    tracker.idle_check(PANE, 22_000);
    assert_eq!(status(&tracker), AttentionStatus::NewOutput);
    tracker.idle_check(PANE, 24_100);
    assert_eq!(status(&tracker), AttentionStatus::Quiet);
    assert!(frame(&tracker).is_none());
    let session = tracker.session_attention(&[(0, "t".to_owned(), vec![(PANE, "p".to_owned())])]);
    assert_eq!(session.counts.quiet, 1);
    assert_eq!(session.status, AttentionStatus::Idle);
    assert_eq!(session.top, None);
    visible(&mut tracker, 25_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn idle_rule_needs_steady_output() {
    let mut tracker = tracker();
    tracker.set_pane_ignore_output(PANE, true, 9_000);
    tracker.on_output(PANE, 10_000);
    tracker.on_output(PANE, 12_500);
    tracker.on_output(PANE, 15_000);
    tracker.on_output(PANE, 17_500);
    tracker.on_output(PANE, 20_100);
    tracker.idle_check(PANE, 23_200);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn idle_rule_does_not_fire_at_a_prompt() {
    let mut tracker = tracker();
    tracker.set_pane_ignore_output(PANE, true, 9_000);
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 9_500);
    steady_output(&mut tracker, 10_000, 21_000);
    tracker.idle_check(PANE, 24_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn idle_rule_does_not_fire_with_program_status() {
    let mut tracker = tracker();
    tracker.set_pane_ignore_output(PANE, true, 9_000);
    tracker.on_signals(PANE, vec![report("state=idle")], 9_500);
    steady_output(&mut tracker, 10_000, 21_000);
    tracker.idle_check(PANE, 24_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn idle_rule_scope_watched_only() {
    let mut config = AttentionConfig::default();
    config.idle_rule = Some(IdleRuleScope::Watched);
    let mut tracker = AttentionTracker::new(config);
    hidden(&mut tracker, 1_000);
    tracker.set_pane_ignore_output(PANE, true, 9_000);
    steady_output(&mut tracker, 10_000, 21_000);
    tracker.idle_check(PANE, 24_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn a_watch_stays_armed_and_fires_on_every_command_until_turned_off() {
    let mut tracker = tracker();
    run_command(&mut tracker, "make", 2_000);
    tracker.set_pane_watched(PANE, true, 2_100);
    assert!(tracker.pane_is_watched(&PANE));
    finish(&mut tracker, Some(0), None, 3_000);
    let attention = tracker.attention(&PANE).unwrap().clone();
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    assert!(attention.from_watch);
    assert_eq!(attention.source, Some(AttentionSource::Watch));
    assert_eq!(attention.text, "done · 1s: make");
    assert!(tracker.pane_is_watched(&PANE));
    visible(&mut tracker, 4_000);
    focused(&mut tracker, 4_100);
    assert!(tracker.attention(&PANE).unwrap().watched);
    hidden(&mut tracker, 4_200);
    run_command(&mut tracker, "make", 5_000);
    finish(&mut tracker, Some(0), None, 6_000);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    assert!(tracker.attention(&PANE).unwrap().from_watch);
    tracker.set_pane_watched(PANE, false, 6_100);
    assert!(!tracker.pane_is_watched(&PANE));
    visible(&mut tracker, 6_200);
    focused(&mut tracker, 6_300);
    hidden(&mut tracker, 6_400);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    run_command(&mut tracker, "make", 7_000);
    finish(&mut tracker, Some(0), None, 8_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn new_output_in_a_watched_pane_needs_you() {
    let mut tracker = tracker();
    tracker.set_pane_watched(PANE, true, 1_000);
    hidden(&mut tracker, 4_500);
    tracker.on_output(PANE, 5_000);
    tracker.on_output(OTHER, 5_000);
    let watched = tracker.attention(&PANE).unwrap();
    assert_eq!(watched.status, AttentionStatus::NeedsYou);
    assert!(watched.from_watch);
    assert_eq!(watched.text, "new output");
    assert!(frame(&tracker).is_some());
    assert_eq!(
        tracker.attention(&OTHER).unwrap().status,
        AttentionStatus::NewOutput
    );
    visible(&mut tracker, 6_000);
    focused(&mut tracker, 6_100);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert!(tracker.pane_is_watched(&PANE));
}

#[test]
fn watch_without_markers_fires_on_idle_rule_even_when_visible() {
    let mut tracker = tracker();
    tracker.set_pane_watched(PANE, true, 9_000);
    visible(&mut tracker, 9_500);
    steady_output(&mut tracker, 10_000, 21_000);
    tracker.idle_check(PANE, 24_100);
    assert!(tracker.pane_is_watched(&PANE));
    hidden(&mut tracker, 24_200);
    assert_eq!(tracker.attention(&PANE).unwrap().source, None);
    assert_eq!(frame(&tracker).unwrap().text, "went quiet");
}

#[test]
fn foreground_fallback_reports_unknown_result() {
    let mut tracker = tracker();
    tracker.foreground_changed(PANE, Some("cargo test".to_owned()), 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.foreground_changed(PANE, None, 20_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::Done);
    assert_eq!(attention.source, Some(AttentionSource::ForegroundProcess));
    assert_eq!(attention.text, "finished · 18s: cargo test");
}

#[test]
fn foreground_fallback_is_ignored_for_panes_with_markers() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::PromptShown { aid: None }], 1_500);
    tracker.foreground_changed(PANE, Some("vim".to_owned()), 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
}

#[test]
fn command_pane_exit() {
    let mut tracker = tracker();
    tracker.command_pane_started(PANE, Some("npm test".to_owned()), 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.pane_exited(PANE, Some(3), 2_500);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::Failed);
    assert_eq!(attention.source, Some(AttentionSource::CommandPane));
    assert!(frame(&tracker).is_none());
    tracker.command_pane_started(PANE, Some("npm test".to_owned()), 3_000);
    tracker.pane_exited(PANE, None, 3_100);
    assert_eq!(status(&tracker), AttentionStatus::Failed);
}

#[test]
fn events_are_emitted() {
    let mut tracker = tracker();
    tracker.take_events();
    run_command(&mut tracker, "ls", 2_000);
    finish(&mut tracker, Some(0), None, 2_100);
    run_command(&mut tracker, "false", 2_200);
    finish(&mut tracker, Some(1), None, 2_300);
    let events = tracker.take_events();
    assert!(events.contains(&AttentionEvent::PromptShown(PANE)));
    assert!(events.iter().any(|e| matches!(
        e,
        AttentionEvent::CommandStarted(PANE, Some(CommandText { text, .. })) if text == "ls"
    )));
    assert!(events.iter().any(|e| matches!(
        e,
        AttentionEvent::CommandFinished(
            PANE,
            CommandOutcome {
                result: CommandResult::Succeeded,
                ..
            }
        )
    )));
    assert!(events
        .iter()
        .any(|e| matches!(e, AttentionEvent::StatusChanged(PANE, _))));
}

#[test]
fn session_summary_picks_the_highest_item() {
    let mut tracker = tracker();
    tracker.on_signals(OTHER, vec![report("state=working")], 2_000);
    tracker.on_signals(PANE, vec![report("state=error:msg=Ym9vbQ")], 2_100);
    let session = tracker.session_attention(&[(
        0,
        "api".to_owned(),
        vec![(PANE, "claude".to_owned()), (OTHER, "build".to_owned())],
    )]);
    assert_eq!(session.status, AttentionStatus::Failed);
    assert_eq!(session.counts.failed, 1);
    assert_eq!(session.counts.working, 1);
    assert_eq!(session.top.unwrap().label(), "api › claude: failed: boom");
}

#[test]
fn a_watched_pane_is_listed_even_while_idle() {
    let mut tracker = tracker();
    tracker.add_pane(OTHER, 1_000);
    tracker.set_pane_watched(OTHER, true, 2_000);
    let session = tracker.session_attention(&[(
        0,
        "api".to_owned(),
        vec![(PANE, "claude".to_owned()), (OTHER, "build".to_owned())],
    )]);
    assert_eq!(session.items.len(), 1);
    assert_eq!(session.items[0].pane_id, OTHER);
    assert!(session.items[0].attention.watched);
    assert!(session.top.is_none());
    assert_eq!(session.counts, AttentionCounts::default());
}

#[test]
fn marking_seen_clears_past_events_like_being_visible() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![PaneSignal::Bell], 2_000);
    run_command(&mut tracker, "cargo test", 2_000);
    finish(&mut tracker, Some(1), None, 2_500);
    tracker.on_signals(OTHER, vec![PaneSignal::Bell], 2_600);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    assert!(tracker.mark_seen(PANE, 3_000));
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert!(frame(&tracker).is_none());
    assert_eq!(
        tracker.attention(&OTHER).unwrap().status,
        AttentionStatus::NeedsYou
    );
    assert!(!tracker.mark_seen(PaneId::Terminal(99), 3_000));
    tracker.mark_all_seen(3_100);
    assert_eq!(
        tracker.attention(&OTHER).unwrap().status,
        AttentionStatus::Idle
    );
}

#[test]
fn marking_seen_dismisses_current_truths_until_the_program_reports_again() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=blocked:kind=permission")], 2_000);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
    tracker.mark_seen(PANE, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert!(frame(&tracker).is_none());
    tracker.on_signals(PANE, vec![report("state=working")], 4_000);
    assert_eq!(status(&tracker), AttentionStatus::Working);
    tracker.on_signals(PANE, vec![report("state=blocked:kind=question")], 5_000);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
}

#[test]
fn focusing_keeps_a_program_working_but_marking_it_seen_clears_it() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=working")], 2_000);
    focused(&mut tracker, 2_500);
    assert_eq!(status(&tracker), AttentionStatus::Working);
    hidden(&mut tracker, 2_800);
    tracker.mark_seen(PANE, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.on_signals(PANE, vec![report("state=done")], 15_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::Done);
    assert_eq!(attention.detail.duration_ms, Some(13_000));
}

#[test]
fn output_from_a_working_program_out_of_sight_keeps_it_working_until_done() {
    let mut tracker = tracker();
    visible(&mut tracker, 5_000);
    tracker.on_signals(PANE, vec![report("state=working:app=opencode")], 5_000);
    hidden(&mut tracker, 6_000);
    for at in [7_000, 8_000, 9_000] {
        tracker.on_output(PANE, at);
    }
    assert_eq!(tracker.unseen_output_since(&PANE), Some(7_000));
    assert_eq!(status(&tracker), AttentionStatus::Working);
    tracker.on_signals(PANE, vec![report("state=done:app=opencode")], 20_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::Done);
    assert!(!attention.seen);
    assert_eq!(attention.detail.duration_ms, Some(15_000));
    tracker.on_output(PANE, 21_000);
    assert_eq!(status(&tracker), AttentionStatus::Done);
    tracker.on_signals(PANE, vec![report("state=working")], 22_000);
    tracker.on_output(PANE, 23_000);
    assert_eq!(status(&tracker), AttentionStatus::Working);
}

#[test]
fn output_from_a_program_reporting_under_an_id_or_as_idle_is_not_new_output() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("id=task:state=working")], 5_000);
    tracker.on_signals(OTHER, vec![report("state=idle")], 5_000);
    hidden(&mut tracker, 6_000);
    for at in [7_000, 8_000, 9_000] {
        tracker.on_output(PANE, at);
        tracker.on_output(OTHER, at);
    }
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    assert_eq!(
        tracker.attention(&OTHER).unwrap().status,
        AttentionStatus::Idle
    );
    tracker.set_pane_watched(PANE, true, 10_000);
    tracker.on_output(PANE, 11_000);
    assert_eq!(status(&tracker), AttentionStatus::NeedsYou);
}

#[test]
fn being_visible_keeps_current_truths() {
    let mut tracker = tracker();
    tracker.on_signals(PANE, vec![report("state=blocked:kind=permission")], 2_000);
    visible(&mut tracker, 3_000);
    let attention = tracker.attention(&PANE).unwrap();
    assert_eq!(attention.status, AttentionStatus::NeedsYou);
    assert!(attention.seen);
}

#[test]
fn a_running_command_is_not_working_and_its_failure_still_counts() {
    let mut tracker = tracker();
    run_command(&mut tracker, "cargo build", 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.mark_seen(PANE, 3_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    finish(&mut tracker, Some(1), None, 4_000);
    assert_eq!(status(&tracker), AttentionStatus::Failed);
}

#[test]
fn unseen_output_start_is_known_until_seen() {
    let mut tracker = tracker();
    tracker.on_output(PANE, 10_000);
    assert_eq!(tracker.unseen_output_since(&PANE), Some(10_000));
    tracker.mark_seen(PANE, 11_000);
    assert_eq!(tracker.unseen_output_since(&PANE), None);
}

#[test]
fn programs_that_report_nothing_are_never_working() {
    let mut tracker = tracker();
    run_command(&mut tracker, "htop", 2_000);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    tracker.foreground_changed(PANE, Some("htop".to_owned()), 2_500);
    assert_eq!(status(&tracker), AttentionStatus::Idle);
    let session = tracker.session_attention(&[(0, "t".to_owned(), vec![(PANE, "p".to_owned())])]);
    assert_eq!(session.counts.working, 0);
    assert!(session.items.is_empty());
}
