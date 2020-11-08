use ::nix::pty::Winsize;
use ::insta::assert_snapshot;
use ::std::collections::HashMap;

use crate::{start, Opt};
use crate::tests::possible_tty_inputs::Bytes;
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

use crate::tests::utils::commands::QUIT;

/*
 * These tests are general compatibility tests for non-trivial scenarios running in the terminal.
 * They use fake TTY input replicated from these scenarios (and so don't actually interact with the
 * OS).
 *
 * They work like this:
 * - receive fake TTY input containing various VTE instructions.
 * - run that output through mosaic so it interprets it and creates its state based on it
 * - read that state into a Human-readable snapshot and compare it to the expected snapshot for
 * this scenario.
 *
 */

fn get_fake_os_input (fake_win_size: &Winsize, fixture_name: &str) -> FakeInputOutput {
    let mut tty_inputs = HashMap::new();
    let fixture_bytes = Bytes::from_file_in_fixtures(&fixture_name);
    tty_inputs.insert(fake_win_size.ws_col, fixture_bytes);
    FakeInputOutput::new(fake_win_size.clone()).with_tty_inputs(tty_inputs)
}

#[test]
pub fn run_bandwhich_from_fish_shell() {
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "fish_and_bandwhich";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn fish_tab_completion_options() {
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "fish_tab_completion_options";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn fish_select_tab_completion_options() {
    // the difference between this and the previous test is that here we press <TAB>
    // twice, meaning the selection moves between the options and the command line
    // changes.
    // this is not clearly seen in the snapshot because it does not include styles,
    // but we can see the command line change and the cursor staying in place
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "fish_select_tab_completion_options";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn vim_scroll_region_down () {
    // here we test a case where vim defines the scroll region as lesser than the screen row count
    // and then scrolls down
    // the region is defined here by vim as 1-26 (there are 28 rows)
    // then the cursor is moved to line 26 and a new line is added
    // what should happen is that the first line in the scroll region (1) is deleted
    // and an empty line is inserted in the last scroll region line (26)
    // this tests also has other steps afterwards that fills the line with the next line in the
    // file
    // experience appear to the user
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "vim_scroll_region_down";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    // fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    fake_input_output.add_terminal_input(&[QUIT]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn vim_ctrl_d() {
    // in vim ctrl-d moves down half a page
    // in this case, it sends the terminal the csi 'M' directive, which tells it to delete X (13 in
    // this case) lines inside the scroll region and push the other lines up
    // what happens here is that 13 lines are deleted and instead 13 empty lines are added at the
    // end of the scroll region
    // vim makes sure to fill these empty lines with the rest of the file
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "vim_ctrl_d";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn vim_ctrl_u() {
    // in vim ctrl-u moves up half a page
    // in this case, it sends the terminal the csi 'L' directive, which tells it to insert X (13 in
    // this case) lines at the cursor, pushing away (deleting) the last line in the scroll region
    // this causes the effect of scrolling up X lines (vim replaces the lines with the ones in the
    // file above the current content)
    let fake_win_size = Winsize {
        ws_col: 116,
        ws_row: 28,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fixture_name = "vim_ctrl_u";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[QUIT]);
    start(Box::new(fake_input_output.clone()), Opt::default());
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
