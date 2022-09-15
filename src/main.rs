mod commands;
mod install;
mod sessions;
#[cfg(test)]
mod tests;

use zellij_utils::{
    clap::Parser,
    cli::{CliArgs, Command, Sessions},
    logging::*,
};

fn main() {
    configure_logger();
    let opts = CliArgs::parse();

    {
        if let Some(Command::Sessions(Sessions::Action(cli_action))) = opts.command {
            commands::send_action_to_session(cli_action, opts.session);
            std::process::exit(0);
        }
    }

    if let Some(Command::Sessions(Sessions::ListSessions)) = opts.command {
        commands::list_sessions();
    } else if let Some(Command::Sessions(Sessions::KillAllSessions { yes })) = opts.command {
        commands::kill_all_sessions(yes);
    } else if let Some(Command::Sessions(Sessions::KillSession { ref target_session })) =
        opts.command
    {
        commands::kill_session(target_session);
    } else if let Some(path) = opts.server {
        commands::start_server(path);
    } else {
        commands::start_client(opts);
    }
}
