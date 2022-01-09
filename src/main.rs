mod commands;
mod install;
mod sessions;
#[cfg(test)]
mod tests;

use zellij_utils::{
    cli::{CliArgs, Command, Sessions},
    logging::*,
    structopt::StructOpt,
};

fn main() {
    configure_logger();
    let opts = CliArgs::from_args();

    if let Some(Command::Sessions(Sessions::ListSessions)) = opts.command {
        commands::list_sessions();
    } else if let Some(Command::Sessions(Sessions::KillAllSessions { yes })) = opts.command {
        commands::kill_all_sessions(yes);
    } else if let Some(Command::Sessions(Sessions::KillSession { ref target_session })) =
        opts.command
    {
        commands::kill_session(target_session);
    } else if let Some(Command::Sessions(Sessions::RenameSession{ old_session_name, new_session_name })) = opts.command {
        commands::rename_session(old_session_name, new_session_name);
    } else if let Some(path) = opts.server {
        commands::start_server(path);
    } else {
        commands::start_client(opts);
    }
}
