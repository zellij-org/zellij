use ::nix::pty::Winsize;
use ::insta::assert_snapshot;

use crate::{start, TerminalOutput};
use crate::tests::fakes::{FakeInputOutput};

fn get_fake_os_input (fake_win_size: &Winsize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn resize_left_with_pane_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[14, 8, 17]); // split-vertically, resize-left and quit (ctrl-n + ctrl-h + ctrl-q)
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
pub fn resize_left_with_pane_to_the_right() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // │█████│     │  ==resize=left==>  │███│       │
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[14, 16, 8, 17]); // split-vertically, change-focus resize-left and quit (ctrl-n + ctrl-p + ctrl-h + ctrl-q)
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
pub fn resize_left_with_panes_to_the_left_and_right() {
    // ┌─────┬─────┬─────┐                    ┌─────┬───┬───────┐
    // │     │█████│     │                    │     │███│       │
    // │     │█████│     │  ==resize=left==>  │     │███│       │
    // │     │█████│     │                    │     │███│       │
    // └─────┴─────┴─────┘                    └─────┴───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    // split-vertically * 2, change-focus * 2, resize-right and quit (ctrl-n * 2 + ctrl-p * 2 + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 14, 16, 16, 8, 17]);
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
pub fn resize_left_with_multiple_panes_to_the_left() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┤█████│  ==resize=left==>  ├───┤███████│
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-p + ctrl-b + ctrl-p * 2 + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 16, 2, 16, 16, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │     │█████│                    │   │███████│
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p * 3, ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 16, 16, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤  ==resize=left==>  ├───┬─┴─────┤
    // │█████│     │                    │███│       │
    // └─────┴─────┘                    └───┴───────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │     │█████│                    │   │███████│
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p * 2, ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 16, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌───┬───────┐
    // │█████│     │                    │███│       │
    // ├─────┼─────┤  ==resize=left==>  ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // split-vertically, split_horizontally, change-focus, split-horizontally, resize-right and quit
    // (ctrl-n + ctrl-b + ctrl-p + ctrl-b + ctrl-p, ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[14, 2, 16, 2, 16, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b * 2 + ctrl-n + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-n + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 2, 14, 16, 14, 16, 16, 14, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_current_pane() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // │     │     │                    │     │     │
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // │     │     │                    │     │     │
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 20,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b * 2 + ctrl-n + ctrl-p + ctrl-n + ctrl-p * 2 + ctrl-n + ctrl-p * 2 + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 2, 14, 16, 14, 16, 16, 14, 16, 16, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_left_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // │     ├─────┤                    │   ├───────┤
    // │     │█████│  ==resize=left==>  │   │███████│
    // │     ├─────┤                    │   ├───────┤
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize {
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b * 2 + ctrl-p + ctrl-k * 3 + ctrl-n + ctrl-p * 3 + ctrl-n + ctrl-p * 2 + ctrl-n +
    // ctrl-b * 2 + ctrl-p * 7 + ctrl-k * 2 + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 2, 16, 11, 11, 11, 14, 16, 16, 16, 14, 16, 16, 14, 2, 2, 16, 16, 16, 16, 16, 16, 16, 11, 11, 8, 17]);

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

#[test]
pub fn resize_left_with_panes_to_the_right_aligned_top_and_bottom_with_panes_above_and_below() {
    // ┌─────┬─────┐                    ┌─────┬─────┐
    // ├─────┼─────┤                    ├───┬─┴─────┤
    // ├─────┤     │                    ├───┤       │
    // │█████│     │  ==resize=left==>  │███│       │
    // ├─────┤     │                    ├───┤       │
    // ├─────┼─────┤                    ├───┴─┬─────┤
    // └─────┴─────┘                    └─────┴─────┘
    // █ == focused pane
    let fake_win_size = Winsize { // TODO: combine with above
        ws_col: 121,
        ws_row: 40,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);

    // (ctrl-b * 2 + ctrl-p + ctrl-k * 3 + ctrl-n + ctrl-p * 3 + ctrl-n + ctrl-p * 2 + ctrl-n +
    // ctrl-p * 2 +
    // ctrl-b * 2 + ctrl-p * 7 + ctrl-k * 2 + ctrl-h + ctrl-q)
    fake_input_output.add_terminal_input(&[2, 2, 16, 11, 11, 11, 14, 16, 16, 16, 14, 16, 16, 14, 16, 16, 2, 2, 16, 16, 16, 16, 16, 16, 16, 11, 11, 8, 17]);

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
