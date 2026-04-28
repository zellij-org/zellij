//! Unit tests for the continuous host-reply parser.

use super::{schedule_forward_timeout, HostReply, StdinAnsiParser};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Helper: collect replies and residue from a single `feed` call.
fn feed_once(parser: &mut StdinAnsiParser, bytes: &[u8]) -> (Vec<HostReply>, Vec<u8>) {
    let out = parser.feed(bytes);
    (out.replies, out.residue)
}

#[test]
fn pixel_dimensions_text_area_reply() {
    // CSI 4 ; H ; W t
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[4;720;1280t");
    assert!(
        residue.is_empty(),
        "pixel-dim reply should be fully consumed"
    );
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
    let mut parser = StdinAnsiParser::new();
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
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b]11;rgb:0000/0000/0000\x1b\\");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::BackgroundColor(s) => assert_eq!(s, "rgb:0000/0000/0000"),
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn foreground_color_reply() {
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b]10;rgb:ffff/ffff/ffff\x1b\\");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::ForegroundColor(s) => assert_eq!(s, "rgb:ffff/ffff/ffff"),
        other => panic!("unexpected reply: {:?}", other),
    }
}

#[test]
fn color_register_reply() {
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b]4;5;rgb:8080/8080/8080\x1b\\");
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
    let mut parser = StdinAnsiParser::new();
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
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[A");
    assert!(replies.is_empty());
    assert_eq!(residue, b"\x1b[A");
}

#[test]
fn mixed_keyboard_and_reply_extracts_both_cleanly() {
    // Arrow keys bracketing a pixel-dim reply — residue should be just
    // the arrow-key bytes, reply should be classified.
    let mut parser = StdinAnsiParser::new();
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
fn unterminated_osc_within_single_chunk_is_buffered() {
    // An OSC that never terminates within a chunk is held in the
    // partial-OSC buffer for the next call to complete; nothing leaks
    // into residue. This is the cross-chunk-aware replacement for the
    // pre-fix behaviour where unterminated OSC bytes fell through to
    // residue and surfaced as spurious keypresses.
    let mut parser = StdinAnsiParser::new();
    let (_replies, residue) = feed_once(&mut parser, b"\x1b]10;partial");
    assert!(
        residue.is_empty(),
        "unterminated OSC must be buffered, not leaked to residue: {:?}",
        residue
    );
}

#[test]
fn forwarding_window_accumulates_and_barrier_closes() {
    let mut parser = StdinAnsiParser::new();
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
    let mut parser = StdinAnsiParser::new();
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
    let mut parser = StdinAnsiParser::new();
    // No open_forward — reply should still be classified.
    let out = parser.feed(b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\");
    assert_eq!(out.replies.len(), 1);
    matches!(out.replies[0], HostReply::BackgroundColor(_));
    assert!(out.completed_forward.is_none());
}

#[test]
fn timeout_flushes_accumulated_bytes() {
    let mut parser = StdinAnsiParser::new();
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
    let mut parser = StdinAnsiParser::new();
    parser.open_forward(1);
    // Ask to timeout a different token — nothing happens.
    assert!(parser.close_forward_on_timeout(999).is_none());
    assert_eq!(parser.active_forward_token(), Some(1));
}

#[test]
fn fragmented_osc_does_not_leak_into_residue() {
    let full = b"\x1b]11;rgb:0000/0000/0000\x1b\\";
    for split in 1..full.len() {
        let mut p = StdinAnsiParser::new();
        let r1 = p.feed(&full[..split]);
        let r2 = p.feed(&full[split..]);
        assert!(
            r1.residue.is_empty(),
            "split at {}: chunk 1 residue should be empty, got {:?}",
            split,
            r1.residue
        );
        assert!(
            r2.residue.is_empty(),
            "split at {}: chunk 2 residue should be empty, got {:?}",
            split,
            r2.residue
        );
        assert_eq!(
            r1.replies.len() + r2.replies.len(),
            1,
            "split at {}: exactly one reply across both chunks",
            split
        );
    }
}

#[test]
fn fragmented_csi_report_does_not_leak() {
    // Pixel-dimensions reply (final byte 't').
    let full = b"\x1b[4;800;1200t";
    for split in 1..full.len() {
        let mut p = StdinAnsiParser::new();
        let r1 = p.feed(&full[..split]);
        let r2 = p.feed(&full[split..]);
        assert!(
            r1.residue.is_empty(),
            "split at {}: c1 residue {:?}",
            split,
            r1.residue
        );
        assert!(
            r2.residue.is_empty(),
            "split at {}: c2 residue {:?}",
            split,
            r2.residue
        );
        assert_eq!(r1.replies.len() + r2.replies.len(), 1);
    }
}

#[test]
fn fragmented_osc_byte_by_byte() {
    // Every byte in its own chunk — the worst case.
    let full = b"\x1b]11;rgb:abcd/ef01/2345\x1b\\";
    let mut p = StdinAnsiParser::new();
    let mut total_replies = 0;
    for &b in full {
        let out = p.feed(&[b]);
        assert!(out.residue.is_empty(), "byte 0x{:02x} leaked to residue", b);
        total_replies += out.replies.len();
    }
    assert_eq!(total_replies, 1);
}

#[test]
fn lone_trailing_esc_is_buffered_then_finalized_as_residue() {
    // A bare ESC byte at the tail of a chunk could be the start of an
    // OSC or CSI host-reply that's been fragmented at the ESC
    // boundary, so `feed` parks it under partial state instead of
    // leaking it as a keyboard residue. But the byte must not stay
    // parked forever — `finalize()` is the idle drain that releases
    // it back to the keyboard parser when no follow-up arrives.
    let mut p = StdinAnsiParser::new();
    let out = p.feed(b"\x1b");
    assert!(
        out.residue.is_empty(),
        "lone ESC must not leak immediately: {:?}",
        out.residue
    );
    assert!(
        out.has_partial_state,
        "lone ESC must mark has_partial_state so the caller schedules a finalize tick"
    );
    let drained = p.finalize();
    assert_eq!(
        drained,
        vec![0x1b],
        "finalize must release the parked ESC as keyboard residue"
    );
    // Subsequent finalize is a no-op once the parker is empty.
    assert!(p.finalize().is_empty());
}

#[test]
fn fragmented_osc_does_not_finalize_partial() {
    // Inverse case: a real fragmented host-reply (ESC then `]...`
    // arrived in two chunks with no idle between them) must NOT be
    // released by finalize. The key signal is timing — if the second
    // chunk arrives quickly enough the idle path is never taken, and
    // the OSC completes normally. We exercise that order here.
    let full = b"\x1b]11;rgb:0000/0000/0000\x1b\\";
    let mut p = StdinAnsiParser::new();
    let r1 = p.feed(&full[..1]);
    assert!(r1.residue.is_empty());
    assert!(r1.has_partial_state);
    let r2 = p.feed(&full[1..]);
    assert!(r2.residue.is_empty(), "tail must complete the OSC");
    assert_eq!(r1.replies.len() + r2.replies.len(), 1);
    assert!(p.finalize().is_empty(), "no partial left after completion");
}

#[test]
fn partial_osc_overflow_falls_back_to_residue() {
    // An unterminated OSC larger than the cap must not grow memory
    // unbounded. After the cap is hit the buffered bytes flush to
    // residue and parsing resumes from a clean state.
    let mut p = StdinAnsiParser::new();
    let _ = p.feed(b"\x1b]52;c;");
    let chunk = vec![b'A'; 1024 * 1024]; // 1 MB
    let mut total_residue = 0usize;
    for _ in 0..110 {
        let out = p.feed(&chunk);
        total_residue += out.residue.len();
    }
    assert!(
        total_residue > 0,
        "overflowed partial buffer should flush to residue, not silently grow"
    );
}

#[test]
fn osc_99_routes_into_desktop_notifications() {
    // OSC 99 is the desktop-notification response. It must surface in
    // ParseOutput.desktop_notifications (where the stdin handler routes
    // it as InputInstruction::DesktopNotificationResponse), NOT in
    // residue (which would never reach the keyboard parser anyway,
    // since the scrubber strips OSC bytes).
    let mut p = StdinAnsiParser::new();
    let out = p.feed(b"\x1b]99;notification body\x1b\\");
    assert!(out.residue.is_empty(), "OSC 99 must not leak to residue");
    assert!(out.replies.is_empty(), "OSC 99 is not a HostReply variant");
    assert_eq!(out.desktop_notifications.len(), 1);
    assert_eq!(out.desktop_notifications[0], b"notification body".to_vec());
}

#[test]
fn fragmented_osc_99_emits_one_notification() {
    // Cross-chunk regression: OSC 99 split across two feed() calls
    // must still emit exactly one notification, with no leak into
    // residue.
    let full = b"\x1b]99;hello world\x1b\\";
    for split in 1..full.len() {
        let mut p = StdinAnsiParser::new();
        let r1 = p.feed(&full[..split]);
        let r2 = p.feed(&full[split..]);
        assert!(r1.residue.is_empty(), "split at {}: c1 residue", split);
        assert!(r2.residue.is_empty(), "split at {}: c2 residue", split);
        let total = r1.desktop_notifications.len() + r2.desktop_notifications.len();
        assert_eq!(total, 1, "split at {}: exactly one notification", split);
    }
}

#[test]
fn malformed_osc_still_does_not_eat_following_keyboard_bytes() {
    // Pre-existing invariant. An unterminated OSC followed (in a later
    // chunk) by plain keyboard input — the keyboard input must reach
    // residue intact once the OSC closes via proper flush.
    let mut p = StdinAnsiParser::new();
    let _ = p.feed(b"\x1b]10;partial");
    let _ = p.feed(b"\x1b\\");
    let out = p.feed(b"hello");
    assert_eq!(out.residue, b"hello");
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
    let mut parser = StdinAnsiParser::new();
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
        let mut parser = StdinAnsiParser::new();
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
        let mut parser = StdinAnsiParser::new();
        parser.open_forward(5);
        let mut chunk = Vec::new();
        chunk.extend_from_slice(b"\x1b]11;rgb:aaaa/bbbb/cccc\x1b\\");
        chunk.extend_from_slice(barrier);
        let out = parser.feed(&chunk);
        let (token, reply_bytes) = out
            .completed_forward
            .expect("every Primary-DA reply form must close the forward window");
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
    let mut parser = StdinAnsiParser::new();
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
    let parser = Arc::new(Mutex::new(StdinAnsiParser::new()));
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
    let parser = Arc::new(Mutex::new(StdinAnsiParser::new()));
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
    let parser = Arc::new(Mutex::new(StdinAnsiParser::new()));
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
    let parser = Arc::new(Mutex::new(StdinAnsiParser::new()));
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

    let (token, bytes) = captured.lock().unwrap().take().expect("timer must fire");
    assert_eq!(token, 22);
    assert!(
        bytes.windows(4).any(|w| w == b"]11;"),
        "buffered OSC 11 bytes must appear in the flushed payload: {:?}",
        bytes
    );
}

#[test]
fn host_theme_dsr_997_dark_in_one_chunk() {
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[?997;1n");
    assert!(residue.is_empty(), "DSR 997 reply must be fully consumed");
    assert_eq!(replies.len(), 1);
    match &replies[0] {
        HostReply::HostTerminalThemeChanged(mode) => {
            assert_eq!(*mode, zellij_utils::data::HostTerminalThemeMode::Dark);
        },
        other => panic!("expected HostTerminalThemeChanged, got {:?}", other),
    }
}

#[test]
fn host_theme_dsr_997_light_in_one_chunk() {
    let mut parser = StdinAnsiParser::new();
    let (replies, residue) = feed_once(&mut parser, b"\x1b[?997;2n");
    assert!(residue.is_empty());
    match &replies[0] {
        HostReply::HostTerminalThemeChanged(mode) => {
            assert_eq!(*mode, zellij_utils::data::HostTerminalThemeMode::Light);
        },
        other => panic!("expected HostTerminalThemeChanged, got {:?}", other),
    }
}

#[test]
fn host_theme_dsr_997_across_chunk_boundaries() {
    // Same coverage philosophy as fragmented_csi_report_does_not_leak:
    // split the DSR at every internal byte position and assert the
    // parser still emits exactly one HostReply with no leaked residue.
    let full = b"\x1b[?997;1n";
    for split in 1..full.len() {
        let mut p = StdinAnsiParser::new();
        let r1 = p.feed(&full[..split]);
        let r2 = p.feed(&full[split..]);
        assert!(
            r1.residue.is_empty(),
            "split at {}: chunk 1 leaked residue {:?}",
            split,
            r1.residue
        );
        assert!(
            r2.residue.is_empty(),
            "split at {}: chunk 2 leaked residue {:?}",
            split,
            r2.residue
        );
        let total = r1.replies.len() + r2.replies.len();
        assert_eq!(total, 1, "split at {}: got {} replies", split, total);
        let reply = r1.replies.into_iter().chain(r2.replies).next().unwrap();
        match reply {
            HostReply::HostTerminalThemeChanged(mode) => {
                assert_eq!(mode, zellij_utils::data::HostTerminalThemeMode::Dark);
            },
            other => panic!("expected HostTerminalThemeChanged, got {:?}", other),
        }
    }
}

#[test]
fn host_theme_dsr_997_unknown_param_dropped() {
    let mut parser = StdinAnsiParser::new();
    // CSI ?997;3n is not a defined Dark/Light value; the parser
    // recognises the CSI shape (final byte n) but
    // `from_csi_report` rejects unknown payloads, so no HostReply is
    // produced and no residue leaks (the bytes were consumed by termwiz).
    let out = parser.feed(b"\x1b[?997;3n");
    assert!(
        out.replies.is_empty(),
        "unknown ?997;N value must not classify"
    );
}
