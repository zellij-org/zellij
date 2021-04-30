use ::insta::assert_snapshot;
use ::std::collections::HashMap;

use crate::panes::PositionAndSize;
use crate::tests::fakes::FakeInputOutput;
use crate::tests::possible_tty_inputs::Bytes;
use crate::tests::utils::{get_next_to_last_snapshot, get_output_frame_snapshots};
use crate::{start, CliArgs};

use crate::common::input::config::Config;
use crate::tests::utils::commands::QUIT;

/*
 * These tests are general compatibility tests for non-trivial scenarios running in the terminal.
 * They use fake TTY input replicated from these scenarios (and so don't actually interact with the
 * OS).
 *
 * They work like this:
 * - receive fake TTY input containing various VTE instructions.
 * - run that output through zellij so it interprets it and creates its state based on it
 * - read that state into a Human-readable snapshot and compare it to the expected snapshot for
 * this scenario.
 *
 */

fn get_fake_os_input(fake_win_size: &PositionAndSize, fixture_name: &str) -> FakeInputOutput {
    let mut tty_inputs = HashMap::new();
    let fixture_bytes = Bytes::from_file_in_fixtures(&fixture_name);
    tty_inputs.insert(fake_win_size.columns as u16, fixture_bytes);
    FakeInputOutput::new(fake_win_size.clone()).with_tty_inputs(tty_inputs)
}

#[test]
pub fn run_bandwhich_from_fish_shell() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "fish_and_bandwhich";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn fish_tab_completion_options() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "fish_tab_completion_options";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn fish_select_tab_completion_options() {
    // the difference between this and the previous test is that here we press <TAB>
    // twice, meaning the selection moves between the options and the command line
    // changes.
    // this is not clearly seen in the snapshot because it does not include styles,
    // but we can see the command line change and the cursor staying in place
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "fish_select_tab_completion_options";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn vim_scroll_region_down() {
    // here we test a case where vim defines the scroll region as lesser than the screen row count
    // and then scrolls down
    // the region is defined here by vim as 1-26 (there are 28 rows)
    // then the cursor is moved to line 26 and a new line is added
    // what should happen is that the first line in the scroll region (1) is deleted
    // and an empty line is inserted in the last scroll region line (26)
    // this tests also has other steps afterwards that fills the line with the next line in the
    // file
    // experience appear to the user
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "vim_scroll_region_down";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]); // quit (ctrl-q)
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn vim_ctrl_d() {
    // in vim ctrl-d moves down half a page
    // in this case, it sends the terminal the csi 'M' directive, which tells it to delete X (13 in
    // this case) lines inside the scroll region and push the other lines up
    // what happens here is that 13 lines are deleted and instead 13 empty lines are added at the
    // end of the scroll region
    // vim makes sure to fill these empty lines with the rest of the file
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "vim_ctrl_d";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn vim_ctrl_u() {
    // in vim ctrl-u moves up half a page
    // in this case, it sends the terminal the csi 'L' directive, which tells it to insert X (13 in
    // this case) lines at the cursor, pushing away (deleting) the last line in the scroll region
    // this causes the effect of scrolling up X lines (vim replaces the lines with the ones in the
    // file above the current content)
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "vim_ctrl_u";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn htop() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "htop";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn htop_scrolling() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "htop_scrolling";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn htop_right_scrolling() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "htop_right_scrolling";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn vim_overwrite() {
    // this tests the vim overwrite message
    // to recreate:
    // * open a file in vim
    // * open the same file in another window
    // * change the file in the other window and save
    // * change the file in the original vim window and save
    // * confirm you would like to change the file by pressing 'y' and then ENTER
    // * if everything looks fine, this test passed :)
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "vim_overwrite";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn clear_scroll_region() {
    // this tests the scroll region used by eg. vim is cleared properly
    // this means that when vim exits, we get back the previous scroll
    // buffer
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "clear_scroll_region";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn display_tab_characters_properly() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "tab_characters";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn neovim_insert_mode() {
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "nvim_insert";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn bash_cursor_linewrap() {
    // this test makes sure that when we enter a command that is beyond the screen border, that it
    // immediately goes down one line
    let fake_win_size = PositionAndSize {
        columns: 116,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "bash_cursor_linewrap";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn fish_paste_multiline() {
    // here we paste a multiline command in fish shell, making sure we support it
    // going up and changing the colors of our line-wrapped pasted text
    let fake_win_size = PositionAndSize {
        columns: 149,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "fish_paste_multiline";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn git_log() {
    let fake_win_size = PositionAndSize {
        columns: 149,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "git_log";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn git_diff_scrollup() {
    // this tests makes sure that when we have a git diff that exceeds the screen size
    // we are able to scroll up
    let fake_win_size = PositionAndSize {
        columns: 149,
        rows: 28,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "git_diff_scrollup";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn emacs_longbuf() {
    let fake_win_size = PositionAndSize {
        columns: 284,
        rows: 60,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "emacs_longbuf_tutorial";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn top_and_quit() {
    let fake_win_size = PositionAndSize {
        columns: 235,
        rows: 56,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "top_and_quit";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
pub fn exa_plus_omf_theme() {
    // this tests that we handle a tab delimited table properly
    // without overriding the previous content
    // this is a potential bug because the \t character is a goto
    // if we forwarded it as is to the terminal, we would be skipping
    // over existing on-screen content without deleting it, so we must
    // convert it to spaces
    let fake_win_size = PositionAndSize {
        columns: 235,
        rows: 56,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let fixture_name = "exa_plus_omf_theme";
    let mut fake_input_output = get_fake_os_input(&fake_win_size, fixture_name);
    fake_input_output.add_terminal_input(&[&QUIT]);
    start(
        Box::new(fake_input_output.clone()),
        CliArgs::default(),
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
