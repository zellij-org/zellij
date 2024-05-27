#![allow(unused)]

use ::insta::assert_snapshot;
use zellij_utils::{pane_size::Size, position::Position};

use rand::Rng;
use regex::Regex;

use std::fmt::Write;
use std::path::Path;

use crate::tests::e2e::steps::{
    check_focus_on_second_tab, check_second_tab_opened, check_third_tab_is_left_wrapped,
    check_third_tab_is_right_wrapped, check_third_tab_moved_left,
    check_third_tab_moved_to_beginning, check_third_tab_opened, move_tab_left, move_tab_right,
    new_tab, switch_focus_to_left_tab, type_second_tab_content,
};

use super::remote_runner::{RemoteRunner, RemoteTerminal, Step};

pub const QUIT: [u8; 1] = [17]; // ctrl-q
pub const ESC: [u8; 1] = [27];
pub const ENTER: [u8; 1] = [10]; // char '\n'
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

pub fn sgr_mouse_report(position: Position, button: u8) -> Vec<u8> {
    // button: (release is with lower case m, not supported here yet)
    // 0 => left click
    // 2 => right click
    // 64 => scroll up
    // 65 => scroll down
    let Position { line, column } = position;
    format!("\u{1b}[<{};{};{}M", button, column.0, line.0)
        .as_bytes()
        .to_vec()
}

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
    let base_replace = Regex::new(r" BASE \s*\n").unwrap();
    let eol_arrow_replace = Regex::new(r"\s*\n").unwrap();
    let snapshot = base_replace.replace_all(&snapshot, "\n").to_string();
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
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        if runner.test_timed_out && test_attempts > 0 {
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
pub fn split_terminals_vertically() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for new pane to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
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
pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let fake_win_size = Size { cols: 8, rows: 20 };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2) {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Make sure only one pane appears",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                //two empty lines at the bottom to make sure there is no plugin output
                    && remote_terminal
                        .current_snapshot()
                        .ends_with("        \n        ")
                {
                    // ... is the truncated tip line
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
pub fn scrolling_inside_a_pane() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Fill terminal with text",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.load_fixture("e2e/scrolling_inside_a_pane");
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Scroll up inside pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 20)
                        && remote_terminal.snapshot_contains("line21")
                    {
                        // all lines have been written to the pane
                        remote_terminal.send_key(&SCROLL_MODE);
                        remote_terminal.send_key(&SCROLL_UP_IN_SCROLL_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for scroll to finish",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 20)
                    && remote_terminal.snapshot_contains("line3 ")
                    && remote_terminal.snapshot_contains("SCROLL:  1/4")
                {
                    // keyboard scrolls up 1 line, scrollback is 4 lines: cat command + 2 extra lines from fixture + prompt
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
pub fn toggle_pane_fullscreen() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Change newly opened pane to be fullscreen",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for pane to become fullscreen",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2) {
                    // cursor is in full screen pane now
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
pub fn open_new_tab() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Open new tab",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for new tab to open",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.tip_appears()
                    && remote_terminal.snapshot_contains("Tab #2")
                    && remote_terminal.status_bar_appears()
                {
                    // cursor is in the newly opened second tab
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
pub fn close_tab() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Open new tab",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Close tab",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(3, 2)
                        && remote_terminal.tip_appears()
                        && remote_terminal.snapshot_contains("Tab #2")
                        && remote_terminal.status_bar_appears()
                    {
                        // cursor is in the newly opened second tab
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&CLOSE_TAB_IN_TAB_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for tab to close",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("Tab #1")
                    && !remote_terminal.snapshot_contains("Tab #2")
                {
                    // cursor is in the first tab again
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
    assert!(last_snapshot.contains("Tab #1"));
    assert!(!last_snapshot.contains("Tab #2"));
}

#[test]
#[ignore]
pub fn move_tab_to_left() {
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size());
        let mut runner = RemoteRunner::new(fake_win_size())
            .add_step(new_tab())
            .add_step(check_second_tab_opened())
            .add_step(new_tab())
            .add_step(check_third_tab_opened()) // should have Tab#1 >> Tab#2 >> Tab#3 (focused on Tab#3)
            .add_step(move_tab_left()); // now, it should be Tab#1 >> Tab#3 >> Tab#2

        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(check_third_tab_moved_left());
        if !runner.test_timed_out || test_attempts == 0 {
            break last_snapshot;
        }
        test_attempts -= 1;
    };
    assert_snapshot!(account_for_races_in_snapshot(last_snapshot));
}

fn fake_win_size() -> Size {
    Size {
        cols: 120,
        rows: 24,
    }
}

#[test]
#[ignore]
pub fn move_tab_to_right() {
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size());
        let mut runner = RemoteRunner::new(fake_win_size())
            .add_step(new_tab())
            .add_step(check_second_tab_opened())
            .add_step(type_second_tab_content()) // allows verifying the focus later
            .add_step(new_tab())
            .add_step(check_third_tab_opened())
            .add_step(switch_focus_to_left_tab())
            .add_step(check_focus_on_second_tab()) // should have Tab#1 >> Tab#2 >> Tab#3 (focused on Tab#2)
            .add_step(move_tab_right()); // now, it should be Tab#1 >> Tab#3 >> Tab#2

        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(check_third_tab_moved_left());
        if !runner.test_timed_out || test_attempts == 0 {
            break last_snapshot;
        }
        test_attempts -= 1;
    };
    assert_snapshot!(account_for_races_in_snapshot(last_snapshot));
}

#[test]
#[ignore]
pub fn move_tab_to_left_until_it_wraps_around() {
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size());
        let mut runner = RemoteRunner::new(fake_win_size())
            .add_step(new_tab())
            .add_step(check_second_tab_opened())
            .add_step(new_tab())
            .add_step(check_third_tab_opened())
            .add_step(move_tab_left())
            .add_step(check_third_tab_moved_left())
            .add_step(move_tab_left())
            .add_step(check_third_tab_moved_to_beginning()) // should have Tab#3 >> Tab#1 >> Tab#2 (focused on Tab#3)
            .add_step(move_tab_left()); // now, it should be Tab#2 >> Tab#1 >> Tab#3

        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(check_third_tab_is_left_wrapped());
        if !runner.test_timed_out || test_attempts == 0 {
            break last_snapshot;
        }
        test_attempts -= 1;
    };
    assert_snapshot!(account_for_races_in_snapshot(last_snapshot));
}

#[test]
#[ignore]
pub fn move_tab_to_right_until_it_wraps_around() {
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size());
        let mut runner = RemoteRunner::new(fake_win_size())
            .add_step(new_tab())
            .add_step(check_second_tab_opened())
            .add_step(new_tab())
            .add_step(check_third_tab_opened()) // should have Tab#1 >> Tab#2 >> Tab#3 (focused on Tab#3)
            .add_step(move_tab_right()); // now, it should be Tab#3 >> Tab#2 >> Tab#1

        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(check_third_tab_is_right_wrapped());
        if !runner.test_timed_out || test_attempts == 0 {
            break last_snapshot;
        }
        test_attempts -= 1;
    };
    assert_snapshot!(account_for_races_in_snapshot(last_snapshot));
}

#[test]
#[ignore]
pub fn close_pane() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Close pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&CLOSE_PANE_IN_PANE_MODE);
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
                if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
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
pub fn exit_zellij() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Wait for app to load",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&QUIT);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        runner.take_snapshot_after(Step {
            name: "Wait for app to exit",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if !remote_terminal.status_bar_appears()
                    && remote_terminal.snapshot_contains("Bye from Zellij!")
                {
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
    };
    assert!(last_snapshot.contains("Bye from Zellij!"));
}

#[test]
#[ignore]
pub fn closing_last_pane_exits_zellij() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Close pane",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&CLOSE_PANE_IN_PANE_MODE);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        if runner.test_timed_out && test_attempts > 0 {
            test_attempts -= 1;
            continue;
        }
        break runner.take_snapshot_after(Step {
            name: "Wait for app to exit",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("Bye from Zellij!") {
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
    };
    assert!(last_snapshot.contains("Bye from Zellij!"));
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
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
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        remote_terminal.send_key("e".as_bytes());
                        remote_terminal.send_key("x".as_bytes());
                        remote_terminal.send_key("i".as_bytes());
                        remote_terminal.send_key("t".as_bytes());
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
                // if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
                if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
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
pub fn resize_pane() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Resize pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&RESIZE_MODE);
                        remote_terminal.send_key(&RESIZE_LEFT_IN_RESIZE_MODE);
                        // back to normal mode
                        remote_terminal.send_key(&ENTER);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for pane to be resized",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(57, 2) && remote_terminal.tip_appears() {
                    // pane has been resized
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
pub fn lock_mode() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size)
            .add_step(Step {
                name: "Enter lock mode",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&LOCK_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Send keys that should not be intercepted by the app",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("INTERFACE LOCKED") {
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                        remote_terminal.send_key("abc".as_bytes());
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for terminal to render sent keys",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(7, 2) {
                    // text has been entered into the only terminal pane
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
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
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
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
                if remote_terminal.cursor_position_is(53, 2)
                    && remote_terminal.tip_appears()
                    && remote_terminal.snapshot_contains("Ctrl +")
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
pub fn detach_and_attach_session() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new_mirrored_session(fake_win_size)
            .add_step(Step {
                name: "Split pane to the right",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Send some text to the active pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // new pane has been opened and focused
                        remote_terminal.send_key("I am some text".as_bytes());
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Detach session",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(77, 2) {
                        remote_terminal.send_key(&SESSION_MODE);
                        remote_terminal.send_key(&DETACH_IN_SESSION_MODE);
                        // text has been entered
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Reattach session",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if !remote_terminal.status_bar_appears() {
                        // we don't see the toolbar, so we can assume we've already detached
                        remote_terminal.attach_to_original_session();
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for session to be attached",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(77, 2)
                {
                    // we're back inside the session
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
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("(FLOATING PANES VISIBLE)") {
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
pub fn quit_and_resurrect_session_with_viewport_serialization() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let layout_name = "layout_for_resurrection.kdl";
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new_mirrored_session_with_layout_and_viewport_serialization(
            fake_win_size,
            layout_name,
        )
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
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("(FLOATING PANES VISIBLE)") {
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
pub fn status_bar_loads_custom_keybindings() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let config_file_name = "changed_keys.kdl";
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new_with_config(fake_win_size, config_file_name);
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for app to load",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
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
fn focus_pane_with_mouse() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Click left pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        remote_terminal.send_key(&sgr_mouse_report(Position::new(5, 2), 0));
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for left pane to be focused",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
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
pub fn scrolling_inside_a_pane_with_mouse() {
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
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Fill terminal with text",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        remote_terminal.load_fixture("e2e/scrolling_inside_a_pane");
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Scroll up inside pane",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 20)
                        && remote_terminal.snapshot_contains("line21")
                    {
                        // all lines have been written to the pane
                        remote_terminal.send_key(&sgr_mouse_report(Position::new(2, 64), 64));
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for scroll to finish",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 20)
                    && remote_terminal.snapshot_contains("line1 ")
                    && remote_terminal.snapshot_contains("SCROLL:  3/4")
                {
                    // mouse wheel scrolls up 3 lines, scrollback is 4 lines: cat command + 2 extra lines from fixture + prompt
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
pub fn start_without_pane_frames() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new_without_frames(fake_win_size).add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 1)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for new pane to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(62, 1) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
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
pub fn mirrored_sessions() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let session_name = "mirrored_sessions";
    let (first_runner_snapshot, second_runner_snapshot) = loop {
        // here we connect with one runner, then connect with another, perform some actions and
        // then make sure they were also reflected (mirrored) in the first runner afterwards
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut first_runner =
            RemoteRunner::new_with_session_name(fake_win_size, session_name, true)
                .dont_panic()
                .add_step(Step {
                    name: "Wait for app to load",
                    instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                        let mut step_is_complete = false;
                        if remote_terminal.status_bar_appears()
                            && remote_terminal.cursor_position_is(3, 2)
                        {
                            step_is_complete = true;
                        }
                        step_is_complete
                    },
                });
        first_runner.run_all_steps();

        let mut second_runner = RemoteRunner::new_existing_session(fake_win_size, session_name)
            .dont_panic()
            .add_step(Step {
                name: "Split pane to the right",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Open new tab (second user)",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for new tab to open",
                instruction: |remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(3, 2)
                        && remote_terminal.tip_appears()
                        && remote_terminal.snapshot_contains("Tab #2")
                        && remote_terminal.status_bar_appears()
                    {
                        // cursor is in the newly opened second tab
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Switch to previous tab",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(3, 2)
                        && remote_terminal.tip_appears()
                        && remote_terminal.snapshot_contains("Tab #2")
                    {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key("some text".as_bytes());
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for text to appear on screen",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("some text") {
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&MOVE_FOCUS_LEFT_IN_PANE_MODE); // same key as tab mode
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        second_runner.run_all_steps();

        if first_runner.test_timed_out || second_runner.test_timed_out {
            test_attempts -= 1;
            continue;
        }
        let second_runner_snapshot = second_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2)
                    && remote_terminal.snapshot_contains("┐┌")
                {
                    // cursor is back in the first tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        let first_runner_snapshot = first_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2)
                    && remote_terminal.snapshot_contains("┐┌")
                {
                    // cursor is back in the first tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        if (first_runner.test_timed_out || second_runner.test_timed_out) && test_attempts >= 0 {
            test_attempts -= 1;
            continue;
        } else {
            break (first_runner_snapshot, second_runner_snapshot);
        }
    };
    let first_runner_snapshot = account_for_races_in_snapshot(first_runner_snapshot);
    let second_runner_snapshot = account_for_races_in_snapshot(second_runner_snapshot);
    assert_snapshot!(first_runner_snapshot);
    assert_snapshot!(second_runner_snapshot);
}

#[test]
#[ignore]
pub fn multiple_users_in_same_pane_and_tab() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let session_name = "multiple_users_in_same_pane_and_tab";
    let (first_runner_snapshot, second_runner_snapshot) = loop {
        // here we connect with one runner, then connect with another, perform some actions and
        // then make sure they were also reflected (mirrored) in the first runner afterwards
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut first_runner =
            RemoteRunner::new_with_session_name(fake_win_size, session_name, false)
                .dont_panic()
                .add_step(Step {
                    name: "Wait for app to load",
                    instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                        let mut step_is_complete = false;
                        if remote_terminal.status_bar_appears()
                            && remote_terminal.cursor_position_is(3, 2)
                        {
                            step_is_complete = true;
                        }
                        step_is_complete
                    },
                });
        first_runner.run_all_steps();

        let mut second_runner = RemoteRunner::new_existing_session(fake_win_size, session_name)
            .dont_panic()
            .add_step(Step {
                name: "Wait for app to load",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        second_runner.run_all_steps();

        if first_runner.test_timed_out || second_runner.test_timed_out {
            test_attempts -= 1;
            continue;
        }
        let second_runner_snapshot = second_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.snapshot_contains("MY FOCUS")
                {
                    // cursor is back in the first tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        let first_runner_snapshot = first_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.snapshot_contains("MY FOCUS")
                {
                    // cursor is back in the first tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        if (first_runner.test_timed_out || second_runner.test_timed_out) && test_attempts >= 0 {
            test_attempts -= 1;
            continue;
        } else {
            break (first_runner_snapshot, second_runner_snapshot);
        }
    };
    let first_runner_snapshot = account_for_races_in_snapshot(first_runner_snapshot);
    let second_runner_snapshot = account_for_races_in_snapshot(second_runner_snapshot);
    assert_snapshot!(first_runner_snapshot);
    assert_snapshot!(second_runner_snapshot);
}

#[test]
#[ignore]
pub fn multiple_users_in_different_panes_and_same_tab() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let session_name = "multiple_users_in_same_pane_and_tab";
    let (first_runner_snapshot, second_runner_snapshot) = loop {
        // here we connect with one runner, then connect with another, perform some actions and
        // then make sure they were also reflected (mirrored) in the first runner afterwards
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut first_runner =
            RemoteRunner::new_with_session_name(fake_win_size, session_name, false)
                .dont_panic()
                .add_step(Step {
                    name: "Wait for app to load",
                    instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                        let mut step_is_complete = false;
                        if remote_terminal.status_bar_appears()
                            && remote_terminal.cursor_position_is(3, 2)
                        {
                            step_is_complete = true;
                        }
                        step_is_complete
                    },
                });
        first_runner.run_all_steps();

        let mut second_runner = RemoteRunner::new_existing_session(fake_win_size, session_name)
            .dont_panic()
            .add_step(Step {
                name: "Split pane to the right",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        remote_terminal.send_key(&PANE_MODE);
                        remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        second_runner.run_all_steps();

        if first_runner.test_timed_out || second_runner.test_timed_out {
            test_attempts -= 1;
            continue;
        }

        let second_runner_snapshot = second_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        let first_runner_snapshot = first_runner.take_snapshot_after(Step {
            name: "take snapshot after",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.snapshot_contains("││$")
                {
                    // cursor is back in the first tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        if (first_runner.test_timed_out || second_runner.test_timed_out) && test_attempts >= 0 {
            test_attempts -= 1;
            continue;
        } else {
            break (first_runner_snapshot, second_runner_snapshot);
        }
    };
    let first_runner_snapshot = account_for_races_in_snapshot(first_runner_snapshot);
    let second_runner_snapshot = account_for_races_in_snapshot(second_runner_snapshot);
    assert_snapshot!(first_runner_snapshot);
    assert_snapshot!(second_runner_snapshot);
}

#[test]
#[ignore]
pub fn multiple_users_in_different_tabs() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    let mut test_attempts = 10;
    let session_name = "multiple_users_in_different_tabs";
    let (first_runner_snapshot, second_runner_snapshot) = loop {
        // here we connect with one runner, then connect with another, perform some actions and
        // then make sure they were also reflected (mirrored) in the first runner afterwards
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut first_runner =
            RemoteRunner::new_with_session_name(fake_win_size, session_name, false)
                .dont_panic()
                .add_step(Step {
                    name: "Wait for app to load",
                    instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                        let mut step_is_complete = false;
                        if remote_terminal.status_bar_appears()
                            && remote_terminal.cursor_position_is(3, 2)
                        {
                            step_is_complete = true;
                        }
                        step_is_complete
                    },
                });
        first_runner.run_all_steps();

        let mut second_runner = RemoteRunner::new_existing_session(fake_win_size, session_name)
            .dont_panic()
            .add_step(Step {
                name: "Open new tab",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.cursor_position_is(3, 2) && remote_terminal.tip_appears() {
                        // cursor is in the newly opened second pane
                        remote_terminal.send_key(&TAB_MODE);
                        remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            });
        second_runner.run_all_steps();

        if first_runner.test_timed_out || second_runner.test_timed_out {
            test_attempts -= 1;
            continue;
        }

        let second_runner_snapshot = second_runner.take_snapshot_after(Step {
            name: "Wait for new tab to open",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.tip_appears()
                    && remote_terminal.snapshot_contains("Tab #1 [ ]")
                    && remote_terminal.snapshot_contains("Tab #2")
                    && remote_terminal.status_bar_appears()
                {
                    // cursor is in the newly opened second tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        let first_runner_snapshot = first_runner.take_snapshot_after(Step {
            name: "Wait for new tab to open",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(3, 2)
                    && remote_terminal.tip_appears()
                    && remote_terminal.snapshot_contains("Tab #2 [ ]")
                    && remote_terminal.status_bar_appears()
                {
                    // cursor is in the newly opened second tab
                    step_is_complete = true;
                }
                step_is_complete
            },
        });

        if (first_runner.test_timed_out || second_runner.test_timed_out) && test_attempts >= 0 {
            test_attempts -= 1;
            continue;
        } else {
            break (first_runner_snapshot, second_runner_snapshot);
        }
    };
    let first_runner_snapshot = account_for_races_in_snapshot(first_runner_snapshot);
    let second_runner_snapshot = account_for_races_in_snapshot(second_runner_snapshot);
    assert_snapshot!(first_runner_snapshot);
    assert_snapshot!(second_runner_snapshot);
}

#[test]
#[ignore]
pub fn bracketed_paste() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };
    // here we enter some text, before which we invoke "bracketed paste mode"
    // we make sure the text in bracketed paste mode is sent directly to the terminal and not
    // interpreted by us (in this case it will send ^T to the terminal), then we exit bracketed
    // paste, send some more text and make sure it's also sent to the terminal
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Send pasted text followed by normal text",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears()
                    && remote_terminal.tab_bar_appears()
                    && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&BRACKETED_PASTE_START);
                    remote_terminal.send_key(&TAB_MODE);
                    remote_terminal.send_key(&NEW_TAB_IN_TAB_MODE);
                    remote_terminal.send_key("a".as_bytes());
                    remote_terminal.send_key("b".as_bytes());
                    remote_terminal.send_key("c".as_bytes());
                    remote_terminal.send_key(&BRACKETED_PASTE_END);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for terminal to render sent keys",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("abc") {
                    // text has been entered into the only terminal pane
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
pub fn toggle_floating_panes() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Toggle floating panes",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&TOGGLE_FLOATING_PANES);
                    // back to normal mode after split
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for new pane to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(33, 7)
                    && remote_terminal.snapshot_contains("FLOATING PANES VISIBLE")
                {
                    // cursor is in the newly opened second pane
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
pub fn tmux_mode() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&TMUX_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_TMUX_MODE);
                    // back to normal mode after split
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for new pane to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
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
pub fn edit_scrollback() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&SCROLL_MODE);
                    remote_terminal.send_key(&EDIT_SCROLLBACK);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();
        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for editor to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains(".dump") {
                    // the .dump is an indication we get on the bottom line of vi when editing a
                    // file
                    // the temp file name is randomly generated, so we don't assert the whole snapshot
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
    assert!(last_snapshot.contains(".dump"));
}

#[test]
#[ignore]
pub fn undo_rename_tab() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Undo tab name change",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears()
                    && remote_terminal.snapshot_contains("Tab #1")
                {
                    remote_terminal.send_key(&TAB_MODE);
                    remote_terminal.send_key(&RENAME_TAB_MODE);
                    remote_terminal.send_key(&[97, 97]);
                    remote_terminal.send_key(&ESC);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for tab name to apper on screen",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("Tab #1") {
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

#[test]
#[ignore]
pub fn undo_rename_pane() {
    let fake_win_size = Size {
        cols: 120,
        rows: 24,
    };

    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size).add_step(Step {
            name: "Undo pane name change",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(3, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&RENAME_PANE_MODE);
                    remote_terminal.send_key(&[97, 97]);
                    remote_terminal.send_key(&ESC);
                    step_is_complete = true;
                }
                step_is_complete
            },
        });
        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for pane name to apper on screen",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("Pane #1") {
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

#[test]
#[ignore]
pub fn send_command_through_the_cli() {
    // here we test the following flow:
    // - send a command through the cli to run a bash script in a temporary folder
    // - have it open a "command pane" that can be re-run with Enter
    // - press Enter in the command pane to re-run the script
    //
    // the script appends the word "foo" to a temporary file and then `cat`s that file,
    // so when we press "Enter", it will run again and we'll see two "foo"s one after the other,
    // that's how we know the whole flow is working
    let fake_win_size = Size {
        cols: 150,
        rows: 24,
    };
    let mut test_attempts = 10;
    let last_snapshot = loop {
        RemoteRunner::kill_running_sessions(fake_win_size);
        let mut runner = RemoteRunner::new(fake_win_size)
            .add_step(Step {
                name: "Run command through the cli",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.status_bar_appears()
                        && remote_terminal.cursor_position_is(3, 2)
                    {
                        let fixture_folder = remote_terminal.path_to_fixture_folder();
                        remote_terminal.send_command_through_the_cli(&format!(
                            "{}/append-echo-script.sh",
                            fixture_folder
                        ));
                        step_is_complete = true;
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Initial run of suspended command",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("<Ctrl-c>")
                        && remote_terminal.cursor_position_is(0, 0)
                    // cursor does not appear in
                    // suspend_start panes
                    {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&SPACE); // run script - here we use SPACE
                                                          // instead of the default ENTER because
                                                          // sending ENTER over SSH can be a little
                                                          // problematic (read: I couldn't get it
                                                          // to pass consistently)
                        step_is_complete = true
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for command to run",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("<Ctrl-c>")
                        && remote_terminal.cursor_position_is(76, 3)
                    {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        remote_terminal.send_key(&SPACE); // re-run script - here we use SPACE
                                                          // instead of the default ENTER because
                                                          // sending ENTER over SSH can be a little
                                                          // problematic (read: I couldn't get it
                                                          // to pass consistently)
                        step_is_complete = true
                    }
                    step_is_complete
                },
            })
            .add_step(Step {
                name: "Wait for script to run again",
                instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                    let mut step_is_complete = false;
                    if remote_terminal.snapshot_contains("<Ctrl-c>")
                        && remote_terminal.cursor_position_is(76, 4)
                    {
                        step_is_complete = true
                    }
                    step_is_complete
                },
            });
        runner.run_all_steps();

        let last_snapshot = runner.take_snapshot_after(Step {
            name: "Wait for script to run twice",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("foo")
                    && remote_terminal.cursor_position_is(76, 4)
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
