use super::super::{
    CharacterChunk, FloatingPanesStack, HostKittyState, KittyImageChunk, Output, OutputBuffer,
    SixelImageChunk,
};
use crate::panes::kitty_graphics::parser::{
    DecodedImage, KittyAction, KittyCommandParser, KittyFormat,
};
use crate::panes::kitty_graphics::store::{InternalImageId, KittyImageStore};
use crate::panes::sixel::SixelImageStore;
use crate::panes::terminal_character::AnsiCode;
use crate::panes::{LinkHandler, PaneId, Row, TerminalCharacter};
use crate::ClientId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
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
    let osc8_hyperlinks = true;
    Output::new(
        sixel_image_store,
        character_cell_size,
        styled_underlines,
        osc8_hyperlinks,
        Rc::new(RefCell::new(KittyImageStore::default())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(HashMap::new())),
    )
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
    output.cursor_is_visible(25, 5, None);

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
        !stack.cursor_is_visible(7, 7, None),
        "Cursor should not be visible when covered by pane"
    );

    // Cursor outside pane bounds
    assert!(
        stack.cursor_is_visible(20, 20, None),
        "Cursor should be visible when not covered by pane"
    );
}

#[test]
fn test_cursor_visibility_with_z_index_bottom_layer() {
    // Two overlapping panes: bottom at (5,5) and top at (5,5)
    let bottom_pane = create_pane_geom(5, 5, 10, 10);
    let top_pane = create_pane_geom(5, 5, 10, 10);

    let stack = FloatingPanesStack {
        layers: vec![bottom_pane, top_pane],
    };

    // Cursor at position (7,7) in the bottom layer (z-index 0)
    // Should be hidden because there's a pane above it (z-index 1) at the same position
    assert!(
        !stack.cursor_is_visible(7, 7, Some(0)),
        "Cursor in bottom layer should be hidden by pane above it"
    );
}

#[test]
fn test_cursor_visibility_with_z_index_top_layer() {
    // Two overlapping panes: bottom at (5,5) and top at (5,5)
    let bottom_pane = create_pane_geom(5, 5, 10, 10);
    let top_pane = create_pane_geom(5, 5, 10, 10);

    let stack = FloatingPanesStack {
        layers: vec![bottom_pane, top_pane],
    };

    // Cursor at position (7,7) in the top layer (z-index 1)
    // Should be visible even though there's a pane below it at the same position
    assert!(
        stack.cursor_is_visible(7, 7, Some(1)),
        "Cursor in top layer should be visible (not affected by panes below)"
    );
}

#[test]
fn test_cursor_visibility_with_multiple_layers() {
    // Three panes stacked vertically with different layers
    // Layer 0 (bottom): pane at (5,5)
    // Layer 1 (middle): pane at (10,10)
    // Layer 2 (top): pane at (5,5) - overlaps with layer 0
    let layer0_pane = create_pane_geom(5, 5, 10, 10);
    let layer1_pane = create_pane_geom(10, 10, 10, 10);
    let layer2_pane = create_pane_geom(5, 5, 10, 10);

    let stack = FloatingPanesStack {
        layers: vec![layer0_pane, layer1_pane, layer2_pane],
    };

    // Cursor at (7,7) in layer 0 - should be hidden by layer 2
    assert!(
        !stack.cursor_is_visible(7, 7, Some(0)),
        "Cursor in layer 0 should be hidden by layer 2 above it"
    );

    // Cursor at (16,16) in layer 1 - should be visible (layer 2 doesn't cover this position)
    // Layer 1 is at (10,10) size 10x10, so covers (10,10) to (19,19)
    // Layer 2 is at (5,5) size 10x10, so covers (5,5) to (14,14)
    // Position (16,16) is inside layer 1 but outside layer 2
    assert!(
        stack.cursor_is_visible(16, 16, Some(1)),
        "Cursor in layer 1 should be visible when not covered by layers above"
    );

    // Cursor at (7,7) in layer 2 - should be visible (no layers above)
    assert!(
        stack.cursor_is_visible(7, 7, Some(2)),
        "Cursor in top layer should always be visible"
    );
}

#[test]
fn test_cursor_visibility_pinned_pane_over_floating() {
    // Simulates a floating pane (z-index 0) with a pinned pane (z-index 1) on top
    // Both panes overlap at the same position
    let floating_pane = create_pane_geom(10, 10, 20, 20);
    let mut pinned_pane = create_pane_geom(10, 10, 20, 20);
    pinned_pane.is_pinned = true;

    let stack = FloatingPanesStack {
        layers: vec![floating_pane, pinned_pane],
    };

    // Cursor in the floating pane (z-index 0) at position covered by pinned pane
    assert!(
        !stack.cursor_is_visible(15, 15, Some(0)),
        "Cursor in floating pane should be hidden when covered by pinned pane above"
    );

    // Cursor in the pinned pane (z-index 1) at the same position
    assert!(
        stack.cursor_is_visible(15, 15, Some(1)),
        "Cursor in pinned pane should be visible"
    );
}

#[test]
fn test_cursor_visibility_partial_overlap() {
    // Two panes with partial overlap
    // Layer 0: pane at (0, 0) size 10x10
    // Layer 1: pane at (5, 5) size 10x10 (overlaps bottom-right of layer 0)
    let layer0_pane = create_pane_geom(0, 0, 10, 10);
    let layer1_pane = create_pane_geom(5, 5, 10, 10);

    let stack = FloatingPanesStack {
        layers: vec![layer0_pane, layer1_pane],
    };

    // Cursor at (2,2) in layer 0 - not covered by layer 1, should be visible
    assert!(
        stack.cursor_is_visible(2, 2, Some(0)),
        "Cursor in non-overlapping area should be visible"
    );

    // Cursor at (7,7) in layer 0 - covered by layer 1, should be hidden
    assert!(
        !stack.cursor_is_visible(7, 7, Some(0)),
        "Cursor in overlapping area should be hidden by pane above"
    );

    // Cursor at (7,7) in layer 1 - should be visible (top layer)
    assert!(
        stack.cursor_is_visible(7, 7, Some(1)),
        "Cursor in top layer should be visible in overlapping area"
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

    let result = buffer.serialize(&viewport, true, None).unwrap();

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

#[test]
fn test_pane_defaults_preserved_when_middle_covered() {
    let pane_geom = create_pane_geom(5, 5, 3, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    let pane_bg = Some(AnsiCode::RgbCode((0, 26, 58)));
    let pane_fg = Some(AnsiCode::RgbCode((0, 224, 0)));

    let mut chunk = create_character_chunk_from_str("0123456789", 0, 5);
    chunk.pane_default_bg = pane_bg;
    chunk.pane_default_fg = pane_fg;

    let visible = stack
        .visible_character_chunks(vec![chunk], Some(0))
        .unwrap();

    assert_eq!(visible.len(), 2, "Middle split should produce two chunks");
    for (i, chunk) in visible.iter().enumerate() {
        assert_eq!(
            chunk.pane_default_bg, pane_bg,
            "Chunk {i} should preserve pane_default_bg"
        );
        assert_eq!(
            chunk.pane_default_fg, pane_fg,
            "Chunk {i} should preserve pane_default_fg"
        );
    }
}

#[test]
fn test_pane_defaults_preserved_when_left_covered() {
    let pane_geom = create_pane_geom(0, 5, 5, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    let pane_bg = Some(AnsiCode::RgbCode((0, 26, 58)));

    let mut chunk = create_character_chunk_from_str("0123456789", 0, 5);
    chunk.pane_default_bg = pane_bg;

    let visible = stack
        .visible_character_chunks(vec![chunk], Some(0))
        .unwrap();

    assert_eq!(visible.len(), 1, "Left-covered should produce one chunk");
    assert_eq!(
        visible[0].pane_default_bg, pane_bg,
        "Remaining chunk should preserve pane_default_bg"
    );
}

#[test]
fn test_pane_defaults_preserved_when_right_covered() {
    let pane_geom = create_pane_geom(5, 5, 10, 1);
    let stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };

    let pane_bg = Some(AnsiCode::RgbCode((0, 26, 58)));

    let mut chunk = create_character_chunk_from_str("0123456789", 0, 5);
    chunk.pane_default_bg = pane_bg;

    let visible = stack
        .visible_character_chunks(vec![chunk], Some(0))
        .unwrap();

    assert_eq!(visible.len(), 1, "Right-covered should produce one chunk");
    assert_eq!(
        visible[0].pane_default_bg, pane_bg,
        "Remaining chunk should preserve pane_default_bg"
    );
}

type KittyTestParts = (
    Rc<RefCell<KittyImageStore>>,
    Rc<RefCell<HashMap<ClientId, bool>>>,
    Rc<RefCell<HashMap<ClientId, HostKittyState>>>,
);

fn create_test_kitty_parts() -> KittyTestParts {
    let kitty_image_store = Rc::new(RefCell::new(KittyImageStore::default()));
    let capabilities = Rc::new(RefCell::new(HashMap::new()));
    capabilities.borrow_mut().insert(1, true);
    let host_state = Rc::new(RefCell::new(HashMap::new()));
    (kitty_image_store, capabilities, host_state)
}

fn create_test_kitty_output(parts: &KittyTestParts) -> Output {
    let sixel_image_store = Rc::new(RefCell::new(SixelImageStore::default()));
    let character_cell_size = Rc::new(RefCell::new(Some(SizeInPixels {
        height: 20,
        width: 10,
    })));
    Output::new(
        sixel_image_store,
        character_cell_size,
        true,
        true,
        parts.0.clone(),
        parts.1.clone(),
        parts.2.clone(),
        Rc::new(RefCell::new(HashMap::new())),
    )
}

fn store_test_kitty_image(
    kitty_image_store: &Rc<RefCell<KittyImageStore>>,
    width: u32,
    height: u32,
) -> InternalImageId {
    kitty_image_store
        .borrow_mut()
        .store_image(DecodedImage {
            bytes: vec![255u8; (width * height * 4) as usize],
            width,
            height,
            format: KittyFormat::Rgba32,
        })
        .unwrap()
}

fn kitty_chunk(
    internal_image_id: InternalImageId,
    placement_uid: u64,
    cell_x: usize,
    cell_y: usize,
) -> KittyImageChunk {
    KittyImageChunk {
        cell_x,
        cell_y,
        internal_image_id,
        source_px_x: 0,
        source_px_y: 0,
        source_px_width: 30,
        source_px_height: 40,
        cell_offset_x: 0,
        cell_offset_y: 0,
        z_index: 0,
        dest_cells: (3, 2),
        scaled_px: None,
        placement_uid,
    }
}

fn run_kitty_frame(
    parts: &KittyTestParts,
    chunks: Vec<KittyImageChunk>,
    floating_panes_stack: Option<FloatingPanesStack>,
) -> String {
    let mut output = create_test_kitty_output(parts);
    let client_ids: HashSet<ClientId> = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, floating_panes_stack);
    output.add_kitty_image_chunks_to_client(1, PaneId::Terminal(1), chunks, None);
    output.serialize().unwrap().remove(&1).unwrap_or_default()
}

fn run_kitty_frame_for_panes(
    parts: &KittyTestParts,
    chunks_by_pane: Vec<(PaneId, Vec<KittyImageChunk>)>,
    visible_panes: HashSet<PaneId>,
) -> String {
    let mut output = create_test_kitty_output(parts);
    let client_ids: HashSet<ClientId> = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);
    output.set_kitty_visible_panes(1, visible_panes);
    for (pane_id, chunks) in chunks_by_pane {
        output.add_kitty_image_chunks_to_client(1, pane_id, chunks, None);
    }
    output.serialize().unwrap().remove(&1).unwrap_or_default()
}

fn run_kitty_frame_with_host_clear(parts: &KittyTestParts, chunks: Vec<KittyImageChunk>) -> String {
    let mut output = create_test_kitty_output(parts);
    let client_ids: HashSet<ClientId> = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);
    output.add_pre_vte_instruction_to_client(1, "\u{1b}[m\u{1b}[2J");
    output.mark_host_display_cleared_for_clients(std::iter::once(1));
    output.add_kitty_image_chunks_to_client(1, PaneId::Terminal(1), chunks, None);
    output.serialize().unwrap().remove(&1).unwrap_or_default()
}

fn run_kitty_frame_with_unmarked_clear_string(
    parts: &KittyTestParts,
    chunks: Vec<KittyImageChunk>,
) -> String {
    let mut output = create_test_kitty_output(parts);
    let client_ids: HashSet<ClientId> = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);
    output.add_pre_vte_instruction_to_client(1, "\u{1b}[m\u{1b}[2J");
    output.add_kitty_image_chunks_to_client(1, PaneId::Terminal(1), chunks, None);
    output.serialize().unwrap().remove(&1).unwrap_or_default()
}

fn parse_kitty_placement_crops(output: &str) -> Vec<(usize, usize, usize, usize, usize, usize)> {
    let marker = "\u{1b}_Ga=p,q=2,";
    let mut crops = vec![];
    let mut search_start = 0;
    while let Some(position) = output[search_start..].find(marker) {
        let absolute_position = search_start + position;
        let after = &output[absolute_position + marker.len()..];
        let end = after.find("\u{1b}\\").unwrap();
        let mut params: HashMap<&str, &str> = HashMap::new();
        for key_value in after[..end].split(',') {
            let mut parts = key_value.splitn(2, '=');
            let key = parts.next().unwrap();
            let value = parts.next().unwrap_or("");
            params.insert(key, value);
        }
        let before = output[..absolute_position]
            .strip_suffix("\u{1b}[m")
            .unwrap();
        let goto_start = before.rfind("\u{1b}[").unwrap();
        let coordinates = &before[goto_start + 2..before.len() - 1];
        let mut coordinate_parts = coordinates.split(';');
        let row: usize = coordinate_parts.next().unwrap().parse().unwrap();
        let column: usize = coordinate_parts.next().unwrap().parse().unwrap();
        crops.push((
            column - 1,
            row - 1,
            params["x"].parse().unwrap(),
            params["y"].parse().unwrap(),
            params["w"].parse().unwrap(),
            params["h"].parse().unwrap(),
        ));
        search_start = absolute_position + marker.len();
    }
    crops
}

#[test]
fn kitty_transmit_only_once_across_frames() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let frame_a = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 0, 0)], None);
    let frame_b = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 0, 0)], None);
    let combined = format!("{}{}", frame_a, frame_b);
    assert_eq!(combined.matches("\u{1b}_Ga=t").count(), 1);
    assert!(!frame_b.contains("\u{1b}_G"));
}

#[test]
fn kitty_placement_bytes_with_negative_z() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let mut chunk = kitty_chunk(internal, 1, 5, 3);
    chunk.z_index = -1;
    let output = run_kitty_frame(&parts, vec![chunk], None);
    assert!(output.contains("\u{1b}_Ga=t,q=2,f=32,t=d,i=2000000000,s=30,v=40,m=1;"));
    assert!(output.contains("\u{1b}_Gq=2,m=0;"));
    let placement = "\u{1b}[4;6H\u{1b}[m\u{1b}_Ga=p,q=2,i=2000000000,p=1,x=0,y=0,w=30,h=40,X=0,Y=0,z=-1,C=1\u{1b}\\";
    assert!(output.contains(placement));
    let save_position = output.find("\u{1b}[s").unwrap();
    let transmit_position = output.find("\u{1b}_Ga=t").unwrap();
    let restore_position = output.rfind("\u{1b}[u").unwrap();
    assert!(save_position < transmit_position);
    assert!(transmit_position < restore_position);
}

#[test]
fn kitty_host_display_clear_forces_replacement_next_frame() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let frame_1 = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 0, 0)], None);
    assert_eq!(frame_1.matches("\u{1b}_Ga=p,q=2,").count(), 1);

    let frame_2 = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 0, 0)], None);
    assert!(!frame_2.contains("\u{1b}_Ga=p,q=2,"));

    let frame_3 = run_kitty_frame_with_host_clear(&parts, vec![kitty_chunk(internal, 1, 0, 0)]);
    assert!(
        frame_3.contains("\u{1b}[2J"),
        "frame must carry the host clear"
    );
    assert_eq!(
        frame_3.matches("\u{1b}_Ga=t").count(),
        1,
        "a host display clear must force the image to be re-transmitted (host may drop image data on 2J)"
    );
    assert_eq!(
        frame_3.matches("\u{1b}_Ga=p,q=2,").count(),
        1,
        "a host display clear must force the placement to be re-emitted"
    );
    let clear_pos = frame_3.find("\u{1b}[2J").unwrap();
    let transmit_pos = frame_3.find("\u{1b}_Ga=t").unwrap();
    let place_pos = frame_3.find("\u{1b}_Ga=p,q=2,").unwrap();
    assert!(
        clear_pos < transmit_pos && transmit_pos < place_pos,
        "after a host clear the order must be 2J, then re-transmit, then re-place"
    );
}

#[test]
fn kitty_clear_string_without_explicit_mark_does_not_invalidate_host_state() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let frame_1 = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 0, 0)], None);
    assert_eq!(frame_1.matches("\u{1b}_Ga=t").count(), 1);

    let frame_2 =
        run_kitty_frame_with_unmarked_clear_string(&parts, vec![kitty_chunk(internal, 1, 0, 0)]);
    assert!(
        frame_2.contains("\u{1b}[2J"),
        "frame must carry the clear string"
    );
    assert!(
        !frame_2.contains("\u{1b}_G"),
        "host state invalidation must depend on the explicit flag, not on byte sniffing"
    );
}

#[test]
fn kitty_diff_move_remove_free_retransmit() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let frame_1 = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 2, 2)], None);
    assert_eq!(frame_1.matches("\u{1b}_Ga=t").count(), 1);
    assert!(frame_1.contains("\u{1b}_Ga=p,q=2,i=2000000000,p=1,"));
    let frame_2 = run_kitty_frame(&parts, vec![kitty_chunk(internal, 1, 2, 5)], None);
    assert!(frame_2.contains("\u{1b}_Ga=p,q=2,i=2000000000,p=1,"));
    assert!(!frame_2.contains("a=d"));
    assert!(!frame_2.contains("\u{1b}_Ga=t"));
    let frame_3 = run_kitty_frame(&parts, vec![], None);
    assert!(frame_3.contains("\u{1b}_Ga=d,q=2,d=i,i=2000000000,p=1\u{1b}\\"));
    assert!(!frame_3.contains("d=I"));
    parts.0.borrow_mut().free(internal);
    let frame_4 = run_kitty_frame(&parts, vec![], None);
    assert!(frame_4.contains("\u{1b}_Ga=d,q=2,d=I,i=2000000000\u{1b}\\"));
    let new_internal = store_test_kitty_image(&parts.0, 30, 40);
    let frame_5 = run_kitty_frame(&parts, vec![kitty_chunk(new_internal, 2, 0, 0)], None);
    assert!(frame_5.contains("\u{1b}_Ga=t,q=2,f=32,t=d,i=2000000001,"));
}

#[test]
fn kitty_placements_of_pane_dropped_from_visible_set_are_deleted() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let both_panes_visible: HashSet<PaneId> = [PaneId::Terminal(1), PaneId::Terminal(2)]
        .into_iter()
        .collect();
    let frame_1 = run_kitty_frame_for_panes(
        &parts,
        vec![
            (PaneId::Terminal(1), vec![kitty_chunk(internal, 1, 0, 0)]),
            (PaneId::Terminal(2), vec![kitty_chunk(internal, 2, 0, 10)]),
        ],
        both_panes_visible,
    );
    assert_eq!(frame_1.matches("\u{1b}_Ga=p,q=2,").count(), 2);
    assert!(!frame_1.contains("a=d"));

    let only_second_pane_visible: HashSet<PaneId> = [PaneId::Terminal(2)].into_iter().collect();
    let frame_2 = run_kitty_frame_for_panes(
        &parts,
        vec![(PaneId::Terminal(2), vec![kitty_chunk(internal, 2, 0, 10)])],
        only_second_pane_visible,
    );
    assert_eq!(
        frame_2.matches("\u{1b}_Ga=d,q=2,d=i,").count(),
        1,
        "the placement of the pane that left the visible set must be deleted"
    );
    assert!(
        frame_2.contains("\u{1b}_Ga=d,q=2,d=i,i=2000000000,p=1\u{1b}\\"),
        "the deleted placement must be the one belonging to the invisible pane"
    );
}

#[test]
fn kitty_occlusion_crops_exclude_covered_quarter() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 40, 80);
    let pane_geom = create_pane_geom(2, 0, 2, 2);
    let floating_panes_stack = FloatingPanesStack {
        layers: vec![pane_geom],
    };
    let mut chunk = kitty_chunk(internal, 1, 0, 0);
    chunk.source_px_width = 40;
    chunk.source_px_height = 80;
    chunk.dest_cells = (4, 4);
    let output = run_kitty_frame(&parts, vec![chunk], Some(floating_panes_stack));
    let crops = parse_kitty_placement_crops(&output);
    let crop_set: HashSet<(usize, usize, usize, usize, usize, usize)> =
        crops.iter().copied().collect();
    let expected_crop_set: HashSet<(usize, usize, usize, usize, usize, usize)> =
        [(0, 0, 0, 0, 20, 40), (0, 2, 0, 40, 40, 40)]
            .into_iter()
            .collect();
    assert_eq!(crop_set, expected_crop_set);
    let image_area = 40 * 80;
    let covered_area = (4 - 2) * 10 * ((2 - 0) * 20);
    let union_area: usize = crops.iter().map(|(_, _, _, _, w, h)| w * h).sum();
    assert_eq!(union_area, image_area - covered_area);
    let covered_x_range = 20..40;
    let covered_y_range = 0..40;
    for (cell_x, cell_y, _, _, w, h) in &crops {
        let absolute_x = cell_x * 10;
        let absolute_y = cell_y * 20;
        let intersects_horizontally =
            absolute_x < covered_x_range.end && absolute_x + w > covered_x_range.start;
        let intersects_vertically =
            absolute_y < covered_y_range.end && absolute_y + h > covered_y_range.start;
        assert!(
            !(intersects_horizontally && intersects_vertically),
            "crop at ({}, {}) size {}x{} intersects the covered pane rect",
            cell_x,
            cell_y,
            w,
            h
        );
    }
}

#[test]
fn kitty_capability_gating_suppresses_all_apc() {
    let parts_with_false = create_test_kitty_parts();
    parts_with_false.1.borrow_mut().insert(1, false);
    let internal = store_test_kitty_image(&parts_with_false.0, 30, 40);
    let output = run_kitty_frame(
        &parts_with_false,
        vec![kitty_chunk(internal, 1, 0, 0)],
        None,
    );
    assert!(!output.contains("\u{1b}_G"));
    assert!(!parts_with_false.2.borrow().contains_key(&1));

    let parts_with_absent = create_test_kitty_parts();
    parts_with_absent.1.borrow_mut().clear();
    let internal = store_test_kitty_image(&parts_with_absent.0, 30, 40);
    let output = run_kitty_frame(
        &parts_with_absent,
        vec![kitty_chunk(internal, 1, 0, 0)],
        None,
    );
    assert!(!output.contains("\u{1b}_G"));
    assert!(!parts_with_absent.2.borrow().contains_key(&1));
}

#[test]
fn is_dirty_with_kitty_chunks_and_pending_deletes() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 30, 40);
    let mut output = create_test_kitty_output(&parts);
    let client_ids = create_test_clients(1);
    let link_handler = Rc::new(RefCell::new(LinkHandler::new()));
    output.add_clients(&client_ids, link_handler, None);
    output.add_kitty_image_chunks_to_client(
        1,
        PaneId::Terminal(1),
        vec![kitty_chunk(internal, 1, 0, 0)],
        None,
    );
    assert!(output.is_dirty());
    assert!(output.has_rendered_assets());

    let pending_parts = create_test_kitty_parts();
    let mut transmitted = HashMap::new();
    transmitted.insert((1 as InternalImageId, None), 2_000_000_000u32);
    pending_parts.2.borrow_mut().insert(
        1,
        HostKittyState {
            transmitted,
            ..Default::default()
        },
    );
    let pending_output = create_test_kitty_output(&pending_parts);
    assert!(pending_output.is_dirty());
}

fn extract_kitty_commands(output: &str) -> Vec<Vec<u8>> {
    let bytes = output.as_bytes();
    let mut commands = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b
            && index + 2 < bytes.len()
            && bytes[index + 1] == b'_'
            && bytes[index + 2] == b'G'
        {
            let body_start = index + 3;
            let mut end = body_start;
            while end + 1 < bytes.len() && !(bytes[end] == 0x1b && bytes[end + 1] == b'\\') {
                end += 1;
            }
            commands.push(bytes[body_start..end].to_vec());
            index = end + 2;
        } else {
            index += 1;
        }
    }
    commands
}

#[test]
fn kitty_emitted_bytes_roundtrip_through_our_parser() {
    let parts = create_test_kitty_parts();
    let internal = store_test_kitty_image(&parts.0, 40, 80);
    let mut chunk = kitty_chunk(internal, 1, 2, 3);
    chunk.source_px_x = 5;
    chunk.source_px_y = 7;
    chunk.source_px_width = 20;
    chunk.source_px_height = 40;
    chunk.cell_offset_x = 3;
    chunk.cell_offset_y = 4;
    chunk.z_index = -1;
    chunk.dest_cells = (2, 2);
    let output = run_kitty_frame(&parts, vec![chunk.clone()], None);
    let commands = extract_kitty_commands(&output);
    assert!(
        !commands.is_empty(),
        "expected at least one emitted kitty command"
    );

    let mut parser = KittyCommandParser::new();
    let mut decoded_image: Option<(u32, u32, KittyFormat, usize)> = None;
    let mut placement: Option<crate::panes::kitty_graphics::parser::KittyCommand> = None;
    for raw in &commands {
        match parser.parse(raw) {
            Some(Ok(command)) => match command.action {
                KittyAction::Transmit => {
                    let image = command
                        .image
                        .as_ref()
                        .expect("transmit command must carry decoded image data");
                    decoded_image =
                        Some((image.width, image.height, image.format, image.bytes.len()));
                },
                KittyAction::Display => placement = Some(command),
                _ => {},
            },
            Some(Err(err)) => panic!("emitted command failed to parse: {:?}", err.code),
            None => {},
        }
    }

    let (width, height, format, byte_len) =
        decoded_image.expect("transmit round-trip produced no decoded image");
    assert_eq!((width, height), (40, 80));
    assert_eq!(format, KittyFormat::Rgba32);
    assert_eq!(byte_len, (40 * 80 * 4) as usize);

    let placement = placement.expect("no placement command round-tripped");
    assert_eq!(placement.source_x, chunk.source_px_x as u32);
    assert_eq!(placement.source_y, chunk.source_px_y as u32);
    assert_eq!(placement.source_w, chunk.source_px_width as u32);
    assert_eq!(placement.source_h, chunk.source_px_height as u32);
    assert_eq!(placement.cell_offset_x, chunk.cell_offset_x);
    assert_eq!(placement.cell_offset_y, chunk.cell_offset_y);
    assert_eq!(placement.z_index, chunk.z_index);
    assert!(placement.suppress_cursor_movement);
}

#[test]
fn kitty_host_ids_stay_within_signed_32_bit_range() {
    let parts = create_test_kitty_parts();
    for id in 0..8u64 {
        let internal = store_test_kitty_image(&parts.0, 4, 4);
        let output = run_kitty_frame(&parts, vec![kitty_chunk(internal, id, 0, 0)], None);
        for command in extract_kitty_commands(&output) {
            let text = String::from_utf8(command).unwrap();
            for segment in text.split([',', ';']) {
                if let Some(value) = segment.strip_prefix("i=") {
                    let numeric: i64 = value.parse().unwrap();
                    assert!(
                        numeric <= i32::MAX as i64,
                        "host image id {} exceeds signed 32-bit range (Konsole parses with toInt)",
                        numeric
                    );
                }
                if let Some(value) = segment.strip_prefix("p=") {
                    let numeric: i64 = value.parse().unwrap();
                    assert!(
                        numeric <= i32::MAX as i64,
                        "host placement id {} exceeds signed 32-bit range",
                        numeric
                    );
                }
            }
        }
    }
}
