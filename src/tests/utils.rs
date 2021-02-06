use crate::panes::PositionAndSize;
use crate::panes::TerminalPane;

pub fn get_output_frame_snapshots(
    output_frames: &[Vec<u8>],
    win_size: &PositionAndSize,
) -> Vec<String> {
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let mut terminal_output = TerminalPane::new(main_pid, *win_size);

    let mut snapshots = vec![];
    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output, *byte);
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
    /// ctrl-g
    pub const COMMAND_TOGGLE: [u8; 1] = [7];
    /// b
    pub const SPLIT_HORIZONTALLY: [u8; 1] = [98];
    /// n
    pub const SPLIT_VERTICALLY: [u8; 1] = [110];
    /// j
    pub const RESIZE_DOWN: [u8; 1] = [106];
    /// k
    pub const RESIZE_UP: [u8; 1] = [107];
    /// p
    pub const MOVE_FOCUS: [u8; 1] = [112];
    /// h
    pub const RESIZE_LEFT: [u8; 1] = [104];
    /// l
    pub const RESIZE_RIGHT: [u8; 1] = [108];
    /// z
    pub const SPAWN_TERMINAL: [u8; 1] = [122];
    /// q
    pub const QUIT: [u8; 1] = [113];
    /// PgUp
    pub const SCROLL_UP: [u8; 4] = [27, 91, 53, 126];
    /// PgDn
    pub const SCROLL_DOWN: [u8; 4] = [27, 91, 54, 126];
    /// x
    pub const CLOSE_FOCUSED_PANE: [u8; 1] = [120];
    /// e
    pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN: [u8; 1] = [101];
    /// y
    pub const MOVE_FOCUS_LEFT: [u8; 1] = [121];
    /// u
    pub const MOVE_FOCUS_DOWN: [u8; 1] = [117];
    /// i
    pub const MOVE_FOCUS_UP: [u8; 1] = [105];
    /// o
    pub const MOVE_FOCUS_RIGHT: [u8; 1] = [111];
    /// 1
    pub const NEW_TAB: [u8; 1] = [49];
    /// 2
    pub const SWITCH_PREV_TAB: [u8; 1] = [50];
    /// 3
    pub const SWITCH_NEXT_TAB: [u8; 1] = [51];
    /// 4
    pub const CLOSE_TAB: [u8; 1] = [52];
}
