use insta::assert_snapshot;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{panes::PositionAndSize, tests::utils::commands::CLOSE_PANE_IN_PANE_MODE};
use crate::{start, CliArgs};

use crate::common::input::config::Config;
use crate::tests::utils::commands::{
    CLOSE_TAB_IN_TAB_MODE, NEW_TAB_IN_TAB_MODE, PANE_MODE, QUIT, SPLIT_DOWN_IN_PANE_MODE,
    SWITCH_NEXT_TAB_IN_TAB_MODE, SWITCH_PREV_TAB_IN_TAB_MODE, TAB_MODE,
    TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE,
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
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn switch_to_prev_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &SWITCH_PREV_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn switch_to_next_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &SWITCH_NEXT_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &CLOSE_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_last_pane_in_a_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &CLOSE_PANE_IN_PANE_MODE,
        &CLOSE_PANE_IN_PANE_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_the_middle_tab() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &SWITCH_PREV_TAB_IN_TAB_MODE,
        &CLOSE_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn close_the_tab_that_has_a_pane_in_fullscreen() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &SWITCH_PREV_TAB_IN_TAB_MODE,
        &PANE_MODE,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE,
        &TAB_MODE,
        &CLOSE_TAB_IN_TAB_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}

#[test]
pub fn closing_last_tab_exits_the_app() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &TAB_MODE,
        &NEW_TAB_IN_TAB_MODE,
        &CLOSE_TAB_IN_TAB_MODE,
        &CLOSE_TAB_IN_TAB_MODE,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Config::default(),
    );

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    let snapshot_before_quit =
        get_next_to_last_snapshot(snapshots).expect("could not find snapshot");
    assert_snapshot!(snapshot_before_quit);
}
