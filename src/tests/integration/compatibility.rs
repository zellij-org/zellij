use ::nix::pty::Winsize;
use ::insta::assert_snapshot;
use ::std::collections::HashMap;

use crate::start;
use crate::tests::possible_tty_inputs::Bytes;
use crate::tests::fakes::{FakeInputOutput};
use crate::tests::utils::get_output_frame_snapshots;

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
    fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()));
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
    fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()));
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
    fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()));
    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);
    for snapshot in snapshots {
        assert_snapshot!(snapshot);
    }
}
