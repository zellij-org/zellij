#![allow(unused)]

use ::insta::assert_snapshot;
use zellij_utils::{pane_size::PositionAndSize, position::Position};

use rand::Rng;

use std::path::Path;

use super::remote_runner::{RemoteRunner, RemoteTerminal, Step};

pub const QUIT: [u8; 1] = [17]; // ctrl-q
pub const ESC: [u8; 1] = [27];
pub const ENTER: [u8; 1] = [10]; // char '\n'
pub const LOCK_MODE: [u8; 1] = [7]; // ctrl-g

pub const MOVE_FOCUS_LEFT_IN_NORMAL_MODE: [u8; 2] = [27, 104]; // alt-h
pub const MOVE_FOCUS_RIGHT_IN_NORMAL_MODE: [u8; 2] = [27, 108]; // alt-l

pub const PANE_MODE: [u8; 1] = [16]; // ctrl-p
pub const SPAWN_TERMINAL_IN_PANE_MODE: [u8; 1] = [110]; // n
pub const MOVE_FOCUS_IN_PANE_MODE: [u8; 1] = [112]; // p
pub const SPLIT_DOWN_IN_PANE_MODE: [u8; 1] = [100]; // d
pub const SPLIT_RIGHT_IN_PANE_MODE: [u8; 1] = [114]; // r
pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE: [u8; 1] = [102]; // f
pub const CLOSE_PANE_IN_PANE_MODE: [u8; 1] = [120]; // x
pub const MOVE_FOCUS_DOWN_IN_PANE_MODE: [u8; 1] = [106]; // j
pub const MOVE_FOCUS_UP_IN_PANE_MODE: [u8; 1] = [107]; // k
pub const MOVE_FOCUS_LEFT_IN_PANE_MODE: [u8; 1] = [104]; // h
pub const MOVE_FOCUS_RIGHT_IN_PANE_MODE: [u8; 1] = [108]; // l

pub const SCROLL_MODE: [u8; 1] = [19]; // ctrl-s
pub const SCROLL_UP_IN_SCROLL_MODE: [u8; 1] = [107]; // k
pub const SCROLL_DOWN_IN_SCROLL_MODE: [u8; 1] = [106]; // j
pub const SCROLL_PAGE_UP_IN_SCROLL_MODE: [u8; 1] = [2]; // ctrl-b
pub const SCROLL_PAGE_DOWN_IN_SCROLL_MODE: [u8; 1] = [6]; // ctrl-f

pub const RESIZE_MODE: [u8; 1] = [18]; // ctrl-r
pub const RESIZE_DOWN_IN_RESIZE_MODE: [u8; 1] = [106]; // j
pub const RESIZE_UP_IN_RESIZE_MODE: [u8; 1] = [107]; // k
pub const RESIZE_LEFT_IN_RESIZE_MODE: [u8; 1] = [104]; // h
pub const RESIZE_RIGHT_IN_RESIZE_MODE: [u8; 1] = [108]; // l

pub const TAB_MODE: [u8; 1] = [20]; // ctrl-t
pub const NEW_TAB_IN_TAB_MODE: [u8; 1] = [110]; // n
pub const SWITCH_NEXT_TAB_IN_TAB_MODE: [u8; 1] = [108]; // l
pub const SWITCH_PREV_TAB_IN_TAB_MODE: [u8; 1] = [104]; // h
pub const CLOSE_TAB_IN_TAB_MODE: [u8; 1] = [120]; // x

pub const SESSION_MODE: [u8; 1] = [15]; // ctrl-o
pub const DETACH_IN_SESSION_MODE: [u8; 1] = [100]; // d

pub const BRACKETED_PASTE_START: [u8; 6] = [27, 91, 50, 48, 48, 126]; // \u{1b}[200~
pub const BRACKETED_PASTE_END: [u8; 6] = [27, 91, 50, 48, 49, 126]; // \u{1b}[201
pub const SLEEP: [u8; 0] = [];

// simplified, slighty adapted version of alacritty mouse reporting code
pub fn normal_mouse_report(position: Position, button: u8) -> Vec<u8> {
    let Position { line, column } = position;

    let mut command = vec![b'\x1b', b'[', b'M', 32 + button];
    command.push(32 + 1 + column.0 as u8);
    command.push(32 + 1 + line.0 as u8);

    command
}

// All the E2E tests are marked as "ignored" so that they can be run separately from the normal
// tests

#[test]
#[ignore]
pub fn starts_with_one_terminal() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("starts_with_one_terminal", fake_win_size, None)
        .add_step(Step {
            name: "Wait for app to load",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn split_terminals_vertically() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };

    let last_snapshot = RemoteRunner::new("split_terminals_vertically", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for new pane to appear",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(63, 2) && remote_terminal.tip_appears() {
                    // cursor is in the newly opened second pane
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let fake_win_size = PositionAndSize {
        cols: 8,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new(
        "cannot_split_terminals_vertically_when_active_terminal_is_too_small",
        fake_win_size,
        None,
    )
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .add_step(Step {
        name: "Send text to terminal",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            // this is just normal input that should be sent into the one terminal so that we can make
            // sure we silently failed to split in the previous step
            remote_terminal.send_key(&"Hi!".as_bytes());
            true
        },
    })
    .add_step(Step {
        name: "Wait for text to appear",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(5, 2) && remote_terminal.snapshot_contains("Hi!")
            {
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn scrolling_inside_a_pane() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("scrolling_inside_a_pane", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
                    remote_terminal.send_key(&format!("{:0<57}", "line1 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line2 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line3 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line4 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line5 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line6 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line7 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line8 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line9 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line10 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line11 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line12 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line13 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line14 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line15 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line16 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line17 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line18 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<59}", "line19 ").as_bytes());
                    remote_terminal.send_key(&format!("{:0<58}", "line20 ").as_bytes());
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Scroll up inside pane",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(119, 20) {
                    // all lines have been written to the pane
                    remote_terminal.send_key(&SCROLL_MODE);
                    remote_terminal.send_key(&SCROLL_UP_IN_SCROLL_MODE);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for scroll to finish",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(119, 20)
                    && remote_terminal.snapshot_contains("line1 ")
                {
                    // scrolled up one line
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn toggle_pane_fullscreen() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("toggle_pane_fullscreen", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
                    // back to normal mode after toggling fullscreen
                    remote_terminal.send_key(&ENTER);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for pane to become fullscreen",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(2, 0) {
                    // cursor is in full screen pane now
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn open_new_tab() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("open_new_tab", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for new tab to open",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(2, 2)
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
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn close_pane() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("close_pane", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
                    // back to normal mode after close
                    remote_terminal.send_key(&ENTER);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for pane to close",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(2, 2) && remote_terminal.tip_appears() {
                    // cursor is in the original pane
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn exit_zellij() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("exit_zellij", fake_win_size, None)
        .add_step(Step {
            name: "Wait for app to load",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&QUIT);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
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
        .run_all_steps();
    assert!(last_snapshot.contains("Bye from Zellij!"));
}

#[test]
#[ignore]
pub fn closing_last_pane_exits_zellij() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("closing_last_pane_exits_zellij", fake_win_size, None)
        .add_step(Step {
            name: "Close pane",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&CLOSE_PANE_IN_PANE_MODE);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for app to exit",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.snapshot_contains("Bye from Zellij!") {
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert!(last_snapshot.contains("Bye from Zellij!"));
}

#[test]
#[ignore]
pub fn resize_pane() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("resize_pane", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
                    // back to normal mode after resizing
                    remote_terminal.send_key(&ENTER);
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for pane to be resized",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(53, 2) && remote_terminal.tip_appears() {
                    // pane has been resized
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn lock_mode() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("lock_mode", fake_win_size, None)
        .add_step(Step {
            name: "Enter lock mode",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
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
                    remote_terminal.send_key(&"abc".as_bytes());
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .add_step(Step {
            name: "Wait for terminal to render sent keys",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(6, 2) {
                    // text has been entered into the only terminal pane
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn resize_terminal_window() {
    // this checks the resizing of the whole terminal window (reaction to SIGWINCH) and not just one pane
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let last_snapshot = RemoteRunner::new("resize_terminal_window", fake_win_size, None)
        .add_step(Step {
            name: "Split pane to the right",
            instruction: |mut remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2)
                {
                    remote_terminal.send_key(&PANE_MODE);
                    remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                    // back to normal mode after split
                    remote_terminal.send_key(&ENTER);
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
        })
        .add_step(Step {
            name: "wait for terminal to be resized and app to be re-rendered",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(43, 2) && remote_terminal.tip_appears() {
                    // size has been changed
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn detach_and_attach_session() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let session_id = rand::thread_rng().gen_range(0..10000);
    let session_name = format!("session_{}", session_id);
    let last_snapshot = RemoteRunner::new(
        "detach_and_attach_session",
        fake_win_size,
        Some(session_name),
    )
    .add_step(Step {
        name: "Split pane to the right",
        instruction: |mut remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.status_bar_appears() && remote_terminal.cursor_position_is(2, 2) {
                remote_terminal.send_key(&PANE_MODE);
                remote_terminal.send_key(&SPLIT_RIGHT_IN_PANE_MODE);
                // back to normal mode after split
                remote_terminal.send_key(&ENTER);
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
                remote_terminal.send_key(&"I am some text".as_bytes());
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
    })
    .add_step(Step {
        name: "Wait for session to be attached",
        instruction: |remote_terminal: RemoteTerminal| -> bool {
            let mut step_is_complete = false;
            if remote_terminal.cursor_position_is(77, 2) {
                // we're back inside the session
                step_is_complete = true;
            }
            step_is_complete
        },
    })
    .run_all_steps();
    assert_snapshot!(last_snapshot);
}

#[test]
#[ignore]
pub fn accepts_basic_layout() {
    let fake_win_size = PositionAndSize {
        cols: 120,
        rows: 24,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let layout_file_name = "three-panes-with-nesting.yaml";
    let last_snapshot = RemoteRunner::new_with_layout("accepts_basic_layout", fake_win_size, layout_file_name, None)
        .add_step(Step {
            name: "Wait for app to load",
            instruction: |remote_terminal: RemoteTerminal| -> bool {
                let mut step_is_complete = false;
                if remote_terminal.cursor_position_is(2, 0)
                    && remote_terminal.snapshot_contains("$ █                    │$")
                    && remote_terminal.snapshot_contains("$                                                                                                                       ") {
                    step_is_complete = true;
                }
                step_is_complete
            },
        })
        .run_all_steps();
    assert_snapshot!(last_snapshot);
}
