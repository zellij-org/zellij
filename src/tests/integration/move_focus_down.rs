use ::insta::assert_snapshot;

use crate::terminal_pane::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::get_output_frame_snapshots;
use crate::{start, Opt};

use crate::tests::utils::commands::{
    COMMAND_TOGGLE, MOVE_FOCUS, MOVE_FOCUS_DOWN, MOVE_FOCUS_UP, QUIT, RESIZE_DOWN, RESIZE_LEFT,
    SPLIT_HORIZONTALLY, SPLIT_VERTICALLY,
};

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
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS_UP,
        &MOVE_FOCUS_DOWN,
        &QUIT,
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
pub fn move_focus_down_to_the_largest_overlap() {
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
        &SPLIT_VERTICALLY,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS_UP,
        &MOVE_FOCUS_DOWN,
        &QUIT,
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
