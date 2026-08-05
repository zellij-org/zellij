#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, start_zellij, FakePtyHandle, Size, TERMINAL_SIZE,
};

const ATTACHING_CLIENT_SIZE: Size = Size { cols: 80, rows: 30 };

fn pane_size(terminal: &FakePtyHandle, what: &str) -> (u16, u16) {
    terminal.wait_for_size(what, |_, _| true)
}

#[test]
fn cold_attach_lays_out_at_the_client_reported_size_without_querying_it() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let (initial_cols, initial_rows) = pane_size(&terminal, "initial pane size");

    let attaching_client = zellij.attach_client(ATTACHING_CLIENT_SIZE);
    attaching_client.wait_until("attaching client loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.contains("Ctrl +")
            && grid_snapshot.cursor.is_some()
    });

    let (attached_cols, attached_rows) = terminal.wait_for_size(
        "pane resized to the attaching client's size",
        |cols, rows| (cols, rows) != (initial_cols, initial_rows),
    );

    let chrome_rows = TERMINAL_SIZE.rows as u16 - initial_rows;
    let expected_rows = ATTACHING_CLIENT_SIZE.rows as u16 - chrome_rows;

    assert_eq!(
        attached_cols, ATTACHING_CLIENT_SIZE.cols as u16,
        "layout must be applied at the size the attaching client reported"
    );
    assert_eq!(
        attached_rows, expected_rows,
        "layout must be applied at the size the attaching client reported"
    );

    let attaching_client_messages = attaching_client.received_server_messages();
    assert!(
        !attaching_client_messages
            .iter()
            .any(|name| name == "QueryTerminalSize"),
        "a cold attach must not be asked for its terminal size, got: {:?}",
        attaching_client_messages
    );

    let main_client_messages = zellij.received_server_messages();
    assert!(
        !main_client_messages
            .iter()
            .any(|name| name == "QueryTerminalSize"),
        "an existing client must not be asked for its terminal size when a peer attaches, got: {:?}",
        main_client_messages
    );

    attaching_client.quit();
    zellij.quit();
}
