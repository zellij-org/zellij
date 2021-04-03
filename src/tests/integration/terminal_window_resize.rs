use crate::panes::PositionAndSize;
use ::insta::assert_snapshot;

use crate::common::input::config::Config;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::commands::QUIT;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{start, CliArgs};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn window_width_decrease_with_one_pane() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&QUIT]);
    fake_input_output.add_sigwinch_event(PositionAndSize {
        columns: 90,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    });
    let opts = CliArgs::default();
    start(
        Box::new(fake_input_output.clone()),
        opts,
        Box::new(fake_input_output.clone()),
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
pub fn window_width_increase_with_one_pane() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&QUIT]);
    fake_input_output.add_sigwinch_event(PositionAndSize {
        columns: 141,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    });
    let opts = CliArgs::default();
    start(
        Box::new(fake_input_output.clone()),
        opts,
        Box::new(fake_input_output.clone()),
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
pub fn window_height_increase_with_one_pane() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&QUIT]);
    fake_input_output.add_sigwinch_event(PositionAndSize {
        columns: 121,
        rows: 30,
        x: 0,
        y: 0,
        ..Default::default()
    });
    let opts = CliArgs::default();
    start(
        Box::new(fake_input_output.clone()),
        opts,
        Box::new(fake_input_output.clone()),
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
pub fn window_width_and_height_decrease_with_one_pane() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&QUIT]);
    fake_input_output.add_sigwinch_event(PositionAndSize {
        columns: 90,
        rows: 10,
        x: 0,
        y: 0,
        ..Default::default()
    });
    let opts = CliArgs::default();
    start(
        Box::new(fake_input_output.clone()),
        opts,
        Box::new(fake_input_output.clone()),
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
