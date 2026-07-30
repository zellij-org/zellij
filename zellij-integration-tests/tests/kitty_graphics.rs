#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, start_zellij,
    FakePtyHandle, TestSession,
};

const KITTY_PROBE_ACK: &[u8] = b"\x1b_Gi=31;OK\x1b\\";
const TRANSMIT_HEADER: &[u8] = b"\x1b_Ga=t,q=2,f=32,t=d,i=2000000000,s=2,v=2,m=0;";
const PLACEMENT: &[u8] = b"\x1b_Ga=p,q=2,i=2000000000,p=1,x=0,y=0,w=2,h=2,X=0,Y=0,z=0,C=1\x1b\\";
const IMAGE_FREE_DELETE: &[u8] = b"\x1b_Ga=d,q=2,d=I,i=2000000000\x1b\\";
const RGB_2X2_A_T: &[u8] = b"\x1b_Ga=T,q=2,f=24,s=2,v=2,m=0;////////////////\x1b\\";

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

    let bytes = zellij.wait_until_bytes(
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

    terminal.output(RGB_2X2_A_T);
    zellij.wait_until_raw_output("kitty placement reaches the client", |bytes| {
        contains_bytes(bytes, PLACEMENT)
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

