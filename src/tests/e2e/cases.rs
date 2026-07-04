/*
 *
 * NOTE: These tests are very heavy and are used as smoke tests just to verify the app is working
 * end-to-end. Avoid adding new ones, preferring instead to use the zellij-integration-tests module
 * it tests the app as a whole, only mocking the OS interaction parts
 *
*/
#![allow(unused)]

use insta::assert_snapshot;
use zellij_utils::{
    pane_size::Size,
    position::{Column, Line, Position},
};

use rand::Rng;
use regex::Regex;

use std::fmt::Write;
use std::path::Path;

use super::remote_runner::{RemoteRunner, RemoteTerminal, Step};

pub const QUIT: [u8; 1] = [17]; // ctrl-q
pub const ESC: [u8; 1] = [27];
pub const ENTER: [u8; 2] = [10, 13]; // '\n\r'
pub const SPACE: [u8; 1] = [32];
pub const LOCK_MODE: [u8; 1] = [7]; // ctrl-g

pub const MOVE_FOCUS_LEFT_IN_NORMAL_MODE: [u8; 2] = [27, 104]; // alt-h
pub const MOVE_FOCUS_RIGHT_IN_NORMAL_MODE: [u8; 2] = [27, 108]; // alt-l

pub const PANE_MODE: [u8; 1] = [16]; // ctrl-p
pub const TMUX_MODE: [u8; 1] = [2]; // ctrl-b
pub const SPAWN_TERMINAL_IN_PANE_MODE: [u8; 1] = [110]; // n
pub const MOVE_FOCUS_IN_PANE_MODE: [u8; 1] = [112]; // p
pub const SPLIT_DOWN_IN_PANE_MODE: [u8; 1] = [100]; // d
pub const SPLIT_RIGHT_IN_PANE_MODE: [u8; 1] = [114]; // r
pub const SPLIT_RIGHT_IN_TMUX_MODE: [u8; 1] = [37]; // %
pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE: [u8; 1] = [102]; // f
pub const TOGGLE_FLOATING_PANES: [u8; 1] = [119]; // w
pub const CLOSE_PANE_IN_PANE_MODE: [u8; 1] = [120]; // x
pub const MOVE_FOCUS_DOWN_IN_PANE_MODE: [u8; 1] = [106]; // j
pub const MOVE_FOCUS_UP_IN_PANE_MODE: [u8; 1] = [107]; // k
pub const MOVE_FOCUS_LEFT_IN_PANE_MODE: [u8; 1] = [104]; // h
pub const MOVE_FOCUS_RIGHT_IN_PANE_MODE: [u8; 1] = [108]; // l
pub const RENAME_PANE_MODE: [u8; 1] = [99]; // c

pub const SCROLL_MODE: [u8; 1] = [19]; // ctrl-s
pub const SCROLL_UP_IN_SCROLL_MODE: [u8; 1] = [107]; // k
pub const SCROLL_DOWN_IN_SCROLL_MODE: [u8; 1] = [106]; // j
pub const SCROLL_PAGE_UP_IN_SCROLL_MODE: [u8; 1] = [2]; // ctrl-b
pub const SCROLL_PAGE_DOWN_IN_SCROLL_MODE: [u8; 1] = [6]; // ctrl-f
pub const EDIT_SCROLLBACK: [u8; 1] = [101]; // e

pub const RESIZE_MODE: [u8; 1] = [14]; // ctrl-n
pub const RESIZE_DOWN_IN_RESIZE_MODE: [u8; 1] = [106]; // j
pub const RESIZE_UP_IN_RESIZE_MODE: [u8; 1] = [107]; // k
pub const RESIZE_LEFT_IN_RESIZE_MODE: [u8; 1] = [104]; // h
pub const RESIZE_RIGHT_IN_RESIZE_MODE: [u8; 1] = [108]; // l

pub const TAB_MODE: [u8; 1] = [20]; // ctrl-t
pub const NEW_TAB_IN_TAB_MODE: [u8; 1] = [110]; // n
pub const SWITCH_NEXT_TAB_IN_TAB_MODE: [u8; 1] = [108]; // l
pub const SWITCH_PREV_TAB_IN_TAB_MODE: [u8; 1] = [104]; // h
pub const CLOSE_TAB_IN_TAB_MODE: [u8; 1] = [120]; // x
pub const RENAME_TAB_MODE: [u8; 1] = [114]; // r

pub const MOVE_TAB_LEFT: [u8; 2] = [27, 105]; // Alt + i
pub const MOVE_TAB_RIGHT: [u8; 2] = [27, 111]; // Alt + o

pub const SESSION_MODE: [u8; 1] = [15]; // ctrl-o
pub const DETACH_IN_SESSION_MODE: [u8; 1] = [100]; // d

pub const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
pub const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201
pub const SLEEP: [u8; 0] = [];

pub const SECOND_TAB_CONTENT: [u8; 14] =
    [84, 97, 98, 32, 35, 50, 32, 99, 111, 110, 116, 101, 110, 116]; // Tab #2 content

// what we do here is adjust snapshots for various race conditions that should hopefully be
// temporary until we can fix them - when adding stuff here, please add a detailed comment
// explaining the race condition and what needs to be done to solve it
fn account_for_races_in_snapshot(snapshot: String) -> String {
    // these replacements need to be done because plugins set themselves as "unselectable" at runtime
    // when they are loaded - since they are loaded asynchronously, sometimes the "BASE" indication
    // (which should only happen if there's more than one selectable pane) is rendered and
    // sometimes it isn't - this removes it entirely
    //
    // to fix this, we should set plugins as unselectable in the layout (before they are loaded),
    // once that happens, we should be able to remove this hack (and adjust the snapshots for the
    // trailing spaces that we had to get rid of here)
    let base_replace = Regex::new(r"Alt <\[\]>  BASE \s*\n").unwrap();
    let base_replace_tmux_mode_1 = Regex::new(r"Alt \[\|SPACE\|Alt \]  BASE \s*\n").unwrap();
    let base_replace_tmux_mode_2 = Regex::new(r"Alt \[\|Alt \]\|SPACE  BASE \s*\n").unwrap();
    let eol_arrow_replace = Regex::new(r"\s*\n").unwrap();
    let snapshot = base_replace.replace_all(&snapshot, "\n").to_string();
    let snapshot = base_replace_tmux_mode_1
        .replace_all(&snapshot, "\n")
        .to_string();
    let snapshot = base_replace_tmux_mode_2
        .replace_all(&snapshot, "\n")
        .to_string();
    let snapshot = eol_arrow_replace.replace_all(&snapshot, "\n").to_string();

    snapshot
}

// All the E2E tests are marked as "ignored" so that they can be run separately from the normal
// tests

#[test]
#[ignore]
pub fn starts_with_one_terminal() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size);
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for app to load",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 1)
                {
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        } else {
            break last_snapshot;
        }
    };

    let last_snapshot = account_for_races_in_snapshot(last_snapshot);
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn typing_exit_closes_pane() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size)
            .add_step(Step {
                name: "Split pane to the right",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(2, 1)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Type exit",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(62, 2)
                        && remote_terminal.status_bar_appears()
                    {
                        remote_terminal.send_key("e".as_bytes());
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key("x".as_bytes());
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key("i".as_bytes());
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key("t".as_bytes());
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key("\n".as_bytes());
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for pane to close",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(2, 1) && remote_terminal.status_bar_appears()
                {
                    // cursor is in the original pane
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        } else {
            break last_snapshot;
        }
    };
    let last_snapshot = account_for_races_in_snapshot(last_snapshot);
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn resize_terminal_window() {
    // this checks the resizing of the whole terminal window (reaction to SIGWINCH) and not just one pane
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size)
            .add_step(Step {
                name: "Split pane to the right",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(2, 1)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Change terminal window size",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(62, 2)
                        && remote_terminal.status_bar_appears()
                    {
                        // new pane has been opened and focused
                        remote_terminal.change_size(100, 24);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "wait for terminal to be resized and app to be re-rendered",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(52, 2) && remote_terminal.ctrl_plus_appears()
                {
                    // size has been changed
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        } else {
            break last_snapshot;
        }
    };
    let last_snapshot = account_for_races_in_snapshot(last_snapshot);
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn quit_and_resurrect_session() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let layout_name = "layout_for_resurrection.kdl";
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new_mirrored_session_with_layout(fake_win_size, layout_name)
            .add_step(Step {
                name: "Wait for session to be serialized",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("Waiting to run: top") {
                        std::thread::sleep(std::time::Duration::from_millis(5000)); // wait for
                                                                                    // serialization
                        remote_terminal.send_key(&QUIT);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Resurrect session by attaching",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("Bye from Zellij!") {
                        remote_terminal.attach_to_original_session();
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for session to be resurrected",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                remote_terminal.snapshot_contains("(FLOATING PANES VISIBLE)")
                    && remote_terminal.status_bar_appears()
                    && remote_terminal.tab_bar_appears()
            },
        });
        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        } else {
            break last_snapshot;
        }
    };
    let last_snapshot = account_for_races_in_snapshot(last_snapshot);
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn send_blocking_command_through_the_cli() {
    // here we test the following flow:
    // - send a blocking command through the cli with --blocking --floating --close-on-exit
    // - the command sleeps for 2 seconds (longer than the default 1s timeout) then exits with status 42
    // - verify that the CLI blocks for the full duration (we check after 2+ seconds)
    // - verify that the floating pane appears while running and disappears after completion
    // - verify that the exit status is properly propagated
    let fake_win_size = Size {
        cols: 150,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size)
            .add_step(Step {
                name: "Run blocking command through the cli",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(2, 1)
                    {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal
                            .send_blocking_command_through_the_cli("bash -c 'sleep 2 && exit 42'");
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&ENTER);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for floating pane to appear",
                instruction: |remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    // The floating pane should appear with the running command
                    if remote_terminal.snapshot_contains("PIN [ ]") {
                        std::thread::sleep(std::time::Duration::from_millis(2000)); // wait for
                                                                                    // command to
                                                                                    // end
                        step_is_complete = true
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for command to complete and verify exit status",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    // After 2+ seconds, the command should complete and the floating pane should close
                    // Wait until the floating pane is gone AND the shell prompt is back before
                    // asking for $?, otherwise we can race the blocking CLI process itself
                    // returning to the shell.
                    if !remote_terminal.snapshot_contains("PIN [ ]")
                        && remote_terminal.snapshot_contains("$ \u{2588}")
                        && remote_terminal.status_bar_appears()
                    {
                        remote_terminal.send_key("echo $?".as_bytes());
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&ENTER);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        step_is_complete = true
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Verify CLI returned with proper exit status after command completed",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                // wait until echo $? is visible, the exit status rendered, and the cursor is back
                // at a blank prompt, which means the shell command actually executed
                if remote_terminal.snapshot_contains("echo $?")
                    && remote_terminal.snapshot_contains("42")
                    && remote_terminal.snapshot_contains("$ \u{2588}")
                    && remote_terminal.status_bar_appears()
                {
                    step_is_complete = true
                }
                step_is_complete
            },
        });

        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        } else {
            break last_snapshot;
        }
    };
    let last_snapshot = account_for_races_in_snapshot(last_snapshot);
    assert_snapshot!(last_snapshot);
}
