use ::insta::assert_snapshot;

use crate::panes::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::get_output_frame_snapshots;
use crate::{start, CliArgs};

use crate::tests::utils::commands::{
    COMMAND_TOGGLE, MOVE_FOCUS, QUIT, RESIZE_LEFT, RESIZE_UP, SPLIT_HORIZONTALLY, SPLIT_VERTICALLY,
};

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(*fake_win_size)
}

#[test]
pub fn resize_up_with_pane_above() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // │           │                ├───────────┤
    // ├───────────┤ ==resize=up==> │███████████│
    // │███████████│                │███████████│
    // │███████████│                │███████████│
    // └───────────┘                └───────────┘
    // █ == focused pane
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
        &RESIZE_UP,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_pane_below() {
    // ┌───────────┐                ┌───────────┐
    // │███████████│                │███████████│
    // │███████████│                ├───────────┤
    // ├───────────┤ ==resize=up==> │           │
    // │           │                │           │
    // │           │                │           │
    // └───────────┘                └───────────┘
    // █ == focused pane
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
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_above_and_below() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // │           │                ├───────────┤
    // ├───────────┤                │███████████│
    // │███████████│ ==resize=up==> │███████████│
    // │███████████│                │███████████│
    // ├───────────┤                ├───────────┤
    // │           │                │           │
    // │           │                │           │
    // └───────────┘                └───────────┘
    // █ == focused pane
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
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_multiple_panes_above() {
    //
    // ┌─────┬─────┐                 ┌─────┬─────┐
    // │     │     │                 ├─────┴─────┤
    // ├─────┴─────┤  ==resize=up==> │███████████│
    // │███████████│                 │███████████│
    // └───────────┘                 └───────────┘
    // █ == focused pane
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
        &MOVE_FOCUS,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_above_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │     │                  │     ├─────┤
    // ├─────┼─────┤  ==resize=up==>  ├─────┤█████│
    // │     │█████│                  │     │█████│
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_below_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │█████│                  │     │█████│
    // │     │█████│                  │     ├─────┤
    // ├─────┼─────┤  ==resize=up==>  ├─────┤     │
    // │     │     │                  │     │     │
    // │     │     │                  │     │     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_above_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │     │     │                  │     │     │
    // │     │     │                  ├─────┤     │
    // ├─────┼─────┤  ==resize=up==>  │█████├─────┤
    // │█████│     │                  │█████│     │
    // │█████│     │                  │█████│     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_below_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                  ┌─────┬─────┐
    // │█████│     │                  │█████│     │
    // │█████│     │                  ├─────┤     │
    // ├─────┼─────┤  ==resize=up==>  │     ├─────┤
    // │     │     │                  │     │     │
    // │     │     │                  │     │     │
    // └─────┴─────┘                  └─────┴─────┘
    // █ == focused pane
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
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_above_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                  ┌───┬───┬───┐
    // │   │   │   │                  │   │   │   │
    // │   │   │   │                  │   ├───┤   │
    // ├───┼───┼───┤  ==resize=up==>  ├───┤███├───┤
    // │   │███│   │                  │   │███│   │
    // │   │███│   │                  │   │███│   │
    // └───┴───┴───┘                  └───┴───┴───┘
    // █ == focused pane
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
        &SPLIT_VERTICALLY,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_below_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                  ┌───┬───┬───┐
    // │   │███│   │                  │   │███│   │
    // │   │███│   │                  │   ├───┤   │
    // ├───┼───┼───┤  ==resize=up==>  ├───┤   ├───┤
    // │   │   │   │                  │   │   │   │
    // │   │   │   │                  │   │   │   │
    // └───┴───┴───┘                  └───┴───┴───┘
    // █ == focused pane
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
        &SPLIT_VERTICALLY,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_above_aligned_left_and_right_with_panes_to_the_left_and_right() {
    // ┌─┬───────┬─┐                  ┌─┬───────┬─┐
    // │ │       │ │                  │ │       │ │
    // │ │       │ │                  │ ├─┬───┬─┤ │
    // ├─┼─┬───┬─┼─┤  ==resize=up==>  ├─┤ │███│ ├─┤
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // └─┴─┴───┴─┴─┘                  └─┴─┴───┴─┴─┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 40,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_VERTICALLY,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &SPLIT_VERTICALLY,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn resize_up_with_panes_below_aligned_left_and_right_with_to_the_left_and_right() {
    // ┌─┬─┬───┬─┬─┐                  ┌─┬─┬───┬─┬─┐
    // │ │ │███│ │ │                  │ │ │███│ │ │
    // │ │ │███│ │ │                  │ ├─┴───┴─┤ │
    // ├─┼─┴───┴─┼─┤  ==resize=up==>  ├─┤       ├─┤
    // │ │       │ │                  │ │       │ │
    // │ │       │ │                  │ │       │ │
    // └─┴───────┴─┘                  └─┴───────┴─┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 40,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_VERTICALLY,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_HORIZONTALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &SPLIT_VERTICALLY,
        &SPLIT_VERTICALLY,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &MOVE_FOCUS,
        &RESIZE_LEFT,
        &RESIZE_LEFT,
        &RESIZE_UP,
        &QUIT,
    ]);

    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
pub fn cannot_resize_up_when_pane_above_is_at_minimum_height() {
    // ┌───────────┐                ┌───────────┐
    // │           │                │           │
    // ├───────────┤ ==resize=up==> ├───────────┤
    // │███████████│                │███████████│
    // └───────────┘                └───────────┘
    // █ == focused pane
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 5,
        x: 0,
        y: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        &COMMAND_TOGGLE,
        &COMMAND_TOGGLE,
        &SPLIT_HORIZONTALLY,
        &RESIZE_UP,
        &QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), CliArgs::default());

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
