use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::{start, TerminalOutput};
use crate::tests::fakes::{FakeInputOutput};

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn starts_with_one_terminal () {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[17]); // quit (ctrl-q)
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let x = 0;
    let y = 0;
    let mut terminal_output = TerminalOutput::new(main_pid, fake_win_size, x, y);

    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output, *byte);
        }
        let output_lines = terminal_output.read_buffer_as_lines();
        let cursor_position_in_last_line = terminal_output.cursor_position_in_last_line();
        let mut snapshot = String::new();
        for (line_index, line) in output_lines.iter().enumerate() {
            for (character_index, terminal_character) in line.iter().enumerate() {
                if line_index == output_lines.len() - 1 && character_index == cursor_position_in_last_line {
                    snapshot.push('█');
                } else {
                    snapshot.push(terminal_character.character);
                }
            }
            if line_index != output_lines.len() - 1 {
                snapshot.push('\n');
            }
        }
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn split_terminals_vertically() {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[14, 17]); // split-vertically and quit (ctrl-n + ctrl-q)
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let x = 0;
    let y = 0;
    let mut terminal_output = TerminalOutput::new(main_pid, fake_win_size, x, y);

    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output, *byte);
        }
        let output_lines = terminal_output.read_buffer_as_lines();
        let cursor_position_in_last_line = terminal_output.cursor_position_in_last_line();
        let mut snapshot = String::new();
        for (line_index, line) in output_lines.iter().enumerate() {
            for (character_index, terminal_character) in line.iter().enumerate() {
                if line_index == output_lines.len() - 1 && character_index == cursor_position_in_last_line {
                    snapshot.push('█');
                } else {
                    snapshot.push(terminal_character.character);
                }
            }
            if line_index != output_lines.len() - 1 {
                snapshot.push('\n');
            }
        }
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn split_terminals_horizontally() {
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[2, 17]); // split-horizontally and quit (ctrl-b + ctrl-q)
    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let x = 0;
    let y = 0;
    let mut terminal_output = TerminalOutput::new(main_pid, fake_win_size, x, y);

    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output, *byte);
        }
        let output_lines = terminal_output.read_buffer_as_lines();
        let cursor_position_in_last_line = terminal_output.cursor_position_in_last_line();
        let mut snapshot = String::new();
        for (line_index, line) in output_lines.iter().enumerate() {
            for (character_index, terminal_character) in line.iter().enumerate() {
                if line_index == output_lines.len() - 1 && character_index == cursor_position_in_last_line {
                    snapshot.push('█');
                } else {
                    snapshot.push(terminal_character.character);
                }
            }
            if line_index != output_lines.len() - 1 {
                snapshot.push('\n');
            }
        }
        assert_snapshot!(snapshot);
    }
}

#[test]
pub fn resize_right_and_up_on_the_same_axis() {
    // this is a specific test to explicitly ensure that a tmux-like pane-container algorithm is not
    // implemented (this test can never pass with such an algorithm)
    //
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   │     │     │
    // ├─────┼─────┤ ==resize=right==> ├─────┴─┬───┤ ==resize-left==>
    // │█████│     │                   │███████│   │
    // └─────┴─────┘                   └───────┴───┘
    //
    // ┌─────┬─────┐                   ┌─────┬─────┐
    // │     │     │                   ├─────┤     │
    // ├─────┼─────┤ ==resize=up==>    │█████├─────┤
    // │█████│     │                   │█████│     │
    // └─────┴─────┘                   └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b + ctrl-n + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-l + ctrl-h + ctrl-k + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 14, 16, 14, 16, 16, 12, 8, 11, 17]);

    start(Box::new(fake_input_output.clone()));

    let output_frames = fake_input_output.stdout_writer.output_frames.lock().unwrap();
    let mut vte_parser = vte::Parser::new();
    let main_pid = 0;
    let x = 0;
    let y = 0;
    let mut terminal_output = TerminalOutput::new(main_pid, fake_win_size, x, y);

    for frame in output_frames.iter() {
        for byte in frame.iter() {
            vte_parser.advance(&mut terminal_output, *byte);
        }
        let output_lines = terminal_output.read_buffer_as_lines();
        let (cursor_x, cursor_y) = terminal_output.cursor_coordinates();
        let mut snapshot = String::new();
        for (line_index, line) in output_lines.iter().enumerate() {
            for (character_index, terminal_character) in line.iter().enumerate() {
                if line_index == cursor_y - 1 && character_index == cursor_x {
                    snapshot.push('█');
                } else {
                    snapshot.push(terminal_character.character);
                }
            }
            if line_index != output_lines.len() - 1 {
                snapshot.push('\n');
            }
        }
        assert_snapshot!(snapshot);
    }
}

