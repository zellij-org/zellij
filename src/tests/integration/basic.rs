use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::{start, Opt};
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;
use crate::tests::utils::commands::{
    SPLIT_HORIZONTALLY,
    SPLIT_VERTICALLY,
    RESIZE_UP,
    MOVE_FOCUS,
    RESIZE_LEFT,
    RESIZE_RIGHT,
    SPAWN_TERMINAL,
    QUIT,
    SCROLL_UP,
    SCROLL_DOWN,
    TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
};

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn starts_with_one_terminal () {
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn split_terminals_vertically() {
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_VERTICALLY,
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
pub fn split_terminals_horizontally() {
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
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
pub fn split_largest_terminal () {
    // this finds the largest pane and splits along its longest edge (vertically or horizontally)
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
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
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    fake_input_output.add_terminal_input(&[
        SPLIT_HORIZONTALLY,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        SPLIT_VERTICALLY,
        MOVE_FOCUS,
        MOVE_FOCUS,
        RESIZE_RIGHT,
        RESIZE_LEFT,
        RESIZE_UP,
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
pub fn scrolling_inside_a_pane() {
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
        SCROLL_UP,
        SCROLL_UP,
        SCROLL_DOWN,
        SCROLL_DOWN,
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
pub fn max_panes () {
    // with the --max-panes option, we only allow a certain amount of panes on screen
    // simultaneously, new panes beyond this limit will close older panes on screen
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        QUIT
    ]);
    let mut opts = Opt::default();
    opts.max_panes = Some(4);
    start(Box::new(fake_input_output.clone()), opts);
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn toggle_focused_pane_fullscreen () {
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        SPAWN_TERMINAL,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        MOVE_FOCUS,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        MOVE_FOCUS,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        MOVE_FOCUS,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        TOGGLE_ACTIVE_TERMINAL_FULLSCREEN,
        QUIT
    ]);
    let mut opts = Opt::default();
    opts.max_panes = Some(4);
    start(Box::new(fake_input_output.clone()), opts);
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
