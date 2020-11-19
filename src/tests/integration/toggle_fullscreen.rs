use ::insta::assert_snapshot;

use crate::terminal_pane::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::get_output_frame_snapshots;
use crate::{start, Opt};

use crate::tests::utils::commands::{
    CLOSE_FOCUSED_PANE, MOVE_FOCUS, QUIT, SPLIT_HORIZONTALLY, SPLIT_VERTICALLY,
    TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
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
        SPLIT_VERTICALLY,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        SPLIT_HORIZONTALLY,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
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
        SPLIT_VERTICALLY,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        MOVE_FOCUS,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
