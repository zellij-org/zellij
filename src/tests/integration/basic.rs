use crate::panes::PositionAndSize;
use ::insta::assert_snapshot;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::commands::{
    COMMAND_TOGGLE, QUIT, SCROLL_DOWN, SCROLL_UP, SPAWN_TERMINAL, SPLIT_HORIZONTALLY,
    SPLIT_VERTICALLY, TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
};
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{start, CliArgs};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn starts_with_one_terminal() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&COMMAND_TOGGLE, &COMMAND_TOGGLE, &QUIT]);
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
pub fn split_terminals_vertically() {
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
pub fn split_terminals_horizontally() {
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
pub fn split_largest_terminal() {
    // this finds the largest pane and splits along its longest edge (vertically or horizontally)
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
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
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
pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let fake_win_size = PositionAndSize {
        columns: 8,
        rows: 20,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_VERTICALLY,
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
pub fn cannot_split_terminals_horizontally_when_active_terminal_is_too_small() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 4,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
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
pub fn cannot_split_largest_terminal_when_there_is_no_room() {
    let fake_win_size = PositionAndSize {
        columns: 8,
        rows: 4,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPAWN_TERMINAL,
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
pub fn scrolling_up_inside_a_pane() {
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
        &SCROLL_UP,
        &SCROLL_UP,
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
pub fn scrolling_down_inside_a_pane() {
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
        &SCROLL_UP,
        &SCROLL_UP,
        &SCROLL_DOWN,
        &SCROLL_DOWN,
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
pub fn max_panes() {
    // with the --max-panes option, we only allow a certain amount of panes on screen
    // simultaneously, new panes beyond this limit will close older panes on screen
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
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &QUIT,
    ]);
    let mut opts = CliArgs::default();
    opts.max_panes = Some(4);
    start(Box::new(fake_input_output.clone()), opts);
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
pub fn toggle_focused_pane_fullscreen() {
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
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &SPAWN_TERMINAL,
        &TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        &QUIT,
    ]);
    let mut opts = CliArgs::default();
    opts.max_panes = Some(4);
    start(Box::new(fake_input_output.clone()), opts);
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
