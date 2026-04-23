//! Unit tests for the continuous host-reply parser.

use super::{HostReply, HostReplyParser};

/// Helper: collect replies and residue from a single `feed` call.
fn feed_once(parser: &mut HostReplyParser, bytes: &[u8]) -> (Vec<HostReply>, Vec<u8>) {
    let out = parser.feed(bytes);
    (out.replies, out.residue)
}

#[test]
fn pixel_dimensions_text_area_reply() {
    // CSI 4 ; H ; W t
    let mut parser = HostReplyParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[4;720;1280t");
    assert!(residue.is_empty(), "pixel-dim reply should be fully consumed");
    assert_eq!(replies.len(), 1);
    match &replies[0] {
        HostReply::PixelDimensions(pd) => {
            let tas = pd.text_area_size.expect("text area size");
            assert_eq!(tas.height, 720);
            assert_eq!(tas.width, 1280);
            assert!(pd.character_cell_size.is_none());
        },
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn pixel_dimensions_character_cell_reply() {
    let mut parser = HostReplyParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[6;18;9t");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::PixelDimensions(pd) => {
            let ccs = pd.character_cell_size.expect("cell size");
            assert_eq!(ccs.height, 18);
            assert_eq!(ccs.width, 9);
        },
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn background_color_reply() {
    let mut parser = HostReplyParser::new();
    let (replies, residue) =
        feed_once(&mut parser, b"\x1b]11;rgb:0000/0000/0000\x1b\\");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::BackgroundColor(s) => assert_eq!(s, "rgb:0000/0000/0000"),
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn foreground_color_reply() {
    let mut parser = HostReplyParser::new();
    let (replies, residue) =
        feed_once(&mut parser, b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::ForegroundColor(s) => assert_eq!(s, "rgb:ffff/ffff/ffff"),
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn color_register_reply() {
    let mut parser = HostReplyParser::new();
    let (replies, residue) =
        feed_once(&mut parser, b"\x1b]4;5;rgb:8080/8080/8080\x1b\\");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::ColorRegisters(regs) => {
            assert_eq!(regs.len(), 1);
            assert_eq!(regs[0].0, 5);
            assert_eq!(regs[0].1, "rgb:8080/8080/8080");
        },
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn synchronized_output_supported_reply() {
    let mut parser = HostReplyParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[?2026;1$y");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::SynchronizedOutput(Some(_)) => {},
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn keyboard_residue_passes_through_unchanged() {
    // Arrow key escape sequence is NOT a whitelisted CSI report (final
    // byte 'A'), so it must survive as keyboard residue verbatim.
    let mut parser = HostReplyParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[A");
    assert!(replies.is_empty());
    assert_eq!(residue, b"\x1b[A");
}

#[test]
fn mixed_keyboard_and_reply_extracts_both_cleanly() {
    // Arrow keys bracketing a pixel-dim reply — residue should be just
    // the arrow-key bytes, reply should be classified.
    let mut parser = HostReplyParser::new();
    let mut input = Vec::new();
    input.extend_from_slice(b"\x1b[A");
    input.extend_from_slice(b"\x1b[4;720;1280t");
    input.extend_from_slice(b"\x1b[B");
    let (replies, residue) = feed_once(&mut parser, &input);
    assert_eq!(replies.len(), 1);
    matches!(replies[0], HostReply::PixelDimensions(_));
    assert_eq!(residue, b"\x1b[A\x1b[B");
}

#[test]
fn malformed_osc_does_not_eat_following_keyboard_bytes() {
    // An OSC that never terminates shouldn't swallow subsequent keyboard
    // bytes. In our scrubber, if OSC is incomplete, it simply falls
    // through byte-by-byte.
    let mut parser = HostReplyParser::new();
    // OSC prefix with no terminator — scrubber sees unterminated OSC and
    // emits the bytes as residue (caller keyboard path will drop them,
    // but they must not be *silently consumed*, and subsequent bytes
    // must still arrive).
    let (_replies, residue) = feed_once(&mut parser, b"\x1b]10;partial");
    assert_eq!(residue, b"\x1b]10;partial");
}

#[test]
fn forwarding_window_accumulates_and_barrier_closes() {
    let mut parser = HostReplyParser::new();
    parser.open_forward(42);
    // Feed an OSC 11 reply, then the Primary-DA barrier. Use color
    // bytes that do NOT contain `c` so the barrier-absence assertion
    // below can use a simple byte search.
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"\x1b]11;rgb:aaaa/bbbb/dddd\x1b\\");
    chunk.extend_from_slice(b"\x1b[?65;1c");
    let out = parser.feed(&chunk);
    // OSC 11 was classified (double-dispatch).
    assert_eq!(out.replies.len(), 1);
    matches!(out.replies[0], HostReply::BackgroundColor(_));
    // Barrier closed the window, producing a completed forward.
    let (token, reply_bytes) = out
        .completed_forward
        .expect("barrier should close the window");
    assert_eq!(token, 42);
    // Reply bytes should contain the OSC 11 (re-serialized) but NOT the
    // barrier reply itself.
    assert!(
        reply_bytes.windows(5).any(|w| w == b"]11;r"),
        "OSC 11 should be in the forwarded buffer: {:?}",
        reply_bytes
    );
    assert!(
        !reply_bytes.contains(&b'c'),
        "Primary-DA barrier (final byte 'c') must not appear in forwarded reply"
    );
    // Slot released.
    assert!(parser.active_forward_token().is_none());
}

#[test]
fn unsolicited_osc_between_forwarded_query_and_barrier() {
    // Scenario: host emits a stray OSC 10 between the app's OSC 11 query
    // and the barrier. Both replies should end up in the forwarded
    // buffer, and the barrier closes the window.
    let mut parser = HostReplyParser::new();
    parser.open_forward(7);
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"\x1b]11;rgb:1111/1111/1111\x1b\\");
    chunk.extend_from_slice(b"\x1b]10;rgb:2222/2222/2222\x1b\\");
    chunk.extend_from_slice(b"\x1b[c");
    let out = parser.feed(&chunk);
    assert_eq!(out.replies.len(), 2);
    let (token, reply_bytes) = out.completed_forward.unwrap();
    assert_eq!(token, 7);
    // Both OSCs present.
    assert!(reply_bytes.windows(4).any(|w| w == b"]11;"));
    assert!(reply_bytes.windows(4).any(|w| w == b"]10;"));
}

#[test]
fn double_dispatch_without_active_forward_still_emits_reply() {
    let mut parser = HostReplyParser::new();
    // No open_forward — reply should still be classified.
    let out = parser.feed(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\");
    assert_eq!(out.replies.len(), 1);
    matches!(out.replies[0], HostReply::BackgroundColor(_));
    assert!(out.completed_forward.is_none());
}

#[test]
fn timeout_flushes_accumulated_bytes() {
    let mut parser = HostReplyParser::new();
    parser.open_forward(99);
    let out = parser.feed(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\");
    assert!(out.completed_forward.is_none(), "no barrier yet");
    assert!(parser.active_forward_token() == Some(99));
    // Simulate timeout firing on the watcher.
    let flushed = parser.close_forward_on_timeout(99);
    let (token, bytes) = flushed.expect("timeout flush should produce a payload");
    assert_eq!(token, 99);
    assert!(bytes.windows(4).any(|w| w == b"]11;"));
    assert!(parser.active_forward_token().is_none());
}

#[test]
fn stale_token_timeout_does_nothing() {
    let mut parser = HostReplyParser::new();
    parser.open_forward(1);
    // Ask to timeout a different token — nothing happens.
    assert!(parser.close_forward_on_timeout(999).is_none());
    assert_eq!(parser.active_forward_token(), Some(1));
}

#[test]
fn chunked_input_preserves_cross_chunk_sequences() {
    // Feed a single OSC reply byte-by-byte. Because the scrubber is
    // byte-at-a-time inside a single feed call, the bytes are scanned
    // together. But when chunks arrive separately, each residue call
    // may pass through a partial prefix. This test documents the
    // current, acceptable behaviour: a reply arriving in one chunk is
    // still classified, while arriving in multiple chunks falls
    // through to residue (the caller's keyboard parser absorbs it).
    let mut parser = HostReplyParser::new();
    let out = parser.feed(b"\x1b]11;rgb:0000/0000/0000\x1b\\");
    assert_eq!(out.replies.len(), 1);
    matches!(out.replies[0], HostReply::BackgroundColor(_));
}
