//! Unit tests for the continuous host-reply parser.

use super::{schedule_forward_timeout, HostReply, HostReplyParser};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

#[test]
fn cross_chunk_osc_assembles_across_feeds() {
    // An OSC 11 reply split across two `feed()` calls. The inner
    // `InputParser` buffers OSC state across calls, so the second
    // chunk's terminator completes the sequence and the parser
    // emits a single classified reply. The first chunk produces no
    // reply; what lands in residue on the first chunk is byte-level
    // scrubber behaviour and intentionally not pinned here (see the
    // follow-up chunked_residue tests for those shapes).
    let mut parser = HostReplyParser::new();
    let first = parser.feed(b"\x1b]11;rgb:ffff/");
    assert!(
        first.replies.is_empty(),
        "first chunk must not classify a reply yet: {:?}",
        first.replies
    );

    let second = parser.feed(b"ffff/ffff\x1b\\");
    assert_eq!(
        second.replies.len(),
        1,
        "second chunk completes the OSC; parser emits one reply: {:?}",
        second.replies
    );
    match &second.replies[0] {
        HostReply::BackgroundColor(s) => assert_eq!(s, "rgb:ffff/ffff/ffff"),
        other => panic!("expected BackgroundColor, got {:?}", other),
    }
}

#[test]
fn double_dispatch_matrix_with_forward_active() {
    // For each whitelisted reply variant, opening a forward window and
    // feeding the reply must (a) classify the variant into
    // `ParseOutput.replies` and (b) accumulate the raw bytes into the
    // forward's reply buffer — both paths always fire, so cached state
    // and the forwarded-to pane stay in sync. OSC 11 is already
    // covered by `forwarding_window_accumulates_and_barrier_closes`;
    // this test sweeps OSC 10, OSC 4, CSI 14t / 16t replies, and
    // DECRPM 2026.
    let cases: Vec<(&[u8], fn(&HostReply) -> bool, &str)> = vec![
        (
            b"\x1b]10;rgb:1111/2222/3333\x1b\\",
            |r| matches!(r, HostReply::ForegroundColor(_)),
            "OSC 10",
        ),
        (
            b"\x1b]4;9;rgb:4444/5555/6666\x1b\\",
            |r| matches!(r, HostReply::ColorRegisters(_)),
            "OSC 4",
        ),
        (
            b"\x1b[4;720;1280t",
            |r| matches!(r, HostReply::PixelDimensions(_)),
            "CSI 14t reply",
        ),
        (
            b"\x1b[6;18;9t",
            |r| matches!(r, HostReply::PixelDimensions(_)),
            "CSI 16t reply",
        ),
        (
            b"\x1b[?2026;1$y",
            |r| matches!(r, HostReply::SynchronizedOutput(_)),
            "DECRPM 2026",
        ),
    ];
    for (bytes, is_expected_variant, label) in cases {
        let mut parser = HostReplyParser::new();
        parser.open_forward(11);
        let out = parser.feed(bytes);
        assert_eq!(
            out.replies.len(),
            1,
            "{}: should classify exactly one reply (got {:?})",
            label,
            out.replies
        );
        assert!(
            is_expected_variant(&out.replies[0]),
            "{}: wrong variant {:?}",
            label,
            out.replies[0]
        );
        assert!(
            out.completed_forward.is_none(),
            "{}: no barrier yet, slot must stay open",
            label
        );
        // Close the window to inspect the forward buffer.
        let (token, raw) = parser
            .close_forward_on_timeout(11)
            .expect("forward slot should still be open");
        assert_eq!(token, 11, "{}: token preserved", label);
        assert!(
            !raw.is_empty(),
            "{}: reply bytes must have been accumulated into the forward buffer",
            label
        );
    }
}

#[test]
fn primary_da_barrier_accepts_extended_forms() {
    // The barrier is "any CSI reply with final byte `c`". Real hosts
    // emit parameters (`\x1b[?62;1;6c`) or the secondary-DA-esque
    // prefix (`\x1b[>0;276;0c`). Both must close the forward window
    // the same way the bare `\x1b[c` does.
    for barrier in [
        b"\x1b[c".as_ref(),
        b"\x1b[?62;1;6c".as_ref(),
        b"\x1b[>0;276;0c".as_ref(),
    ] {
        let mut parser = HostReplyParser::new();
        parser.open_forward(5);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"\x1b]11;rgb:aaaa/bbbb/cccc\x1b\\");
        chunk.extend_from_slice(barrier);
        let out = parser.feed(&chunk);
        let (token, reply_bytes) = out.completed_forward.expect(
            "every Primary-DA reply form must close the forward window",
        );
        assert_eq!(token, 5);
        assert!(
            reply_bytes.windows(4).any(|w| w == b"]11;"),
            "OSC 11 should be present in the forwarded buffer for barrier {:?}",
            std::str::from_utf8(barrier).unwrap_or("<non-utf8>")
        );
        assert!(parser.active_forward_token().is_none());
    }
}

// =====================================================================
// Re-entry guard on `open_forward`
// =====================================================================

#[test]
#[should_panic(expected = "while slot for token")]
fn open_forward_debug_asserts_on_reentry() {
    // In debug builds (which is where tests run) the guard fires. This
    // catches a misbehaving server that dispatches a second forward
    // before receiving the first's completion — a scenario that should
    // be impossible given `forward_in_flight` serialization, but the
    // parser asserts it anyway so regressions surface in CI.
    let mut parser = HostReplyParser::new();
    parser.open_forward(1);
    parser.open_forward(2); // panics via debug_assert!
}

// =====================================================================
// `schedule_forward_timeout` — async timer, driven by paused tokio clock
// =====================================================================

/// Build a paused current-thread runtime with `enable_time()`. With
/// `start_paused(true)` the clock only advances when we explicitly
/// call `tokio::time::advance`, so tests don't wall-clock sleep.
fn paused_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("paused runtime build must succeed")
}

#[test]
fn timer_fires_after_deadline_and_closes_slot() {
    let rt = paused_runtime();
    let parser = Arc::new(Mutex::new(HostReplyParser::new()));
    parser.lock().unwrap().open_forward(7);

    let captured: Arc<Mutex<Option<(u32, Vec<u8>)>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    schedule_forward_timeout(
        rt.handle(),
        parser.clone(),
        7,
        Duration::from_millis(500),
        move |token, bytes| {
            *captured_clone.lock().unwrap() = Some((token, bytes));
        },
    );

    // Drive the runtime enough to run the just-spawned task up to its
    // sleep point, then jump past the deadline.
    rt.block_on(async {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(600)).await;
        // One more yield so the woken task can complete synchronously.
        tokio::task::yield_now().await;
    });

    let (token, bytes) = captured
        .lock()
        .unwrap()
        .take()
        .expect("timer must have invoked on_timeout");
    assert_eq!(token, 7);
    assert!(
        bytes.is_empty(),
        "nothing was fed to the parser → buffer is empty"
    );
    assert!(
        parser.lock().unwrap().active_forward_token().is_none(),
        "slot must have been cleared by the timer"
    );
}

#[test]
fn timer_is_noop_when_barrier_already_closed_the_slot() {
    let rt = paused_runtime();
    let parser = Arc::new(Mutex::new(HostReplyParser::new()));
    parser.lock().unwrap().open_forward(11);

    let fired: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let fired_clone = fired.clone();
    schedule_forward_timeout(
        rt.handle(),
        parser.clone(),
        11,
        Duration::from_millis(500),
        move |_, _| {
            *fired_clone.lock().unwrap() = true;
        },
    );

    // Before the deadline, simulate the barrier arriving: feed a reply
    // and the Primary-DA barrier; the parser's `completed_forward`
    // path clears the slot.
    {
        let mut p = parser.lock().unwrap();
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"\x1b]11;rgb:0/0/0\x1b\\");
        chunk.extend_from_slice(b"\x1b[c");
        let out = p.feed(&chunk);
        assert!(out.completed_forward.is_some(), "barrier should close slot");
    }

    // Now let the timer wake up — token-guard idempotency should make
    // it a no-op.
    rt.block_on(async {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(600)).await;
        tokio::task::yield_now().await;
    });

    assert!(
        !*fired.lock().unwrap(),
        "timer must not invoke on_timeout once the barrier has closed the slot"
    );
}

#[test]
fn timer_is_noop_when_slot_holds_a_different_token() {
    // This models re-entry *after* the server has moved on: the old
    // timer's token no longer matches the active slot, so
    // `close_forward_on_timeout(old_token)` returns None and the
    // callback doesn't fire.
    let rt = paused_runtime();
    let parser = Arc::new(Mutex::new(HostReplyParser::new()));
    parser.lock().unwrap().open_forward(1);

    let fired: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let fired_clone = fired.clone();
    schedule_forward_timeout(
        rt.handle(),
        parser.clone(),
        1,
        Duration::from_millis(500),
        move |_, _| {
            *fired_clone.lock().unwrap() = true;
        },
    );

    // Close slot for token 1 via the barrier path, then open a fresh
    // slot for a different token. The earlier spawned timer is still
    // sleeping and holds a snapshot of token=1.
    {
        let mut p = parser.lock().unwrap();
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"\x1b[c");
        let _ = p.feed(&chunk); // close via barrier
        p.open_forward(2);
    }

    rt.block_on(async {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(600)).await;
        tokio::task::yield_now().await;
    });

    assert!(
        !*fired.lock().unwrap(),
        "stale timer must not close the new slot"
    );
    assert_eq!(
        parser.lock().unwrap().active_forward_token(),
        Some(2),
        "new slot untouched"
    );
}

#[test]
fn timer_preserves_accumulated_reply_bytes_on_timeout() {
    // If the host went silent after emitting partial replies, the
    // timer still has to flush whatever landed in the buffer so the
    // pane sees *something* — empty is fine, partial is better.
    let rt = paused_runtime();
    let parser = Arc::new(Mutex::new(HostReplyParser::new()));
    parser.lock().unwrap().open_forward(22);

    // Simulate a single OSC 11 reply arriving before the host goes
    // silent.
    {
        let mut p = parser.lock().unwrap();
        let _ = p.feed(b"\x1b]11;rgb:1234/5678/9abc\x1b\\");
    }

    let captured: Arc<Mutex<Option<(u32, Vec<u8>)>>> = Arc::new(Mutex::new(None));
    let captured_clone = captured.clone();
    schedule_forward_timeout(
        rt.handle(),
        parser.clone(),
        22,
        Duration::from_millis(500),
        move |t, b| {
            *captured_clone.lock().unwrap() = Some((t, b));
        },
    );

    rt.block_on(async {
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_millis(600)).await;
        tokio::task::yield_now().await;
    });

    let (token, bytes) = captured
        .lock()
        .unwrap()
        .take()
        .expect("timer must fire");
    assert_eq!(token, 22);
    assert!(
        bytes.windows(4).any(|w| w == b"]11;"),
        "buffered OSC 11 bytes must appear in the flushed payload: {:?}",
        bytes
    );
}
