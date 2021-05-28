use ::insta::assert_snapshot;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::start;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::CliArgs;
use zellij_utils::pane_size::PositionAndSize;

use crate::tests::utils::commands::{
    MOVE_FOCUS_IN_PANE_MODE, PANE_MODE, QUIT, RESIZE_LEFT_IN_RESIZE_MODE, RESIZE_MODE,
    RESIZE_UP_IN_RESIZE_MODE, SLEEP, SPLIT_DOWN_IN_PANE_MODE, SPLIT_RIGHT_IN_PANE_MODE,
};
use zellij_utils::input::config::Config;

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(*fake_win_size)
}

#[test]
pub fn resize_left_with_pane_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_pane_to_the_right() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // │█████│     │  ==resize=left==>  │███│       │
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_left_and_right() {
    // ┌─────┬─────┬─────┐                    ┌─────┬───┬───────┐
    // │     │█████│     │                    │     │███│       │
    // │     │█████│     │  ==resize=left==>  │     │███│       │
    // │     │█████│     │                    │     │███│       │
    // └─────┴─────┴─────┘                    └─────┴───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┤█████│  ==resize=left==>  ├───┤███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_left_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_right_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_left_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_right_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SLEEP,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SLEEP,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     ├─────┤                    │   ├───────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     ├─────┤                    │   ├───────┤
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 40,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SLEEP,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // ├─────┤     │                    ├───┤       │
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┤     │                    ├───┤       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        // TODO: combine with above
        columns: 121,
        rows: 40,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &SPLIT_DOWN_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &MOVE_FOCUS_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_UP_IN_RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);

    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
pub fn cannot_resize_left_when_pane_to_the_left_is_at_minimum_width() {
    // ┌─┬─┐                    ┌─┬─┐
    // │ │█│                    │ │█│
    // │ │█│  ==resize=left==>  │ │█│
    // │ │█│                    │ │█│
    // └─┴─┘                    └─┴─┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        columns: 9,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &PANE_MODE,
        &SPLIT_RIGHT_IN_PANE_MODE,
        &RESIZE_MODE,
        &RESIZE_LEFT_IN_RESIZE_MODE,
        &SLEEP,
        &QUIT,
    ]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
        Box::new(fake_input_output.clone()),
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
