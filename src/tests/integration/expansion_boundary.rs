use insta::assert_snapshot;
use std::path::PathBuf;

use crate::panes::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::commands::{
    COMMAND_TOGGLE, ESC, PANE_MODE, QUIT, RESIZE_DOWN_IN_RESIZE_MODE, RESIZE_MODE,
    SPAWN_TERMINAL_IN_PANE_MODE, TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE,
};
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{start, CliArgs};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn new_panes_are_open_inside_expansion_border() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 50,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &PANE_MODE,
        &SPAWN_TERMINAL_IN_PANE_MODE,
        &QUIT,
    ]);
    let mut opts = CliArgs::default();
    opts.layout = Some(PathBuf::from(
        "src/tests/fixtures/layouts/expansion-boundary-in-the-middle.yaml",
    ));

    start(Box::new(fake_input_output.clone()), opts);
    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);

    let next_to_last_snapshot = get_next_to_last_snapshot(snapshots).unwrap();
    assert_snapshot!(next_to_last_snapshot);
}

#[test]
pub fn resize_pane_inside_expansion_border() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 50,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &PANE_MODE,
        &SPAWN_TERMINAL_IN_PANE_MODE,
        &ESC,
        &RESIZE_MODE,
        &RESIZE_DOWN_IN_RESIZE_MODE,
        &QUIT,
    ]);
    let mut opts = CliArgs::default();
    opts.layout = Some(PathBuf::from(
        "src/tests/fixtures/layouts/expansion-boundary-in-the-middle.yaml",
    ));

    start(Box::new(fake_input_output.clone()), opts);
    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);

    let next_to_last_snapshot = get_next_to_last_snapshot(snapshots).unwrap();
    assert_snapshot!(next_to_last_snapshot);
}

#[test]
pub fn toggling_fullcsreen_in_expansion_border_expands_only_until_border() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 50,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &PANE_MODE,
        &SPAWN_TERMINAL_IN_PANE_MODE,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE,
        &QUIT,
    ]);
    let mut opts = CliArgs::default();
    opts.layout = Some(PathBuf::from(
        "src/tests/fixtures/layouts/expansion-boundary-in-the-middle.yaml",
    ));

    start(Box::new(fake_input_output.clone()), opts);
    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);

    let next_to_last_snapshot = get_next_to_last_snapshot(snapshots).unwrap();
    assert_snapshot!(next_to_last_snapshot);
}

// TODO:
// * fullscreen with expansion boundary
