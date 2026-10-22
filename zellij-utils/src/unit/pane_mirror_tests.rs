use super::*;
use crate::data::BareKey;
use crate::input::mouse::MouseEventType;
use crate::position::Position;
use crate::structured_render::WireColor;

fn screen_with_text(cols: u16, rows: u16, lines: &[&str]) -> MirrorScreen {
    let mut screen = MirrorScreen::blank(cols, rows);
    for (y, line) in lines.iter().enumerate() {
        for (x, character) in line.chars().enumerate() {
            let index = y * cols as usize + x;
            screen.cells[index] = MirrorCell::plain(WireCell {
                ch: character as u32,
                ..WireCell::BLANK
            });
        }
    }
    screen
}

fn text_of(screen: &MirrorScreen, y: u16) -> String {
    screen
        .row(y)
        .iter()
        .map(|cell| match &cell.text {
            Some(text) => text.clone(),
            None => char::from_u32(cell.cell.ch).unwrap_or(' ').to_string(),
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[test]
fn viewer_messages_survive_the_wire() {
    let messages = vec![
        ViewerMessage::Attach(MirrorAttach {
            version: PANE_MIRROR_VERSION,
            capabilities: capabilities(),
            target_pane_id: PaneId::Terminal(3),
            viewer_size: Size { cols: 80, rows: 24 },
            focused: true,
            resize_allowed: true,
            view_only: true,
        }),
        ViewerMessage::Input(MirrorInput::Key {
            key: KeyWithModifier::new(BareKey::Up),
            raw_bytes: b"\x1b[A".to_vec(),
            is_kitty_keyboard_protocol: false,
        }),
        ViewerMessage::Input(MirrorInput::Key {
            key: KeyWithModifier::new(BareKey::Char('x')).with_ctrl_modifier(),
            raw_bytes: vec![24],
            is_kitty_keyboard_protocol: true,
        }),
        ViewerMessage::Input(MirrorInput::Bytes(b"abc".to_vec())),
        ViewerMessage::Input(MirrorInput::Paste(b"pasted\ntext".to_vec())),
        ViewerMessage::Input(MirrorInput::Mouse(MouseEvent::new_left_press_event(
            Position::new(2, 5),
        ))),
        ViewerMessage::Resize {
            size: Size { cols: 10, rows: 4 },
            focused: false,
            resize_allowed: true,
        },
        ViewerMessage::Resize {
            size: Size { cols: 1, rows: 1 },
            focused: true,
            resize_allowed: false,
        },
        ViewerMessage::FrameAck { seq: 42 },
        ViewerMessage::Detach,
    ];
    for message in messages {
        assert_eq!(ViewerMessage::decode(&message.encode()), Ok(message));
    }
}

#[test]
fn source_messages_survive_the_wire() {
    let messages = vec![
        SourceMessage::Attached {
            version: PANE_MIRROR_VERSION,
            capabilities: capabilities(),
            pane_size: Size { cols: 80, rows: 24 },
        },
        SourceMessage::Frame {
            frame: vec![1, 2, 3, 4],
            alternate_screen: false,
        },
        SourceMessage::Frame {
            frame: vec![5],
            alternate_screen: true,
        },
        SourceMessage::PinState {
            pinned: true,
            pane_size: Size { cols: 40, rows: 10 },
        },
        SourceMessage::PinState {
            pinned: false,
            pane_size: Size { cols: 80, rows: 24 },
        },
        SourceMessage::closed(MirrorClosedReason::PaneClosed),
        SourceMessage::closed(MirrorClosedReason::SessionEnding),
        SourceMessage::closed(MirrorClosedReason::Rejected),
        SourceMessage::closed(MirrorClosedReason::VersionMismatch),
        SourceMessage::Closed {
            reason: MirrorClosedReason::PaneNotFound,
            detail: "terminal_9".to_owned(),
        },
    ];
    for message in messages {
        assert_eq!(SourceMessage::decode(&message.encode()), Ok(message));
    }
}

#[test]
fn an_empty_or_garbled_message_is_an_error() {
    assert_eq!(ViewerMessage::decode(&[]), Err(MirrorDecodeError::Empty));
    assert!(SourceMessage::decode(&[0xff, 0xff, 0xff]).is_err());
}

#[test]
fn the_first_frame_is_a_full_repaint_that_rebuilds_the_screen() {
    let mut source = screen_with_text(6, 2, &["hello", "world"]);
    source.cursor = CursorState {
        x: 5,
        y: 1,
        visible: true,
        ..CursorState::default()
    };
    let mut encoder = MirrorFrameEncoder::new();
    let frame = encoder.encode(&source).expect("a first frame");
    assert!(frame.full);
    let header = *structured_render::decode(&frame.bytes).unwrap().header();
    assert!(header.full_repaint());
    let viewer = MirrorScreen::from_frame(&frame.bytes).unwrap();
    assert_eq!(viewer, source);
}

#[test]
fn an_unchanged_screen_sends_nothing() {
    let source = screen_with_text(6, 2, &["hello"]);
    let mut encoder = MirrorFrameEncoder::new();
    encoder.encode(&source).unwrap();
    assert_eq!(encoder.encode(&source), None);
}

#[test]
fn a_change_sends_only_the_changed_span() {
    let source = screen_with_text(10, 3, &["first", "second", "third"]);
    let mut encoder = MirrorFrameEncoder::new();
    let first = encoder.encode(&source).unwrap();
    let mut viewer = MirrorScreen::from_frame(&first.bytes).unwrap();
    let changed = screen_with_text(10, 3, &["first", "secXnd", "third"]);
    let frame = encoder.encode(&changed).unwrap();
    assert!(!frame.full);
    let view = structured_render::decode(&frame.bytes).unwrap();
    assert!(!view.header().full_repaint());
    let records: Vec<_> = view.rows().map(|(record, _)| record).collect();
    assert_eq!(records.len(), 1);
    assert_eq!((records[0].y, records[0].x0, records[0].len), (1, 3, 1));
    let applied = viewer.apply_frame(&frame.bytes).unwrap();
    assert!(!applied.full);
    assert_eq!(
        applied.changed_rows.into_iter().collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(text_of(&viewer, 1), "secXnd");
    assert_eq!(viewer, changed);
}

#[test]
fn a_cursor_move_alone_sends_a_frame_without_rows() {
    let mut source = screen_with_text(4, 2, &["ab"]);
    let mut encoder = MirrorFrameEncoder::new();
    encoder.encode(&source).unwrap();
    source.cursor.x = 2;
    source.cursor.visible = true;
    let frame = encoder.encode(&source).unwrap();
    let view = structured_render::decode(&frame.bytes).unwrap();
    assert_eq!(view.rows().count(), 0);
    assert_eq!(view.cursor().x, 2);
}

#[test]
fn a_size_change_sends_a_full_repaint() {
    let mut encoder = MirrorFrameEncoder::new();
    encoder.encode(&screen_with_text(4, 2, &["ab"])).unwrap();
    let bigger = screen_with_text(8, 3, &["ab", "cd"]);
    let frame = encoder.encode(&bigger).unwrap();
    assert!(frame.full);
    let mut viewer = MirrorScreen::blank(4, 2);
    let applied = viewer.apply_frame(&frame.bytes).unwrap();
    assert!(applied.full);
    assert_eq!(viewer, bigger);
}

#[test]
fn colours_clusters_wide_characters_and_links_reach_the_viewer() {
    let mut source = MirrorScreen::blank(6, 1);
    source.cells[0] = MirrorCell {
        cell: WireCell {
            ch: 'R' as u32,
            fg: WireColor::Rgb(255, 0, 0).pack(),
            bg: WireColor::Indexed(17).pack(),
            ..WireCell::BLANK
        },
        text: None,
        link: Some("https://zellij.dev".to_owned()),
    };
    source.cells[1] = MirrorCell {
        cell: WireCell {
            ch: 0,
            width: 2,
            ..WireCell::BLANK
        },
        text: Some("👍🏽".to_owned()),
        link: None,
    };
    source.cells[2] = MirrorCell::plain(WireCell {
        ch: ' ' as u32,
        width: 1,
        ..WireCell::BLANK
    });
    source.cells[3] = MirrorCell::plain(WireCell {
        ch: '中' as u32,
        width: 2,
        ..WireCell::BLANK
    });
    source.wants_mouse = true;
    let mut encoder = MirrorFrameEncoder::new();
    let frame = encoder.encode(&source).unwrap();
    let viewer = MirrorScreen::from_frame(&frame.bytes).unwrap();
    assert_eq!(viewer, source);
    assert!(viewer.wants_mouse);
}

#[test]
fn turning_mouse_tracking_off_reaches_the_viewer() {
    let mut source = screen_with_text(3, 1, &["a"]);
    source.wants_mouse = true;
    let mut encoder = MirrorFrameEncoder::new();
    let mut viewer = MirrorScreen::from_frame(&encoder.encode(&source).unwrap().bytes).unwrap();
    assert!(viewer.wants_mouse);
    source.wants_mouse = false;
    let frame = encoder.encode(&source).unwrap();
    viewer.apply_frame(&frame.bytes).unwrap();
    assert!(!viewer.wants_mouse);
}

#[test]
fn frames_beyond_the_unacknowledged_limit_are_skipped_then_a_full_frame_follows() {
    let mut pacer = MirrorFramePacer::new(2);
    assert!(pacer.wants_frame(1));
    let first = pacer
        .next_frame(1, &screen_with_text(5, 1, &["1"]))
        .expect("first frame");
    assert!(pacer.wants_frame(2));
    let second = pacer
        .next_frame(2, &screen_with_text(5, 1, &["2"]))
        .expect("second frame");
    assert!(first.full);
    assert!(!second.full);
    assert_eq!(pacer.unacknowledged(), 2);
    assert!(!pacer.wants_frame(3));
    assert_eq!(pacer.next_frame(4, &screen_with_text(5, 1, &["4"])), None);
    assert!(pacer.has_skipped());
    assert!(pacer.acknowledge(first.seq));
    assert_eq!(pacer.unacknowledged(), 1);
    let latest = screen_with_text(5, 1, &["5"]);
    assert!(pacer.wants_frame(5));
    let catch_up = pacer.next_frame(5, &latest).expect("catch-up frame");
    assert!(catch_up.full);
    assert!(!pacer.has_skipped());
    assert_eq!(MirrorScreen::from_frame(&catch_up.bytes).unwrap(), latest);
}

#[test]
fn an_unchanged_pane_is_not_snapshotted_again() {
    let mut pacer = MirrorFramePacer::new(2);
    assert!(pacer.wants_frame(7));
    pacer
        .next_frame(7, &screen_with_text(2, 1, &["a"]))
        .unwrap();
    assert!(!pacer.wants_frame(7));
    assert!(!pacer.wants_frame(7));
    assert!(pacer.wants_frame(8));
}

#[test]
fn waiting_without_changes_does_not_mark_a_skip_or_force_a_full_frame() {
    let mut pacer = MirrorFramePacer::new(1);
    let first = pacer
        .next_frame(1, &screen_with_text(3, 1, &["a"]))
        .unwrap();
    for _ in 0..5 {
        assert!(!pacer.wants_frame(1));
    }
    assert!(!pacer.has_skipped());
    assert!(!pacer.acknowledge(first.seq));
    assert!(pacer.wants_frame(2));
    let next = pacer
        .next_frame(2, &screen_with_text(3, 1, &["ab"]))
        .unwrap();
    assert!(!next.full);
}

#[test]
fn a_change_while_waiting_is_caught_up_after_the_acknowledgement() {
    let mut pacer = MirrorFramePacer::new(1);
    let first = pacer
        .next_frame(1, &screen_with_text(3, 1, &["a"]))
        .unwrap();
    assert!(!pacer.wants_frame(2));
    assert!(pacer.has_skipped());
    assert!(pacer.acknowledge(first.seq));
    assert!(pacer.wants_frame(2));
}

#[test]
fn acknowledging_all_frames_without_skips_needs_no_catch_up() {
    let mut pacer = MirrorFramePacer::new(2);
    let frame = pacer
        .next_frame(1, &screen_with_text(2, 1, &["a"]))
        .unwrap();
    assert!(!pacer.acknowledge(frame.seq));
    assert_eq!(pacer.unacknowledged(), 0);
    let next = pacer
        .next_frame(2, &screen_with_text(2, 1, &["b"]))
        .unwrap();
    assert!(!next.full);
}

#[test]
fn a_requested_full_repaint_is_honoured() {
    let source = screen_with_text(2, 1, &["a"]);
    let mut pacer = MirrorFramePacer::default();
    pacer.next_frame(1, &source).unwrap();
    pacer.request_full_repaint();
    assert!(pacer.wants_frame(1));
    let frame = pacer
        .next_frame(1, &source)
        .expect("a repaint of an unchanged screen");
    assert!(frame.full);
}

#[test]
fn the_alternate_screen_flag_alone_sends_a_frame() {
    let mut source = screen_with_text(3, 1, &["a"]);
    let mut encoder = MirrorFrameEncoder::new();
    let first = encoder.encode(&source).unwrap();
    assert!(!first.alternate_screen);
    source.alternate_screen = true;
    let frame = encoder
        .encode(&source)
        .expect("a frame for the alternate screen flag");
    assert!(frame.alternate_screen);
    assert_eq!(encoder.encode(&source), None);
}

#[test]
fn viewer_sizes_are_clamped() {
    assert_eq!(
        clamp_mirror_size(Size { cols: 0, rows: 0 }),
        Size { cols: 1, rows: 1 }
    );
    assert_eq!(
        clamp_mirror_size(Size {
            cols: usize::MAX,
            rows: 100_000
        }),
        Size {
            cols: MAX_MIRROR_COLS,
            rows: MAX_MIRROR_ROWS
        }
    );
    assert_eq!(
        clamp_mirror_size(Size { cols: 80, rows: 24 }),
        Size { cols: 80, rows: 24 }
    );
}

#[test]
fn a_frame_larger_than_the_limit_is_refused() {
    let frame = FrameBuilder::new(MAX_MIRROR_COLS as u16 + 1, 2, 0).finish();
    assert_eq!(
        MirrorScreen::from_frame(&frame),
        Err(MirrorFrameError::TooLarge {
            cols: MAX_MIRROR_COLS as u16 + 1,
            rows: 2
        })
    );
    let frame = FrameBuilder::new(4, MAX_MIRROR_ROWS as u16 + 1, 0).finish();
    assert!(MirrorScreen::from_frame(&frame).is_err());
}

#[test]
fn the_pin_lease_is_three_renewal_intervals() {
    if std::env::var(MIRROR_PIN_LEASE_ENV_VAR).is_ok() {
        return;
    }
    assert_eq!(mirror_pin_lease(), MIRROR_PIN_RENEW_INTERVAL * 3);
    assert_eq!(mirror_pin_renew_interval(), MIRROR_PIN_RENEW_INTERVAL);
}

#[test]
fn mouse_events_keep_their_type() {
    let mut release = MouseEvent::new_left_press_event(Position::new(0, 0));
    release.event_type = MouseEventType::Release;
    let message = ViewerMessage::Input(MirrorInput::Mouse(release));
    assert_eq!(ViewerMessage::decode(&message.encode()), Ok(message));
}
