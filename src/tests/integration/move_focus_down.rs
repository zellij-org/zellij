use ::insta::assert_snapshot;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::start;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::CliArgs;
use zellij_utils::pane_size::PositionAndSize;

use crate::tests::utils::commands::{
    MOVE_FOCUS_DOWN_IN_PANE_MODE, MOVE_FOCUS_UP_IN_PANE_MODE, PANE_MODE, QUIT,
    SPLIT_DOWN_IN_PANE_MODE, SPLIT_RIGHT_IN_PANE_MODE,
};
use zellij_utils::input::config::Config;

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(*fake_win_size)
}

#[test]
pub fn move_focus_down() {
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
        &MOVE_FOCUS_UP_IN_PANE_MODE,
        &MOVE_FOCUS_DOWN_IN_PANE_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
        Config::default(),
        None,
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
pub fn move_focus_down_to_the_most_recently_used_pane() {
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_UP_IN_PANE_MODE,
        &MOVE_FOCUS_DOWN_IN_PANE_MODE,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
        Config::default(),
        None,
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
