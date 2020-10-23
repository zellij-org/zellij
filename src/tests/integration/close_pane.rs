use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::start;
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

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
    fake_input_output.add_terminal_input(&[2, 24, 17]); // split-horizontally, close focused pane and quit (ctrl-b + ctrl-x + ctrl-q)
    start(Box::new(fake_input_output.clone()));

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
    fake_input_output.add_terminal_input(&[2, 16, 24, 17]); // split-horizontally, change-focus, close focused pane and quit (ctrl-b + ctrl-p + ctrl-x + ctrl-q)
    start(Box::new(fake_input_output.clone()));

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
    fake_input_output.add_terminal_input(&[14, 24, 17]); // split-vertically, close focused pane and quit (ctrl-n + ctrl-x + ctrl-q)
    start(Box::new(fake_input_output.clone()));

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
    fake_input_output.add_terminal_input(&[14, 16, 24, 17]); // split-vertically, change-focus, close focused pane and quit (ctrl-n + ctrl-p + ctrl-x + ctrl-q)
    start(Box::new(fake_input_output.clone()));

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
    // split-horizontally, change-focus, split-vertically, change-focus * 2, close focused pane and quit
    // (ctrl-b + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 16, 14, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-horizontally, split-vertically, change-focus, change-focus, close focused pane and quit
    // (ctrl-b + ctrl-n + ctrl-p + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 14, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-vertically, change-focus, split-horizontally, change-focus * 2, close focused pane and quit
    // (ctrl-n + ctrl-p + ctrl-b + ctrl-p * 2 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 16, 2, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-vertically, change-focus, split-horizontally, change-focus * 2, close focused pane and quit
    // (ctrl-n + ctrl-p + ctrl-b + ctrl-p * 2 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-horizontally, split-vertically * 2, change-focus, split-vertically * 2, resize-up,
    // change-focus, resize-up, close focuse * 4, split-vertically, change-focus * 3, close-focused-pane and quit
    //
    // (ctrl-b + ctrl-n + ctrl-n + ctrl-p + ctrl-n + ctrl-n + ctrl-k + ctrl-p + ctrl-k + ctrl-p * 4
    // + ctrl-n + ctrl-p * 3 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 14, 14, 16, 14, 14, 11, 16, 11, 16, 16, 16, 16, 14, 16, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-horizontally, split-vertically * 2, change-focus, split-vertically * 2,
    // resize-down, change-focus, resize-down, change-focus * 2, split-vertically, change-focus * 5, close-focused-pane and quit
    //
    // (ctrl-b + ctrl-n + ctrl-n + ctrl-p + ctrl-n + ctrl-n + ctrl-j + ctrl-p + ctrl-j + ctrl-p * 2
    // + ctrl-n + ctrl-p * 5 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 14, 14, 16, 14, 14, 10, 16, 10, 16, 16, 14, 16, 16, 16, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-vertically, split-horizontally * 2, change-focus, split-horizontally * 2, resize-left,
    // change-focus, resize-left, change-focus * 4, split-horizontally, change-focus * 3,
    // close-focused pane and quit
    //
    // (ctrl-n + ctrl-b + ctrl-b + ctrl-p + ctrl-b + ctrl-b + ctrl-h + ctrl-p + ctrl-h + ctrl-p * 4
    // + ctrl-b + ctrl-p * 3 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 2, 16, 2, 2, 8, 16, 8, 16, 16, 16, 16, 2, 16, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

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
    // split-vertically, split-horizontally * 2, change-focus, split-horizontally * 2, resize-left,
    // change-focus, resize-left, change-focus * 2, split-horizontally, change-focus * 5, close-focused-pane and quit
    //
    // (ctrl-n + ctrl-b + ctrl-b + ctrl-p + ctrl-b + ctrl-b + ctrl-h + ctrl-p + ctrl-h + ctrl-p * 2
    // + ctrl-b + ctrl-p * 5 + ctrl-x + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 2, 16, 2, 2, 8, 16, 8, 16, 16, 2, 16, 16, 16, 16, 16, 24, 17]);
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
