use zellij_utils::{vte, zellij_tile};

use zellij_server::{panes::TerminalPane, tab::Pane};
use zellij_tile::data::Palette;
use zellij_utils::pane_size::PositionAndSize;

pub fn get_output_frame_snapshots(
    output_frames: &[Vec<u8>],
    win_size: &PositionAndSize,
) -> Vec<String> {
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let mut terminal_output = TerminalPane::new(main_pid, *win_size, Palette::default());

    let mut snapshots = vec![];
    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output.grid, *byte);
        }
        let output_lines = terminal_output.read_buffer_as_lines();
        let cursor_coordinates = terminal_output.cursor_coordinates();
        let mut snapshot = String::new();
        for (line_index, line) in output_lines.iter().enumerate() {
            for (character_index, terminal_character) in line.iter().enumerate() {
                if let Some((cursor_x, cursor_y)) = cursor_coordinates {
                    if line_index == cursor_y && character_index == cursor_x {
                        snapshot.push('█');
                        continue;
                    }
                }
                snapshot.push(terminal_character.character);
            }
            if line_index != output_lines.len() - 1 {
                snapshot.push('\n');
            }
        }
        snapshots.push(snapshot);
    }
    snapshots
}

pub fn get_next_to_last_snapshot(mut snapshots: Vec<String>) -> Option<String> {
    if snapshots.len() < 2 {
        None
    } else {
        Some(snapshots.remove(snapshots.len() - 2))
    }
}

pub mod commands {
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
}
