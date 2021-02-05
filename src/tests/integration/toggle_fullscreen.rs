use insta::assert_snapshot;

use crate::panes::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{start, CliArgs};

use crate::tests::utils::commands::{
    CLOSE_FOCUSED_PANE, COMMAND_TOGGLE, MOVE_FOCUS, QUIT, SPLIT_HORIZONTALLY, SPLIT_VERTICALLY,
    TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(*fake_win_size)
}

#[test]
pub fn adding_new_terminal_in_fullscreen() {
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
        &SPLIT_VERTICALLY,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        &SPLIT_HORIZONTALLY,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn move_focus_is_disabled_in_fullscreen() {
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
        &SPLIT_VERTICALLY,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        &MOVE_FOCUS,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
