use insta::assert_snapshot;
use std::path::PathBuf;

use crate::tests::fakes::FakeInputOutput;
use crate::tests::start;
use crate::tests::utils::commands::QUIT;
use crate::tests::utils::get_output_frame_snapshots;
use crate::CliArgs;
use zellij_utils::input::config::Config;
use zellij_utils::pane_size::PositionAndSize;

fn get_fake_os_input(fake_win_size: &PositionAndSize) -> FakeInputOutput {
    FakeInputOutput::new(fake_win_size.clone())
}

#[test]
pub fn accepts_basic_layout() {
    let fake_win_size = PositionAndSize {
        columns: 121,
        rows: 20,
        x: 0,
        y: 0,
        ..Default::default()
    };
    let mut fake_input_output = get_fake_os_input(&fake_win_size);
    fake_input_output.add_terminal_input(&[&QUIT]);
    let mut opts = CliArgs::default();
    opts.layout_path = Some(PathBuf::from(
        "src/tests/fixtures/layouts/three-panes-with-nesting.yaml",
    ));

    start(
        Box::new(fake_input_output.clone()),
        opts,
        Box::new(fake_input_output.clone()),
        Config::default(),
    );
    let output_frames = fake_input_output
        .stdout_writer
        .output_frames
        .lock()
        .unwrap();
    let snapshots = get_output_frame_snapshots(&output_frames, &fake_win_size);

    let snapshot_count = snapshots.len();
    let first_snapshot = snapshots.get(0).unwrap();
    let next_to_last_snapshot = snapshots.get(snapshot_count - 2).unwrap();
    let last_snapshot = snapshots.last().unwrap();
    // here we only test the first, next to last and last snapshot because there's a race condition
    // with the other snapshots. Namely all the terminals are created asynchronously and read in an
    // async task, so we have no way to guarantee the order in which their bytes will be read, and
    // it doesn't really matter in this context. We just want to see that the layout is initially
    // created properly and that in the end it's populated properly with its content
    //
    // we read the next to last as well as the last, because the last includes the "Bye from
    // Zellij" message, and we also want to make sure things are fine before that
    assert_snapshot!(first_snapshot);
    assert_snapshot!(next_to_last_snapshot);
    assert_snapshot!(last_snapshot);
}
