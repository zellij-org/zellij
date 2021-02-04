use insta::assert_snapshot;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{panes::PositionAndSize, tests::utils::commands::CLOSE_FOCUSED_PANE};
use crate::{start, CliArgs};

use crate::tests::utils::commands::{
    CLOSE_TAB, COMMAND_TOGGLE, NEW_TAB, QUIT, SPLIT_HORIZONTALLY, SWITCH_NEXT_TAB, SWITCH_PREV_TAB,
    TOGGLE_ACTIVE_TERMINAL_FULLSCREEN
};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(*fake_win_size)
}

#[test]
pub fn open_new_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn switch_to_prev_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SWITCH_PREV_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn switch_to_next_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SWITCH_NEXT_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &CLOSE_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_last_pane_in_a_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &NEW_TAB,
        &SPLIT_HORIZONTALLY,
        &CLOSE_FOCUSED_PANE,
        &CLOSE_FOCUSED_PANE,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_the_middle_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SWITCH_PREV_TAB,
        &CLOSE_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_the_tab_that_has_a_pane_in_fullscreen() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &SWITCH_PREV_TAB,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        &CLOSE_TAB,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn closing_last_tab_exits_the_app() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &NEW_TAB,
        &CLOSE_TAB,
        &CLOSE_TAB,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit = get_next_to_last_snapshot(snapshots)
        .expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}
