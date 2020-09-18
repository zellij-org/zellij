use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::start;
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn resize_down_with_pane_above() {
    // ┌───────────┐                  ┌───────────┐
    // │           │                  │           │
    // │           │                  │           │
    // ├───────────┤ ==resize=down==> │           │
    // │███████████│                  ├───────────┤
    // │███████████│                  │███████████│  
    // │███████████│                  │███████████│
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[2, 10, 17]); // split-horizontally, resize-down and quit (ctrl-b + ctrl-j + ctrl-q)
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_pane_below() {
    // ┌───────────┐                  ┌───────────┐
    // │███████████│                  │███████████│
    // │███████████│                  │███████████│
    // ├───────────┤ ==resize=down==> │███████████│
    // │           │                  ├───────────┤
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[2, 16, 10, 17]); // split-horizontally, change-focus, resize-down and quit (ctrl-b + ctrl-p + ctrl-j + ctrl-q)
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_above_and_below() {
    // ┌───────────┐                  ┌───────────┐
    // │           │                  │           │
    // │           │                  │           │
    // ├───────────┤                  │           │
    // │███████████│ ==resize=down==> ├───────────┤
    // │███████████│                  │███████████│
    // ├───────────┤                  ├───────────┤
    // │           │                  │           │
    // │           │                  │           │
    // └───────────┘                  └───────────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    // split-horizontally * 2, change-focus * 2, resize-down and quit (ctrl-n * 2 + ctrl-p * 2 + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 2, 16, 16, 10, 17]);
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_multiple_panes_above() {
    //
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┴─────┤  ==resize=down==>  │     │     │
    // │███████████│                    ├─────┴─────┤
    // │███████████│                    │███████████│
    // └───────────┘                    └───────────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 16, 14, 16, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=down==>  ├─────┤     │
    // │     │█████│                    │     ├─────┤
    // │     │█████│                    │     │█████│
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p * 3, ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 16, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │█████│                    │     │█████│
    // │     │█████│                    │     │█████│
    // ├─────┼─────┤  ==resize=down==>  ├─────┤█████│
    // │     │     │                    │     ├─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p + ctrl-p + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_above_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=down==>  │     ├─────┤
    // │█████│     │                    ├─────┤     │
    // │█████│     │                    │█████│     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_below_aligned_right_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │█████│     │                    │█████│     │
    // │█████│     │                    │█████│     │
    // ├─────┼─────┤  ==resize=down==>  │█████├─────┤
    // │     │     │                    ├─────┤     │
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // split-vertically, split_horizontally, change-focus, split-horizontally, resize-right and quit
    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p, ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                    ┌───┬───┬───┐
    // │   │   │   │                    │   │   │   │
    // │   │   │   │                    │   │   │   │
    // ├───┼───┼───┤  ==resize=down==>  ├───┤   ├───┤
    // │   │███│   │                    │   ├───┤   │
    // │   │███│   │                    │   │███│   │
    // └───┴───┴───┘                    └───┴───┴───┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n * 2 + ctrl-b + ctrl-p + ctrl-b + ctrl-p * 2 + ctrl-b + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 14, 2, 16, 2, 16, 16, 2, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_and_right_with_current_pane() {
    // ┌───┬───┬───┐                    ┌───┬───┬───┐
    // │   │███│   │                    │   │███│   │
    // │   │███│   │                    │   │███│   │
    // ├───┼───┼───┤  ==resize=down==>  ├───┤███├───┤
    // │   │   │   │                    │   ├───┤   │
    // │   │   │   │                    │   │   │   │
    // └───┴───┴───┘                    └───┴───┴───┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n * 2 + ctrl-b + ctrl-p + ctrl-b + ctrl-p * 2 + ctrl-b + ctrl-p * 2 + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 14, 2, 16, 2, 16, 16, 2, 16, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_above_aligned_left_and_right_with_panes_to_the_left_and_right() {
    // ┌─┬───────┬─┐                    ┌─┬───────┬─┐
    // │ │       │ │                    │ │       │ │
    // │ │       │ │                    │ │       │ │
    // ├─┼─┬───┬─┼─┤  ==resize=down==>  ├─┤       ├─┤
    // │ │ │███│ │ │                    │ ├─┬───┬─┤ │
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // └─┴─┴───┴─┴─┘                    └─┴─┴───┴─┴─┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n * 2 + ctrl-p + ctrl-h * 3 + ctrl-b + ctrl-p * 3 + ctrl-b + ctrl-p * 2 + ctrl-b +
    // ctrl-n * 2 + ctrl-p * 6 + ctrl-h * 2 + ctrl-p + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 14, 16, 8, 8, 8, 2, 16, 16, 16, 2, 16, 16, 2, 14, 14, 16, 16, 16, 16, 16, 16, 8, 8, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_down_with_panes_below_aligned_left_and_right_with_to_the_left_and_right() {
    // ┌─┬─┬───┬─┬─┐                    ┌─┬─┬───┬─┬─┐
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // │ │ │███│ │ │                    │ │ │███│ │ │
    // ├─┼─┴───┴─┼─┤  ==resize=down==>  ├─┤ │███│ ├─┤
    // │ │       │ │                    │ ├─┴───┴─┤ │
    // │ │       │ │                    │ │       │ │
    // └─┴───────┴─┘                    └─┴───────┴─┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n * 2 + ctrl-p + ctrl-h * 3 + ctrl-b + ctrl-p * 3 + ctrl-b + ctrl-p * 2 + ctrl-b +
    // ctrl-p * 2 +
    // ctrl-n * 2 + ctrl-p * 2 + ctrl-h * 2 + ctrl-p * 5 + ctrl-j + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 14, 16, 8, 8, 8, 2, 16, 16, 16, 2, 16, 16, 2, 16, 16, 14, 14, 16, 16, 8, 8, 16, 16, 16, 16, 16, 10, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
