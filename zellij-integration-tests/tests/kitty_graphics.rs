#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_down_and_wait_for_prompt,
    split_right_and_wait_for_prompt, start_zellij, FakePtyHandle, TestSession,
};

const KITTY_PROBE_ACK: &[u8] = b"\x1b_Gi=31;OK\x1b\\";
const TRANSMIT_HEADER: &[u8] = b"\x1b_Ga=t,q=2,f=32,t=d,i=2000000000,s=2,v=2,m=0;";
const PLACEMENT: &[u8] = b"\x1b_Ga=p,q=2,i=2000000000,p=1,x=0,y=0,w=2,h=2,X=0,Y=0,z=0,C=1\x1b\\";
const IMAGE_FREE_DELETE: &[u8] = b"\x1b_Ga=d,q=2,d=I,i=2000000000\x1b\\";
const RGB_2X2_A_T: &[u8] = b"\x1b_Ga=T,q=2,f=24,s=2,v=2,m=0;////////////////\x1b\\";
fn rgb_tall_transmit_and_display() -> Vec<u8> {
    let width = 2usize;
    let height = 48usize;
    let raster = vec![0xffu8; width * height * 3];
    let payload = base64_encode(&raster);
    let mut out = format!("\x1b_Ga=T,q=2,f=24,s={},v={},C=1,m=0;", width, height).into_bytes();
    out.extend_from_slice(payload.as_bytes());
    out.extend_from_slice(b"\x1b\\");
    out
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn setup_kitty_host(zellij: &TestSession, terminal: &FakePtyHandle) {
    terminal.disable_echo();
    zellij.send_stdin(b"\x1b[6;21;8t");
    zellij.send_stdin(b"\x1b_Gi=31;OK\x1b\\");
    zellij.send_stdin(b"Z");
    terminal.wait_for_stdin(
        "kitty handshake barrier keystroke reached the pane",
        |stdin_bytes| stdin_bytes.contains(&b'Z'),
    );
    terminal.output(b"\x1b_Ga=q,i=31,s=1,v=1,t=d,f=24;AAAA\x1b\\");
    terminal.wait_for_stdin("kitty probe acknowledged", |stdin_bytes| {
        contains_bytes(stdin_bytes, KITTY_PROBE_ACK)
    });
}

#[test]
fn pane_kitty_image_reaches_client_with_transmit_and_placement() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &terminal);

    terminal.output(RGB_2X2_A_T);

    let bytes = zellij.wait_until_raw_output(
        "kitty transmit and placement reach the client",
        |bytes| contains_bytes(bytes, TRANSMIT_HEADER) && contains_bytes(bytes, PLACEMENT),
    );

    let placement_position = find_bytes(&bytes, PLACEMENT).unwrap();
    let before_placement = &bytes[..placement_position];
    let prompt_cursor_zero_indexed = (2usize, 1usize);
    let expected_goto_row = prompt_cursor_zero_indexed.1 + 1;
    let expected_goto_column = prompt_cursor_zero_indexed.0 + 1;
    let expected_prefix = format!("\x1b[{};{}H\x1b[m", expected_goto_row, expected_goto_column);
    assert!(
        before_placement.ends_with(expected_prefix.as_bytes()),
        "placement must be immediately preceded by a goto to the prompt cursor position {:?}, got trailing bytes: {:?}",
        expected_prefix.as_bytes(),
        &before_placement[before_placement.len().saturating_sub(16)..]
    );

    zellij.quit();
}

#[test]
fn kitty_image_at_bottom_row_scrolls_and_reaches_client() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &terminal);

    let mut fill = Vec::new();
    for _ in 0..64 {
        fill.extend_from_slice(b"\r\n");
    }
    terminal.output(&fill);
    terminal.output(RGB_2X2_A_T);

    let bytes = zellij.wait_until_raw_output(
        "kitty placement at bottom row reaches the client after scrolling",
        |bytes| contains_bytes(bytes, TRANSMIT_HEADER) && contains_bytes(bytes, PLACEMENT),
    );

    let placement_position = find_bytes(&bytes, PLACEMENT).unwrap();
    let before_placement = &bytes[..placement_position];
    let goto_marker = find_bytes(before_placement, b"\x1b[").is_some();
    assert!(
        goto_marker,
        "placement must be preceded by a cursor goto after scrolling to the bottom row"
    );

    zellij.quit();
}

#[test]
fn clear_screen_deletes_kitty_images_on_host() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &terminal);

    terminal.output(b"\x1b[H");
    terminal.output(&rgb_tall_transmit_and_display());
    zellij.wait_until_raw_output("kitty placement reaches the client", |bytes| {
        contains_bytes(bytes, b"\x1b_Ga=p,q=2,i=2000000000,p=1,")
    });

    terminal.output(b"\x1b[2J");
    zellij.wait_until_raw_output(
        "host-side image delete after clear screen",
        |bytes| contains_bytes(bytes, IMAGE_FREE_DELETE),
    );

    zellij.quit();
}

#[test]
fn closing_pane_flushes_kitty_deletes_and_host_ids_are_never_reused() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &first_terminal);
    let second_terminal = split_right_and_wait_for_prompt(&zellij);

    second_terminal.output(RGB_2X2_A_T);
    zellij.wait_until_raw_output("kitty placement from the second pane", |bytes| {
        contains_bytes(bytes, PLACEMENT)
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));

    zellij.wait_until_raw_output("host-side image delete after pane close", |bytes| {
        contains_bytes(bytes, IMAGE_FREE_DELETE)
    });

    zellij.quit();

    let bytes = zellij.raw_bytes();
    let delete_position = find_bytes(&bytes, IMAGE_FREE_DELETE).unwrap();
    let tail = &bytes[delete_position + IMAGE_FREE_DELETE.len()..];
    assert!(
        !contains_bytes(tail, b"i=2000000000"),
        "host image id 2000000000 was referenced after its delete"
    );
}

#[test]
fn opening_new_pane_keeps_existing_pane_image_on_host() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &first_terminal);

    first_terminal.output(b"\x1b[H");
    first_terminal.output(&rgb_tall_transmit_and_display());
    let bytes = zellij.wait_until_raw_output("first pane placement reaches the host", |bytes| {
        contains_bytes(bytes, b"\x1b_Ga=p,q=2,i=2000000000,p=1,")
    });
    let baseline_len = bytes.len();

    let _second_terminal = split_down_and_wait_for_prompt(&zellij);

    let bytes = zellij.wait_until_raw_output(
        "existing pane image is re-transmitted and re-placed after the layout clears the host display",
        |bytes| {
            let after_split = &bytes[baseline_len..];
            contains_bytes(after_split, b"\x1b_Ga=t") && contains_bytes(after_split, b"\x1b_Ga=p,q=2,")
        },
    );
    let after_split = &bytes[baseline_len..];
    assert!(
        contains_bytes(after_split, b"\x1b[2J"),
        "opening a new pane must clear the host display"
    );
    assert!(
        contains_bytes(after_split, b"\x1b_Ga=t"),
        "existing pane image was not re-transmitted after the host display was cleared"
    );
    assert!(
        contains_bytes(after_split, b"\x1b_Ga=p,q=2,"),
        "existing pane image was not re-placed after the host display was cleared"
    );

    zellij.quit();
}

const CELL_WIDTH: usize = 8;
const CELL_HEIGHT: usize = 21;
const IMAGE_CELL_X: usize = 20;
const IMAGE_CELL_Y: usize = 3;
const IMAGE_CELL_WIDTH: usize = 30;
const IMAGE_CELL_HEIGHT: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct CellRect {
    cell_y: usize,
    cell_x: usize,
    cell_width: usize,
    cell_height: usize,
}

impl CellRect {
    fn intersects(&self, other: &CellRect) -> bool {
        self.cell_x < other.cell_x + other.cell_width
            && other.cell_x < self.cell_x + self.cell_width
            && self.cell_y < other.cell_y + other.cell_height
            && other.cell_y < self.cell_y + self.cell_height
    }
}

fn scaled_rgb_transmit_and_display(cell_columns: usize, cell_rows: usize) -> Vec<u8> {
    let raster = vec![0xffu8; 2 * 2 * 3];
    let mut out = format!(
        "\x1b_Ga=T,q=2,f=24,s=2,v=2,c={},r={},C=1,m=0;",
        cell_columns, cell_rows
    )
    .into_bytes();
    out.extend_from_slice(base64_encode(&raster).as_bytes());
    out.extend_from_slice(b"\x1b\\");
    out
}

fn host_placements(bytes: &[u8]) -> Vec<(CellRect, u32)> {
    let text = String::from_utf8_lossy(bytes).to_string();
    let marker = "\u{1b}_Ga=p,q=2,i=2000000000,p=";
    let control_prefix = "\u{1b}_Ga=p,q=2,";
    let mut placements = vec![];
    let mut search_start = 0;
    while let Some(position) = text[search_start..].find(marker) {
        let start = search_start + position;
        let end = start
            + text[start..]
                .find("\u{1b}\\")
                .expect("unterminated placement");
        let mut placement_id = 0u32;
        let mut source_width = 0usize;
        let mut source_height = 0usize;
        for field in text[start + control_prefix.len()..end].split(',') {
            let mut parts = field.splitn(2, '=');
            let key = parts.next().unwrap_or("");
            let value = parts.next().unwrap_or("");
            match key {
                "p" => placement_id = value.parse().expect("placement id"),
                "w" => source_width = value.parse().expect("placement width"),
                "h" => source_height = value.parse().expect("placement height"),
                _ => {},
            }
        }
        let goto_end = text[..start].rfind('H').expect("placement goto");
        let goto_start = text[..goto_end]
            .rfind("\u{1b}[")
            .expect("placement goto start");
        let mut coordinates = text[goto_start + 2..goto_end].split(';');
        let row: usize = coordinates.next().unwrap().parse().expect("goto row");
        let column: usize = coordinates.next().unwrap().parse().expect("goto column");
        placements.push((
            CellRect {
                cell_y: row - 1,
                cell_x: column - 1,
                cell_width: source_width / CELL_WIDTH,
                cell_height: source_height / CELL_HEIGHT,
            },
            placement_id,
        ));
        search_start = end;
    }
    placements
}

fn floating_pane_rect(grid_snapshot: &zellij_integration_tests::GridSnapshot) -> CellRect {
    let mut top = None;
    let mut bottom = 0usize;
    let mut left = 0usize;
    let mut right = 0usize;
    for (row, line) in grid_snapshot.lines().iter().enumerate() {
        let characters: Vec<char> = line.chars().collect();
        let first = characters
            .iter()
            .position(|c| *c == '\u{250c}' || *c == '\u{2502}' || *c == '\u{2514}');
        let last = characters
            .iter()
            .rposition(|c| *c == '\u{2510}' || *c == '\u{2502}' || *c == '\u{2518}');
        if let (Some(first), Some(last)) = (first, last) {
            if top.is_none() {
                top = Some(row);
                left = first;
                right = last;
            }
            bottom = row;
        }
    }
    let top = top.expect("the floating pane frame must be visible");
    CellRect {
        cell_y: top,
        cell_x: left,
        cell_width: right - left + 1,
        cell_height: bottom - top + 1,
    }
}

fn uncovered_parts(image: CellRect, cover: CellRect) -> Vec<CellRect> {
    if !image.intersects(&cover) {
        return vec![image];
    }
    let image_bottom = image.cell_y + image.cell_height;
    let image_right = image.cell_x + image.cell_width;
    let cover_bottom = cover.cell_y + cover.cell_height;
    let cover_right = cover.cell_x + cover.cell_width;
    let mut parts = vec![];
    if cover.cell_y > image.cell_y {
        parts.push(CellRect {
            cell_y: image.cell_y,
            cell_x: image.cell_x,
            cell_width: image.cell_width,
            cell_height: cover.cell_y - image.cell_y,
        });
    }
    let middle_top = std::cmp::max(image.cell_y, cover.cell_y);
    let middle_bottom = std::cmp::min(image_bottom, cover_bottom);
    let middle_height = middle_bottom - middle_top;
    if cover.cell_x > image.cell_x {
        parts.push(CellRect {
            cell_y: middle_top,
            cell_x: image.cell_x,
            cell_width: cover.cell_x - image.cell_x,
            cell_height: middle_height,
        });
    }
    if cover_right < image_right {
        parts.push(CellRect {
            cell_y: middle_top,
            cell_x: cover_right,
            cell_width: image_right - cover_right,
            cell_height: middle_height,
        });
    }
    if cover_bottom < image_bottom {
        parts.push(CellRect {
            cell_y: cover_bottom,
            cell_x: image.cell_x,
            cell_width: image.cell_width,
            cell_height: image_bottom - cover_bottom,
        });
    }
    parts.sort();
    parts
}

#[test]
fn floating_pane_occludes_kitty_image_and_restores_it_without_retransmit() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    setup_kitty_host(&zellij, &terminal);

    let image_rect = CellRect {
        cell_y: IMAGE_CELL_Y,
        cell_x: IMAGE_CELL_X,
        cell_width: IMAGE_CELL_WIDTH,
        cell_height: IMAGE_CELL_HEIGHT,
    };
    terminal.output(format!("\x1b[{};{}H", IMAGE_CELL_Y, IMAGE_CELL_X + 1).as_bytes());
    terminal.output(&scaled_rgb_transmit_and_display(
        IMAGE_CELL_WIDTH,
        IMAGE_CELL_HEIGHT,
    ));
    let bytes = zellij.wait_until_raw_output("the full image placement reaches the host", |bytes| {
        !host_placements(bytes).is_empty()
    });
    assert_eq!(
        host_placements(&bytes),
        vec![(image_rect, 1)],
        "the unoccluded image must be emitted as a single full-size placement"
    );
    assert_eq!(
        bytes
            .windows(b"\x1b_Ga=t".len())
            .filter(|window| *window == b"\x1b_Ga=t")
            .count(),
        1,
        "the image pixels must be transmitted exactly once"
    );
    let baseline = bytes.len();

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(b"$ ");
    zellij.wait_until(
        "the floating pane covers part of the image",
        |grid_snapshot| grid_snapshot.contains("Pane #2") && grid_snapshot.contains("\u{250c}"),
    );
    let bytes = zellij.wait_until_raw_output("the occluded placements reach the host", |bytes| {
        host_placements(&bytes[baseline..]).len() >= 2
    });
    let float_rect = floating_pane_rect(&zellij.snapshot());
    assert!(
        float_rect.intersects(&image_rect),
        "the floating pane must overlap the image for this test to be meaningful"
    );
    let occluded_placements = host_placements(&bytes[baseline..]);
    for (rect, placement_id) in &occluded_placements {
        assert!(
            !rect.intersects(&float_rect),
            "placement {} at {:?} covers cells inside the floating pane rect {:?}",
            placement_id,
            rect,
            float_rect
        );
    }
    let mut occluded_rects: Vec<CellRect> =
        occluded_placements.iter().map(|(rect, _)| *rect).collect();
    occluded_rects.sort();
    assert_eq!(
        occluded_rects,
        uncovered_parts(image_rect, float_rect),
        "the emitted placements must be exactly the uncovered sub-rectangles of the image"
    );
    assert!(
        !occluded_rects.contains(&image_rect),
        "the full-image placement must no longer be emitted while the floating pane covers it"
    );
    let surplus_placement_ids: Vec<u32> = occluded_placements
        .iter()
        .map(|(_, placement_id)| *placement_id)
        .filter(|placement_id| *placement_id != 1)
        .collect();
    assert!(
        !surplus_placement_ids.is_empty(),
        "occlusion must emit at least one additional host placement id"
    );
    let baseline_after_occlusion = bytes.len();

    zellij.run_cli_action(zellij_utils::cli::CliAction::HideFloatingPanes { tab_id: None });
    let bytes = zellij.wait_until_raw_output(
        "the image is restored after the floating pane goes away",
        |bytes| host_placements(&bytes[baseline_after_occlusion..]).contains(&(image_rect, 1)),
    );
    let after_restore = &bytes[baseline_after_occlusion..];
    for placement_id in surplus_placement_ids {
        let delete = format!("\x1b_Ga=d,q=2,d=i,i=2000000000,p={}\x1b\\", placement_id);
        assert!(
            contains_bytes(after_restore, delete.as_bytes()),
            "the sub-placement {} must be deleted when the image is no longer occluded",
            placement_id
        );
    }
    assert!(
        !contains_bytes(after_restore, b"\x1b_Ga=t"),
        "restoring the image must not retransmit its pixels"
    );

    zellij.quit();
}
