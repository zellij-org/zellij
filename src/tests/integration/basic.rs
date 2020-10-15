use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::start;
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn starts_with_one_terminal () {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn split_terminals_vertically() {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[14, 17]); // split-vertically and quit (ctrl-n + ctrl-q)
    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn split_terminals_horizontally() {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[2, 17]); // split-horizontally and quit (ctrl-b + ctrl-q)
    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_right_and_up_on_the_same_axis() {
    // this is a specific test to explicitly ensure that a tmux-like pane-container algorithm is not
    // implemented (this test can never pass with such an algorithm)
    //
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤ ==resize=right==> ├─────┴─┬───┤ ==resize-left==>
    // │█████│     │                   │███████│   │
    // └─────┴─────┘                   └───────┴───┘
    //
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   ├─────┤     │
    // ├─────┼─────┤ ==resize=up==>    │█████├─────┤
    // │█████│     │                   │█████│     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b + ctrl-n + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-l + ctrl-h + ctrl-k + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 14, 16, 14, 16, 16, 12, 8, 11, 17]);

    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn scrolling_inside_a_pane() {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[2, 14, 27, 27, 29, 29, 17]); // split-horizontally, split-vertically, scroll up twice, scroll down twice and quit (ctrl-b + ctrl+[ * 2 + ctrl+] * 2, ctrl-q)
    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
