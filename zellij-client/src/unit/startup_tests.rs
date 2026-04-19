use std::path::PathBuf;

use crate::new_session_cli_assets;
use zellij_utils::{
    cli::{CliArgs, Command, SessionCommand, Sessions},
    input::options::Options,
    pane_size::Size,
};

#[test]
fn detached_new_sessions_keep_merged_attach_options() {
    let merged_options = Options {
        default_cwd: Some(PathBuf::from("/tmp/expected-cwd")),
        theme: Some("dayfox".to_owned()),
        ..Default::default()
    };
    let cli_args = CliArgs {
        command: Some(Command::Sessions(Sessions::Attach {
            session_name: Some("detached-session".to_owned()),
            create: false,
            create_background: true,
            index: None,
            options: Some(Box::new(SessionCommand::Options(merged_options.clone()))),
            force_run_commands: false,
            token: None,
            remember: false,
            forget: false,
            ca_cert: None,
            insecure: false,
        })),
        ..Default::default()
    };

    assert_eq!(cli_args.options(), None);

    let cli_assets = new_session_cli_assets(
        &cli_args,
        &merged_options,
        None,
        None,
        Size { cols: 50, rows: 50 },
    );

    assert_eq!(cli_assets.configuration_options, Some(merged_options));

    let (config, _layout) = cli_assets.load_config_and_layout();
    let runtime_config_options = match cli_assets.configuration_options.clone() {
        Some(configuration_options) => config.options.merge(configuration_options),
        None => config.options,
    };
    assert_eq!(
        runtime_config_options.default_cwd,
        Some(PathBuf::from("/tmp/expected-cwd"))
    );
    assert_eq!(runtime_config_options.theme.as_deref(), Some("dayfox"));
}
