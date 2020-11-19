use ::insta::assert_snapshot;
use ::nix::pty::Winsize;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::utils::get_output_frame_snapshots;
use crate::{start, Opt};

use crate::tests::utils::commands::{
    COMMAND_TOGGLE, MOVE_FOCUS, QUIT, RESIZE_LEFT, RESIZE_UP, SPLIT_HORIZONTALLY, SPLIT_VERTICALLY,
};

fn get_fake_os_input(fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn resize_left_with_pane_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        RESIZE_LEFT,
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
pub fn resize_left_with_pane_to_the_right() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // │█████│     │  ==resize=left==>  │███│       │
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_left_and_right() {
    // ┌─────┬─────┬─────┐                    ┌─────┬───┬───────┐
    // │     │█████│     │                    │     │███│       │
    // │     │█████│     │  ==resize=left==>  │     │███│       │
    // │     │█████│     │                    │     │███│       │
    // └─────┴─────┴─────┘                    └─────┴───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┤█████│  ==resize=left==>  ├───┤███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_left_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_right_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_left_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_right_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     ├─────┤                    │   ├───────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     ├─────┤                    │   ├───────┤
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        RESIZE_UP,
        RESIZE_UP,
        RESIZE_UP,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_UP,
        RESIZE_UP,
        RESIZE_LEFT,
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
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // ├─────┤     │                    ├───┤       │
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┤     │                    ├───┤       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        // TODO: combine with above
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        COMMAND_TOGGLE,
        COMMAND_TOGGLE,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        RESIZE_UP,
        RESIZE_UP,
        RESIZE_UP,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_UP,
        RESIZE_UP,
        RESIZE_LEFT,
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
