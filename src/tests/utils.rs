use crate::terminal_pane::TerminalPane;
use ::nix::pty::Winsize;

pub fn get_output_frame_snapshots(output_frames: &[Vec<u8>], win_size: &Winsize) -> Vec<String> {
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
    /// ctrl-g
    pub const COMMAND_TOGGLE: [u8; 10] = [7, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// b
    pub const SPLIT_HORIZONTALLY: [u8; 10] = [98, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// n
    pub const SPLIT_VERTICALLY: [u8; 10] = [110, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// j
    pub const RESIZE_DOWN: [u8; 10] = [106, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// k
    pub const RESIZE_UP: [u8; 10] = [107, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// p
    pub const MOVE_FOCUS: [u8; 10] = [112, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// h
    pub const RESIZE_LEFT: [u8; 10] = [104, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// l
    pub const RESIZE_RIGHT: [u8; 10] = [108, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// z
    pub const SPAWN_TERMINAL: [u8; 10] = [122, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// q
    pub const QUIT: [u8; 10] = [113, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// PgUp
    pub const SCROLL_UP: [u8; 10] = [27, 91, 53, 126, 0, 0, 0, 0, 0, 0];
    /// PgDn
    pub const SCROLL_DOWN: [u8; 10] = [27, 91, 54, 126, 0, 0, 0, 0, 0, 0];
    /// x
    pub const CLOSE_FOCUSED_PANE: [u8; 10] = [120, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    /// e
    pub const TOGGLE_ACTIVE_TERMINAL_FULLSCREEN: [u8; 10] = [101, 0, 0, 0, 0, 0, 0, 0, 0, 0];
}
