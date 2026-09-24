use std::path::Path;

use anyhow::Result;
use zellij_utils::ipc::ServerToClientMsg;

use crate::fixture::{self, Fixture};
use crate::terminal::TerminalState;

pub struct ReplayOutcome {
    pub state: TerminalState,
    pub applied_renders: usize,
    pub discarded_ansi: usize,
    pub stopped_at_exit: bool,
}

pub fn replay_file(path: &Path) -> Result<ReplayOutcome> {
    Ok(replay(&fixture::read(path)?))
}

pub fn replay(fixture: &Fixture) -> ReplayOutcome {
    let mut state = TerminalState::new(fixture.header.rows, fixture.header.cols);
    state.set_cell_size(
        fixture.header.cell_width as u32,
        fixture.header.cell_height as u32,
    );
    let mut applied_renders = 0usize;
    let mut discarded_ansi = 0usize;
    let mut stopped_at_exit = false;

    for message in &fixture.messages {
        match &message.msg {
            ServerToClientMsg::RenderFrame { frame } => match state.apply_frame(frame) {
                Ok(_) => applied_renders += 1,
                Err(e) => eprintln!("zellij-window: unusable frame in fixture: {}", e),
            },
            ServerToClientMsg::Render { .. } => discarded_ansi += 1,
            ServerToClientMsg::Exit { .. } => {
                stopped_at_exit = true;
                break;
            },
            _ => {},
        }
    }

    ReplayOutcome {
        state,
        applied_renders,
        discarded_ansi,
        stopped_at_exit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{Header, Message, FIXTURE_FORMAT_VERSION};
    use zellij_utils::consts::CLIENT_SERVER_CONTRACT_VERSION;
    use zellij_utils::ipc::ExitReason;
    use zellij_utils::structured_render::{FrameBuilder, WireCell};

    fn fixture_of(msgs: Vec<ServerToClientMsg>) -> Fixture {
        Fixture {
            header: Header {
                format_version: FIXTURE_FORMAT_VERSION,
                contract_version: CLIENT_SERVER_CONTRACT_VERSION,
                session: "window-test".to_owned(),
                rows: 3,
                cols: 6,
                cell_width: 10,
                cell_height: 20,
                recorded_at_unix_secs: 0,
            },
            messages: msgs
                .into_iter()
                .enumerate()
                .map(|(index, msg)| Message {
                    ts_us: index as u64,
                    msg,
                })
                .collect(),
        }
    }

    fn frame(seq: u64, col: u16, text: &str) -> ServerToClientMsg {
        let mut builder = FrameBuilder::new(6, 3, seq);
        let cells: Vec<WireCell> = text
            .chars()
            .map(|character| WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            })
            .collect();
        builder.push_row(0, col, &cells);
        ServerToClientMsg::RenderFrame {
            frame: builder.finish(),
        }
    }

    #[test]
    fn the_screen_is_sized_from_the_fixture_header() {
        let outcome = replay(&fixture_of(vec![]));
        assert_eq!(outcome.state.size().rows, 3);
        assert_eq!(outcome.state.size().cols, 6);
    }

    #[test]
    fn frames_are_applied_in_order() {
        let outcome = replay(&fixture_of(vec![frame(0, 0, "ab"), frame(1, 2, "cd")]));
        assert_eq!(outcome.applied_renders, 2);
        assert_eq!(crate::dump::grid_rows(&outcome.state)[0], "abcd  ");
    }

    #[test]
    fn an_ansi_payload_is_counted_and_discarded() {
        let outcome = replay(&fixture_of(vec![
            ServerToClientMsg::Render {
                content: "\u{1b}[31mnever parsed".to_owned(),
            },
            frame(0, 0, "ab"),
        ]));
        assert_eq!(outcome.discarded_ansi, 1);
        assert_eq!(outcome.applied_renders, 1);
        assert_eq!(crate::dump::grid_rows(&outcome.state)[0], "ab    ");
    }

    #[test]
    fn an_unusable_frame_is_skipped_rather_than_aborting_the_replay() {
        let outcome = replay(&fixture_of(vec![
            ServerToClientMsg::RenderFrame {
                frame: b"nonsense".to_vec(),
            },
            frame(0, 0, "ab"),
        ]));
        assert_eq!(outcome.applied_renders, 1);
        assert_eq!(crate::dump::grid_rows(&outcome.state)[0], "ab    ");
    }

    #[test]
    fn non_render_messages_are_skipped_and_exit_ends_the_replay() {
        let outcome = replay(&fixture_of(vec![
            ServerToClientMsg::UnblockInputThread,
            frame(0, 0, "ab"),
            ServerToClientMsg::QueryTerminalSize,
            ServerToClientMsg::Exit {
                exit_reason: ExitReason::Normal,
            },
            frame(1, 2, "NEVER"),
        ]));
        assert!(outcome.stopped_at_exit);
        assert_eq!(outcome.applied_renders, 1);
        assert_eq!(crate::dump::grid_rows(&outcome.state)[0], "ab    ");
    }

    #[test]
    fn a_fixture_without_an_exit_replays_to_the_end() {
        let outcome = replay(&fixture_of(vec![frame(0, 0, "ab")]));
        assert!(!outcome.stopped_at_exit);
        assert_eq!(outcome.applied_renders, 1);
    }
}
