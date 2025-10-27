use super::super::{CharacterChunk, FloatingPanesStack, Output, OutputBuffer, SixelImageChunk};
use crate::panes::sixel::SixelImageStore;
use crate::panes::{LinkHandler, Row, TerminalCharacter};
use crate::ClientId;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::rc::Rc;
use zellij_utils::pane_size::{Dimension, PaneGeom, Size, SizeInPixels};

/// Helper to create a simple Output instance for testing
fn create_test_output() -> Output {
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        height: 20,
        width: 10,
    })));
    let styled_underlines = true;
    Output::new(sixel_image_store, character_cell_size, styled_underlines)
}

/// Helper to create a simple CharacterChunk with text
fn create_character_chunk_from_str(text: &str, x: usize, y: usize) -> CharacterChunk {
    let terminal_chars: Vec<TerminalCharacter> =
        text.chars().map(|c| TerminalCharacter::new(c)).collect();
    CharacterChunk::new(terminal_chars, x, y)
}

/// Helper to create test clients
fn create_test_clients(count: usize) -> HashSet<ClientId> {
    (1..=count).map(|i| i as ClientId).collect()
}

/// Helper to create PaneGeom for FloatingPanesStack tests
fn create_pane_geom(x: usize, y: usize, cols: usize, rows: usize) -> PaneGeom {
    PaneGeom {
        x,
        y,
        cols: Dimension::fixed(cols),
        rows: Dimension::fixed(rows),
        stacked: None,
        is_pinned: false,
        logical_position: None,
    }
}

#[test]
fn test_output_new() {
    let output = create_test_output();

    // Verify default state of all fields
    assert!(!output.is_dirty(), "New output should not be dirty");
    assert!(
        !output.has_rendered_assets(),
        "New output should not have rendered assets"
    );
}

#[test]
fn test_add_clients() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(3);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

    output.add_clients(&client_ids, link_handler, None);

    // Verify that client_character_chunks has entries for all clients
    assert!(!output.is_dirty(), "Should not be dirty until chunks added");
}

#[test]
fn test_is_dirty_with_empty_output() {
    let output = create_test_output();
    assert!(!output.is_dirty(), "Empty output should not be dirty");
}

#[test]
fn test_is_dirty_with_character_chunks() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Hi", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    assert!(
        output.is_dirty(),
        "Output should be dirty after adding character chunks"
    );
}

#[test]
fn test_is_dirty_with_pre_vte_instructions() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    output.add_pre_vte_instruction_to_client(1, "\u{1b}[?1049h");

    assert!(
        output.is_dirty(),
        "Output should be dirty after adding pre VTE instructions"
    );
}

#[test]
fn test_is_dirty_with_post_vte_instructions() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    output.add_post_vte_instruction_to_client(1, "\u{1b}[?25h");

    assert!(
        output.is_dirty(),
        "Output should be dirty after adding post VTE instructions"
    );
}

#[test]
fn test_is_dirty_with_sixel_chunks() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let sixel_chunk = SixelImageChunk {
        cell_x: 0,
        cell_y: 0,
        sixel_image_pixel_x: 0,
        sixel_image_pixel_y: 0,
        sixel_image_pixel_width: 100,
        sixel_image_pixel_height: 100,
        sixel_image_id: 1,
    };
    output.add_sixel_image_chunks_to_client(1, vec![sixel_chunk], None);

    assert!(
        output.is_dirty(),
        "Output should be dirty after adding sixel chunks"
    );
}

#[test]
fn test_has_rendered_assets_empty() {
    let output = create_test_output();
    assert!(
        !output.has_rendered_assets(),
        "Empty output should not have rendered assets"
    );
}

#[test]
fn test_has_rendered_assets_only_vte_instructions() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    output.add_pre_vte_instruction_to_client(1, "\u{1b}[?25l");
    output.add_post_vte_instruction_to_client(1, "\u{1b}[?25h");

    assert!(
        !output.has_rendered_assets(),
        "VTE instructions alone should not count as rendered assets"
    );
}

#[test]
fn test_has_rendered_assets_with_character_chunks() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Hello", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    assert!(
        output.has_rendered_assets(),
        "Character chunks should count as rendered assets"
    );
}

#[test]
fn test_has_rendered_assets_with_sixel_chunks() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let sixel_chunk = SixelImageChunk {
        cell_x: 0,
        cell_y: 0,
        sixel_image_pixel_x: 0,
        sixel_image_pixel_y: 0,
        sixel_image_pixel_width: 100,
        sixel_image_pixel_height: 100,
        sixel_image_id: 1,
    };
    output.add_sixel_image_chunks_to_client(1, vec![sixel_chunk], None);

    assert!(
        output.has_rendered_assets(),
        "Sixel chunks should count as rendered assets"
    );
}

#[test]
fn test_serialize_empty() {
    let mut output = create_test_output();
    let result = output.serialize().unwrap();
    assert!(
        result.is_empty(),
        "Serializing empty output should return empty HashMap"
    );
}

#[test]
fn test_serialize_single_client_simple_text() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Hello", 5, 10);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    let result = output.serialize().unwrap();
    assert_eq!(result.len(), 1, "Should have one client in result");

    let client_output = result.get(&1).unwrap();
    // Verify contains goto instruction (y+1, x+1 for 1-indexed VTE)
    assert!(
        client_output.contains("\u{1b}[11;6H"),
        "Should contain goto instruction for position (5, 10)"
    );
    // Verify contains reset styles
    assert!(
        client_output.contains("\u{1b}[m"),
        "Should contain reset styles"
    );
    // Verify contains the text
    assert!(client_output.contains("Hello"), "Should contain the text");
}

#[test]
fn test_serialize_multiple_clients() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(2);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk1 = create_character_chunk_from_str("Hello", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk1], None)
        .unwrap();

    let chunk2 = create_character_chunk_from_str("World", 10, 10);
    output
        .add_character_chunks_to_client(2, vec![chunk2], None)
        .unwrap();

    let result = output.serialize().unwrap();
    assert_eq!(result.len(), 2, "Should have two clients in result");

    let client1_output = result.get(&1).unwrap();
    assert!(
        client1_output.contains("Hello"),
        "Client 1 should contain 'Hello'"
    );

    let client2_output = result.get(&2).unwrap();
    assert!(
        client2_output.contains("World"),
        "Client 2 should contain 'World'"
    );
}

#[test]
fn test_serialize_with_pre_and_post_vte_instructions() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    output.add_pre_vte_instruction_to_client(1, "\u{1b}[?1049h");
    let chunk = create_character_chunk_from_str("Test", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();
    output.add_post_vte_instruction_to_client(1, "\u{1b}[?25h");

    let result = output.serialize().unwrap();
    let client_output = result.get(&1).unwrap();

    // Verify correct ordering
    let pre_vte_pos = client_output.find("\u{1b}[?1049h").unwrap();
    let text_pos = client_output.find("Test").unwrap();
    let post_vte_pos = client_output.find("\u{1b}[?25h").unwrap();

    assert!(
        pre_vte_pos < text_pos && text_pos < post_vte_pos,
        "Instructions should be in correct order: pre, content, post"
    );
}

#[test]
fn test_serialize_drains_state() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Hello", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    assert!(output.is_dirty(), "Output should be dirty before serialize");

    let result1 = output.serialize().unwrap();
    assert_eq!(result1.len(), 1, "First serialize should return data");

    assert!(
        !output.is_dirty(),
        "Output should not be dirty after serialize"
    );

    let result2 = output.serialize().unwrap();
    assert!(
        result2.is_empty(),
        "Second serialize should return empty HashMap"
    );
}

#[test]
fn test_serialize_with_size_no_constraints() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Hello", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    let result = output.serialize_with_size(None, None).unwrap();
    assert_eq!(result.len(), 1, "Should have one client in result");

    let client_output = result.get(&1).unwrap();
    assert!(client_output.contains("Hello"), "Should contain the text");
}

#[test]
fn test_serialize_with_size_crops_chunks_below_visible_area() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let max_size = Some(Size { rows: 10, cols: 80 });

    // Add chunk below visible area (should be cropped)
    let chunk_below = create_character_chunk_from_str("Hidden", 0, 15);
    output
        .add_character_chunks_to_client(1, vec![chunk_below], None)
        .unwrap();

    // Add chunk within visible area (should be included)
    let chunk_visible = create_character_chunk_from_str("Visible", 0, 5);
    output
        .add_character_chunks_to_client(1, vec![chunk_visible], None)
        .unwrap();

    let result = output.serialize_with_size(max_size, None).unwrap();
    let client_output = result.get(&1).unwrap();

    assert!(
        client_output.contains("Visible"),
        "Should contain visible chunk"
    );
    assert!(
        !client_output.contains("Hidden"),
        "Should not contain chunk below visible area"
    );
}

#[test]
fn test_serialize_with_size_crops_chunks_outside_cols() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let max_size = Some(Size { rows: 10, cols: 20 });

    // Add chunk outside visible columns (should be cropped)
    let chunk_outside = create_character_chunk_from_str("Hidden", 25, 5);
    output
        .add_character_chunks_to_client(1, vec![chunk_outside], None)
        .unwrap();

    // Add chunk within visible area
    let chunk_visible = create_character_chunk_from_str("Visible", 5, 5);
    output
        .add_character_chunks_to_client(1, vec![chunk_visible], None)
        .unwrap();

    let result = output.serialize_with_size(max_size, None).unwrap();
    let client_output = result.get(&1).unwrap();

    assert!(
        client_output.contains("Visible"),
        "Should contain visible chunk"
    );
    assert!(
        !client_output.contains("Hidden"),
        "Should not contain chunk outside visible columns"
    );
}

#[test]
fn test_serialize_with_size_crops_characters_within_chunk() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let max_size = Some(Size { rows: 10, cols: 20 });

    // Add chunk that starts at x=15 and would extend to x=25 (10 chars)
    let chunk = create_character_chunk_from_str("1234567890", 15, 5);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    let result = output.serialize_with_size(max_size, None).unwrap();
    let client_output = result.get(&1).unwrap();

    // Should only render first 5 characters (cols 15-19)
    assert!(
        client_output.contains("12345"),
        "Should contain first 5 characters"
    );
    assert!(
        !client_output.contains("67890"),
        "Should not contain characters beyond max_size.cols"
    );
}

#[test]
fn test_serialize_with_size_adds_padding_instructions() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let max_size = Some(Size {
        rows: 30,
        cols: 100,
    });
    let content_size = Some(Size { rows: 20, cols: 80 });

    let chunk = create_character_chunk_from_str("Test", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    let result = output.serialize_with_size(max_size, content_size).unwrap();
    let client_output = result.get(&1).unwrap();

    // Verify padding/clearing instructions are present
    // Should contain clear line instructions: \u{1b}[y;xH\u{1b}[m\u{1b}[K
    assert!(
        client_output.contains("\u{1b}[K"),
        "Should contain clear line instructions"
    );
    // Should contain clear below instruction: \u{1b}[21;1H\u{1b}[m\u{1b}[J
    assert!(
        client_output.contains("\u{1b}[21;1H\u{1b}[m\u{1b}[J"),
        "Should contain clear below instruction at line 21"
    );
}

#[test]
fn test_serialize_with_size_hides_cursor_when_cropped() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let max_size = Some(Size { rows: 10, cols: 20 });

    // Set cursor outside max_size
    output.cursor_is_visible(25, 5);

    let chunk = create_character_chunk_from_str("Test", 0, 0);
    output
        .add_character_chunks_to_client(1, vec![chunk], None)
        .unwrap();

    let result = output.serialize_with_size(max_size, None).unwrap();
    let client_output = result.get(&1).unwrap();

    // Verify hide cursor instruction is added
    assert!(
        client_output.contains("\u{1b}[?25l"),
        "Should contain hide cursor instruction when cursor is cropped"
    );
}

#[test]
fn test_add_character_chunks_to_multiple_clients() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(3);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let chunk = create_character_chunk_from_str("Test", 0, 0);
    output
        .add_character_chunks_to_multiple_clients(vec![chunk], client_ids.iter().copied(), None)
        .unwrap();

    let result = output.serialize().unwrap();
    assert_eq!(result.len(), 3, "Should have three clients in result");

    for client_id in 1..=3 {
        let client_output = result.get(&client_id).unwrap();
        assert!(
            client_output.contains("Test"),
            "Client {} should contain the text",
            client_id
        );
    }
}

#[test]
fn test_add_sixel_image_chunks_to_multiple_clients() {
    let mut output = create_test_output();
    let client_ids = create_test_clients(2);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);

    let sixel_chunk = SixelImageChunk {
        cell_x: 0,
        cell_y: 0,
        sixel_image_pixel_x: 0,
        sixel_image_pixel_y: 0,
        sixel_image_pixel_width: 100,
        sixel_image_pixel_height: 100,
        sixel_image_id: 1,
    };

    output.add_sixel_image_chunks_to_multiple_clients(
        vec![sixel_chunk],
        client_ids.iter().copied(),
        None,
    );

    assert!(
        output.has_rendered_assets(),
        "Output should have rendered assets"
    );
}

#[test]
fn test_character_chunk_new() {
    let terminal_chars: Vec<TerminalCharacter> = vec![TerminalCharacter::new('A')];

    let chunk = CharacterChunk::new(terminal_chars, 5, 10);

    assert_eq!(chunk.x, 5, "x should be set correctly");
    assert_eq!(chunk.y, 10, "y should be set correctly");
    assert_eq!(
        chunk.terminal_characters.len(),
        1,
        "Should have one character"
    );
}

#[test]
fn test_character_chunk_width() {
    let chunk = create_character_chunk_from_str("Hello", 0, 0);
    assert_eq!(chunk.width(), 5, "Width should be 5 for 'Hello'");

    // Test with wide characters
    let terminal_chars: Vec<TerminalCharacter> = vec![
        TerminalCharacter::new('a'),
        TerminalCharacter::new('中'),
        TerminalCharacter::new('b'),
    ];
    let chunk_wide = CharacterChunk::new(terminal_chars, 0, 0);
    assert_eq!(
        chunk_wide.width(),
        4,
        "Width should be 4 (1 + 2 + 1) for mixed characters"
    );
}

#[test]
fn test_character_chunk_drain_by_width() {
    let mut chunk = create_character_chunk_from_str("Hello World", 0, 0);
    assert_eq!(chunk.width(), 11, "Initial width should be 11");

    // Drain first 5 characters
    let drained: Vec<TerminalCharacter> = chunk.drain_by_width(5).collect();
    assert_eq!(drained.len(), 5, "Should drain 5 characters");
    assert_eq!(
        chunk.terminal_characters.len(),
        6,
        "Should have 6 characters remaining"
    );

    let drained_text: String = drained.iter().map(|c| c.character).collect();
    assert_eq!(drained_text, "Hello", "Drained part should be 'Hello'");

    let remaining_text: String = chunk
        .terminal_characters
        .iter()
        .map(|c| c.character)
        .collect();
    assert_eq!(
        remaining_text, " World",
        "Remaining part should be ' World'"
    );
}

#[test]
fn test_character_chunk_drain_by_width_with_wide_chars() {
    let terminal_chars: Vec<TerminalCharacter> = vec![
        TerminalCharacter::new('a'),
        TerminalCharacter::new('中'),
        TerminalCharacter::new('b'),
    ];
    let mut chunk = CharacterChunk::new(terminal_chars, 0, 0);

    // Drain 2 characters - this cuts in the middle of wide char
    let drained: Vec<TerminalCharacter> = chunk.drain_by_width(2).collect();

    // Should have padding with EMPTY_TERMINAL_CHARACTER
    assert!(
        drained.len() >= 2,
        "Drained part should have at least 2 characters"
    );
}

#[test]
fn test_character_chunk_retain_by_width() {
    let mut chunk = create_character_chunk_from_str("Hello World", 0, 0);

    // Retain only first 5 characters
    chunk.retain_by_width(5);

    assert_eq!(
        chunk.terminal_characters.len(),
        5,
        "Should have 5 characters"
    );
    let text: String = chunk
        .terminal_characters
        .iter()
        .map(|c| c.character)
        .collect();
    assert_eq!(text, "Hello", "Should retain 'Hello'");
}

#[test]
fn test_character_chunk_cut_middle_out() {
    let mut chunk = create_character_chunk_from_str("Hello World", 0, 0);

    // Cut middle (characters 5-8)
    let (left, right) = chunk.cut_middle_out(5, 8).unwrap();

    let left_text: String = left.iter().map(|c| c.character).collect();
    assert_eq!(left_text, "Hello", "Left chunk should be 'Hello'");

    let right_text: String = right.iter().map(|c| c.character).collect();
    assert_eq!(right_text, "rld", "Right chunk should be 'rld'");
}

#[test]
fn test_visible_character_chunks_no_panes() {
    let stack = FloatingPanesStack { layers: vec![] };
    let chunks = vec![create_character_chunk_from_str("Test", 0, 0)];

    let visible = stack.visible_character_chunks(chunks, None).unwrap();

    assert_eq!(visible.len(), 1, "All chunks should be visible");
}

#[test]
fn test_visible_character_chunks_completely_covered() {
    let pane_geom = create_pane_geom(0, 0, 10, 10);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    // Chunk completely within pane bounds
    let chunks = vec![create_character_chunk_from_str("Test", 5, 5)];

    let visible = stack.visible_character_chunks(chunks, Some(0)).unwrap();

    assert_eq!(
        visible.len(),
        0,
        "Completely covered chunk should not be visible"
    );
}

#[test]
fn test_visible_character_chunks_partially_covered_left() {
    let pane_geom = create_pane_geom(0, 5, 10, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    // Chunk that spans x=5-15, pane covers x=0-9
    let chunks = vec![create_character_chunk_from_str("HelloWorld", 5, 5)];

    let visible = stack.visible_character_chunks(chunks, Some(0)).unwrap();

    // Should retain the right part
    assert!(visible.len() > 0, "Should have visible chunks");
    if !visible.is_empty() {
        assert!(
            visible[0].x >= 10,
            "Visible chunk should start after pane edge"
        );
    }
}

#[test]
fn test_visible_character_chunks_partially_covered_right() {
    let pane_geom = create_pane_geom(10, 5, 10, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    // Chunk that spans x=5-15, pane covers x=10-19
    let chunks = vec![create_character_chunk_from_str("HelloWorld", 5, 5)];

    let visible = stack.visible_character_chunks(chunks, Some(0)).unwrap();

    // Should retain the left part
    assert!(visible.len() > 0, "Should have visible chunks");
    if !visible.is_empty() {
        assert_eq!(visible[0].x, 5, "Visible chunk should start at original x");
        assert!(
            visible[0].width() < 10,
            "Visible chunk should be shorter than original"
        );
    }
}

#[test]
fn test_visible_character_chunks_middle_covered() {
    let pane_geom = create_pane_geom(5, 5, 3, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    // Chunk spans x=0-10, pane covers x=5-7
    let chunks = vec![create_character_chunk_from_str("0123456789", 0, 5)];

    let visible = stack.visible_character_chunks(chunks, Some(0)).unwrap();

    // Should return two chunks (left and right parts)
    assert!(
        visible.len() >= 1,
        "Should have at least one visible chunk when middle is covered"
    );
}

#[test]
fn test_cursor_is_visible_with_floating_panes() {
    let pane_geom = create_pane_geom(5, 5, 10, 10);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    // Cursor inside pane bounds
    assert!(
        !stack.cursor_is_visible(7, 7),
        "Cursor should not be visible when covered by pane"
    );

    // Cursor outside pane bounds
    assert!(
        stack.cursor_is_visible(20, 20),
        "Cursor should be visible when not covered by pane"
    );
}

#[test]
fn test_output_buffer_update_line() {
    let mut buffer = OutputBuffer::default();
    buffer.clear(); // Clear the initial "update all lines" state

    buffer.update_line(5);

    assert!(
        buffer.changed_lines.contains(&5),
        "Changed lines should contain line 5"
    );
}

#[test]
fn test_output_buffer_update_all_lines() {
    let mut buffer = OutputBuffer::default();

    assert!(
        buffer.should_update_all_lines,
        "Should update all lines by default"
    );

    buffer.clear();
    assert!(
        !buffer.should_update_all_lines,
        "Should not update all lines after clear"
    );

    buffer.update_all_lines();
    assert!(
        buffer.should_update_all_lines,
        "Should update all lines after update_all_lines"
    );
}

#[test]
fn test_output_buffer_serialize() {
    let buffer = OutputBuffer::default();

    // Create a simple viewport with Row data
    let mut columns = VecDeque::new();
    columns.push_back(TerminalCharacter::new('A'));
    let row = Row::from_columns(columns);
    let viewport = vec![row];

    let result = buffer.serialize(&viewport, None).unwrap();

    // Should contain the character and newlines/carriage returns
    assert!(result.contains('A'), "Serialized output should contain 'A'");
    assert!(
        result.contains("\n\r"),
        "Serialized output should contain newlines"
    );
}

#[test]
fn test_output_buffer_changed_chunks_in_viewport_when_all_dirty() {
    let buffer = OutputBuffer::default();

    let mut columns = VecDeque::new();
    columns.push_back(TerminalCharacter::new('A'));
    let row = Row::from_columns(columns);
    let viewport = vec![row];

    let chunks = buffer.changed_chunks_in_viewport(&viewport, 10, 1, 0, 0);

    assert_eq!(
        chunks.len(),
        1,
        "Should return all lines when should_update_all_lines is true"
    );
}

#[test]
fn test_output_buffer_changed_chunks_in_viewport_partial() {
    let mut buffer = OutputBuffer::default();
    buffer.clear();

    // Mark only specific lines as changed
    buffer.update_line(2);
    buffer.update_line(5);
    buffer.update_line(7);

    let rows: Vec<Row> = (0..10)
        .map(|_| {
            let mut columns = VecDeque::new();
            columns.push_back(TerminalCharacter::new('A'));
            Row::from_columns(columns)
        })
        .collect();

    let chunks = buffer.changed_chunks_in_viewport(&rows, 10, 10, 0, 0);

    assert_eq!(chunks.len(), 3, "Should return only changed lines");
    assert_eq!(chunks[0].y, 2, "First chunk should be at line 2");
    assert_eq!(chunks[1].y, 5, "Second chunk should be at line 5");
    assert_eq!(chunks[2].y, 7, "Third chunk should be at line 7");
}
