use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::{start, Opt};
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

use crate::tests::utils::commands::{
    SPLIT_HORIZONTALLY,
    SPLIT_VERTICALLY,
    RESIZE_DOWN,
    RESIZE_UP,
    MOVE_FOCUS,
    RESIZE_LEFT,
    QUIT,
    CLOSE_FOCUSED_PANE,
};

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn close_pane_with_another_pane_above_it() {
    // ┌───────────┐            ┌───────────┐
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // ├───────────┤ ==close==> │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // └───────────┘            └───────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_another_pane_below_it() {
    // ┌───────────┐            ┌───────────┐
    // │███████████│            │xxxxxxxxxxx│
    // │███████████│            │xxxxxxxxxxx│
    // ├───────────┤ ==close==> │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // │xxxxxxxxxxx│            │xxxxxxxxxxx│
    // └───────────┘            └───────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_another_pane_to_the_left() {
    // ┌─────┬─────┐            ┌──────────┐
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│ ==close==> │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_another_pane_to_the_right() {
    // ┌─────┬─────┐            ┌──────────┐
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│ ==close==> │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_above_it() {
    // ┌─────┬─────┐            ┌─────┬─────┐
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // ├─────┴─────┤ ==close==> │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // └───────────┘            └─────┴─────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_below_it() {
    // ┌───────────┐            ┌─────┬─────┐
    // │███████████│            │xxxxx│xxxxx│
    // │███████████│            │xxxxx│xxxxx│
    // ├─────┬─────┤ ==close==> │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // │xxxxx│xxxxx│            │xxxxx│xxxxx│
    // └─────┴─────┘            └─────┴─────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐            ┌──────────┐
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // ├─────┤█████│ ==close==> ├──────────┤
    // │xxxxx│█████│            │xxxxxxxxxx│
    // │xxxxx│█████│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_right() {
    // ┌─────┬─────┐            ┌──────────┐
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████├─────┤ ==close==> ├──────────┤
    // │█████│xxxxx│            │xxxxxxxxxx│
    // │█████│xxxxx│            │xxxxxxxxxx│
    // └─────┴─────┘            └──────────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_above_it_away_from_screen_edges() {
    // ┌───┬───┬───┬───┐            ┌───┬───┬───┬───┐
    // │xxx│xxx│xxx│xxx│            │xxx│xxx│xxx│xxx│
    // ├───┤xxx│xxx├───┤            ├───┤xxx│xxx├───┤
    // │xxx├───┴───┤xxx│ ==close==> │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // └───┴───────┴───┘            └───┴───┴───┴───┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        SPLIT_VERTICALLY,
        RESIZE_UP,
        MOVE_FOCUS,
        RESIZE_UP,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_below_it_away_from_screen_edges() {
    // ┌───┬───────┬───┐            ┌───┬───┬───┬───┐
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx│███████│xxx│            │xxx│xxx│xxx│xxx│
    // │xxx├───┬───┤xxx│ ==close==> │xxx│xxx│xxx│xxx│
    // ├───┤xxx│xxx├───┤            ├───┤xxx│xxx├───┤
    // │xxx│xxx│xxx│xxx│            │xxx│xxx│xxx│xxx│
    // └───┴───┴───┴───┘            └───┴───┴───┴───┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        SPLIT_VERTICALLY,
        RESIZE_DOWN,
        MOVE_FOCUS,
        RESIZE_DOWN,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_left_away_from_screen_edges() {
    // ┌────┬──────┐            ┌────┬──────┐
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // ├────┴┬─────┤            ├────┴──────┤
    // │xxxxx│█████│            │xxxxxxxxxxx│
    // ├─────┤█████│ ==close==> ├───────────┤
    // │xxxxx│█████│            │xxxxxxxxxxx│
    // ├────┬┴─────┤            ├────┬──────┤
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // └────┴──────┘            └────┴──────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        RESIZE_LEFT,
        MOVE_FOCUS,
        RESIZE_LEFT,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn close_pane_with_multiple_panes_to_the_right_away_from_screen_edges() {
    // ┌────┬──────┐            ┌────┬──────┐
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // ├────┴┬─────┤            ├────┴──────┤
    // │█████│xxxxx│            │xxxxxxxxxxx│
    // │█████├─────┤ ==close==> ├───────────┤
    // │█████│xxxxx│            │xxxxxxxxxxx│
    // ├────┬┴─────┤            ├────┬──────┤
    // │xxxx│xxxxxx│            │xxxx│xxxxxx│
    // └────┴──────┘            └────┴──────┘
    // █ == pane being closed
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        SPLIT_HORIZONTALLY,
        RESIZE_LEFT,
        MOVE_FOCUS,
        RESIZE_LEFT,
        MOVE_FOCUS,
        MOVE_FOCUS,
        SPLIT_HORIZONTALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        MOVE_FOCUS,
        CLOSE_FOCUSED_PANE,
        QUIT,
    ]);
    start(Box::new(fake_input_output.clone()), Opt::default());

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
