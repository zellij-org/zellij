use super::{InterceptorResult, KittyApcInterceptor};
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::sixel::SixelImageStore;
use crate::panes::terminal_pane::TerminalPane;
use crate::panes::LinkHandler;
use crate::tab::Pane;
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::data::{Palette, Style};
use zellij_utils::pane_size::PaneGeom;

#[derive(Default, Clone, PartialEq, Debug)]
struct RecordingPerformer {
    events: Vec<String>,
}

impl vte::Perform for RecordingPerformer {
    fn print(&mut self, c: char) {
        self.events.push(format!("print({:?})", c));
    }
    fn execute(&mut self, byte: u8) {
        self.events.push(format!("execute({})", byte));
    }
    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], ignore: bool, action: char) {
        let params: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();
        self.events.push(format!(
            "hook({:?},{:?},{},{:?})",
            params, intermediates, ignore, action
        ));
    }
    fn put(&mut self, byte: u8) {
        self.events.push(format!("put({})", byte));
    }
    fn unhook(&mut self) {
        self.events.push("unhook".to_string());
    }
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let params: Vec<Vec<u8>> = params.iter().map(|p| p.to_vec()).collect();
        self.events
            .push(format!("osc({:?},{})", params, bell_terminated));
    }
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        let params: Vec<Vec<u16>> = params.iter().map(|p| p.to_vec()).collect();
        self.events.push(format!(
            "csi({:?},{:?},{},{:?})",
            params, intermediates, ignore, action
        ));
    }
    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.events
            .push(format!("esc({:?},{},{})", intermediates, ignore, byte));
    }
}

fn feed(
    interceptor: &mut KittyApcInterceptor,
    parser: &mut vte::Parser,
    performer: &mut RecordingPerformer,
    captured: &mut Vec<Vec<u8>>,
    input: &[u8],
) {
    for byte in input {
        match interceptor.advance(*byte) {
            InterceptorResult::Forward(fwd) => {
                for b in fwd.as_slice() {
                    parser.advance(performer, std::slice::from_ref(b));
                }
            },
            InterceptorResult::Swallow => {},
            InterceptorResult::Captured(cmd) => captured.push(cmd),
        }
    }
}

fn run_through_interceptor(input: &[u8], chunk_size: usize) -> (Vec<Vec<u8>>, RecordingPerformer) {
    let mut interceptor = KittyApcInterceptor::new();
    let mut parser = vte::Parser::new();
    let mut performer = RecordingPerformer::default();
    let mut captured = Vec::new();
    for chunk in input.chunks(chunk_size) {
        feed(
            &mut interceptor,
            &mut parser,
            &mut performer,
            &mut captured,
            chunk,
        );
    }
    (captured, performer)
}

fn run_bare(input: &[u8]) -> RecordingPerformer {
    let mut parser = vte::Parser::new();
    let mut performer = RecordingPerformer::default();
    for byte in input {
        parser.advance(&mut performer, std::slice::from_ref(byte));
    }
    performer
}

#[test]
fn split_invariance_across_arbitrary_chunk_sizes() {
    let corpus: &[u8] = b"hello \x1b[31mred\x1b[0m \x1b_Ga=T,f=24,s=1,v=1;AAAA\x1b\\ mid \x1bPq#0;2;0;0;0-\x1b\\ \x1b]0;title\x07 \x1b_Xnot-kitty\x1b\\ tail";
    let (whole_captured, whole_performer) = run_through_interceptor(corpus, corpus.len());
    for chunk_size in [1, 2, 3, 7, corpus.len()] {
        let (captured, performer) = run_through_interceptor(corpus, chunk_size);
        assert_eq!(captured, whole_captured, "chunk_size {}", chunk_size);
        assert_eq!(performer, whole_performer, "chunk_size {}", chunk_size);
    }
    assert_eq!(whole_captured, vec![b"a=T,f=24,s=1,v=1;AAAA".to_vec()]);
}

#[test]
fn kitty_apc_is_isolated_from_vte() {
    let (captured, performer) = run_through_interceptor(b"AB\x1b_Gq=2,a=d,d=A\x1b\\CD", usize::MAX);
    assert_eq!(performer, run_bare(b"ABCD"));
    assert_eq!(captured, vec![b"q=2,a=d,d=A".to_vec()]);
}

#[test]
fn non_kitty_streams_pass_through_byte_identically() {
    let inputs: Vec<&[u8]> = vec![
        b"\x1b_Xsome payload\x1b\\after",
        b"\x1bPq#0;2;0;0;0#0~~@@vv@@~~@@~~$#1!14@\x1b\\",
        "h\u{e9}llo\u{2192}".as_bytes(),
        b"\x1b]0;t\x07",
    ];
    for input in inputs {
        let (captured, performer) = run_through_interceptor(input, usize::MAX);
        assert_eq!(performer, run_bare(input), "input {:?}", input);
        assert!(captured.is_empty(), "input {:?}", input);
    }
}

#[test]
fn lone_trailing_esc_is_held_across_calls() {
    let mut interceptor = KittyApcInterceptor::new();
    let mut parser = vte::Parser::new();
    let mut performer = RecordingPerformer::default();
    let mut captured = Vec::new();
    feed(
        &mut interceptor,
        &mut parser,
        &mut performer,
        &mut captured,
        b"A\x1b",
    );
    feed(
        &mut interceptor,
        &mut parser,
        &mut performer,
        &mut captured,
        b"_Gx\x1b\\B",
    );
    assert_eq!(captured, vec![b"x".to_vec()]);
    assert_eq!(
        performer.events,
        vec!["print('A')".to_string(), "print('B')".to_string()]
    );
}

#[test]
fn esc_inside_payload_stays_in_capture_buffer() {
    let (captured, performer) = run_through_interceptor(b"\x1b_Ga\x1bb\x1b\\Z", usize::MAX);
    assert_eq!(captured, vec![b"a\x1bb".to_vec()]);
    assert_eq!(performer.events, vec!["print('Z')".to_string()]);
}

#[test]
fn oversized_sequence_is_discarded_and_stream_recovers() {
    let mut input = Vec::new();
    input.extend_from_slice(b"\x1b_G");
    input.extend(std::iter::repeat(b'j').take(1_048_577));
    input.extend_from_slice(b"\x1b\\OK");
    let (captured, performer) = run_through_interceptor(&input, usize::MAX);
    assert!(captured.is_empty());
    assert_eq!(
        performer.events,
        vec!["print('O')".to_string(), "print('K')".to_string()]
    );
}

#[test]
fn can_and_sub_abort_capture_and_match_bare_vte() {
    for abort_byte in [0x18u8, 0x1au8] {
        let input = vec![0x1b, b'_', b'G', b'a', abort_byte, b'B'];
        let (captured, performer) = run_through_interceptor(&input, usize::MAX);
        assert!(captured.is_empty(), "abort_byte {}", abort_byte);
        assert_eq!(performer, run_bare(&input), "abort_byte {}", abort_byte);
    }
}

#[test]
fn c1_st_terminates_capture() {
    let (captured, performer) = run_through_interceptor(b"\x1b_Gx\x9cY", usize::MAX);
    assert_eq!(captured, vec![b"x".to_vec()]);
    assert_eq!(performer.events, vec!["print('Y')".to_string()]);
}

#[test]
fn esc_esc_underscore_sequences_replay_correctly() {
    let (captured, performer) = run_through_interceptor(b"\x1b\x1b_Gx\x1b\\A", usize::MAX);
    assert_eq!(captured, vec![b"x".to_vec()]);
    assert_eq!(performer, run_bare(b"\x1bA"));
}

#[test]
fn terminal_pane_grid_shows_ab_around_captured_apc() {
    let mut fake_win_size = PaneGeom::default();
    fake_win_size.cols.set_inner(121);
    fake_win_size.rows.set_inner(20);
    let pid = 1;
    let style = Style::default();
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let terminal_emulator_colors = Rc::new(RefCell::new(Palette::default()));
    let terminal_emulator_color_codes = Rc::new(RefCell::new(HashMap::new()));
    let debug = false;
    let arrow_fonts = true;
    let styled_underlines = true;
    let osc8_hyperlinks = true;
    let explicitly_disable_kitty_keyboard_protocol = false;
    let mut terminal_pane = TerminalPane::new(
        pid,
        fake_win_size,
        style,
        0,
        String::new(),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        sixel_image_store,
        Rc::new(RefCell::new(KittyImageStore::default())),
        terminal_emulator_colors,
        terminal_emulator_color_codes,
        None,
        None,
        debug,
        arrow_fonts,
        styled_underlines,
        osc8_hyperlinks,
        explicitly_disable_kitty_keyboard_protocol,
        None,
    );
    terminal_pane.handle_pty_bytes(Vec::from(&b"A\x1b_Gq=2,a=d,d=A\x1b\\B"[..]));
    assert_snapshot!(format!("{:?}", terminal_pane.grid));
    assert_eq!(terminal_pane.grid.kitty_commands_handled(), 1);
}
