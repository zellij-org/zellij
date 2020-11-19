use crate::terminal_pane::PositionAndSize;
use crate::terminal_pane::TerminalPane;

pub fn get_output_frame_snapshots(
    output_frames: &[Vec<u8>],
    win_size: &PositionAndSize,
) -> Vec<String> {
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let x = 0;
    let y = 0;
    let mut terminal_output = TerminalPane::new(main_pid, *win_size, x, y);

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

pub mod commands {
    pub const SPLIT_HORIZONTALLY: [u8; 10] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-b
    pub const SPLIT_VERTICALLY: [u8; 10] = [14, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-n
    pub const RESIZE_DOWN: [u8; 10] = [10, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-j
    pub const RESIZE_UP: [u8; 10] = [11, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-k
    pub const MOVE_FOCUS: [u8; 10] = [16, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-p
    pub const RESIZE_LEFT: [u8; 10] = [8, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-h
    pub const RESIZE_RIGHT: [u8; 10] = [12, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-l
    pub const SPAWN_TERMINAL: [u8; 10] = [26, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-z
    pub const QUIT: [u8; 10] = [17, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-q
    pub const SCROLL_UP: [u8; 10] = [27, 91, 53, 94, 0, 0, 0, 0, 0, 0]; // ctrl-PgUp
    pub const SCROLL_DOWN: [u8; 10] = [27, 91, 54, 94, 0, 0, 0, 0, 0, 0]; // ctrl-PgDown
    pub const CLOSE_FOCUSED_PANE: [u8; 10] = [24, 0, 0, 0, 0, 0, 0, 0, 0, 0]; // ctrl-x
    pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN: [u8; 10] = [5, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    // ctrl-e
}
