mod commands;
mod sessions;
#[cfg(test)]
mod tests;

use zellij_utils::{
    clap::Parser,
    cli::{CliAction, CliArgs, Command, Sessions},
    input::config::Config,
    logging::*,
};

fn main() {
    configure_logger();
    let opts = CliArgs::parse();

    {
        let config = Config::try_from(&opts).ok();
        if let Some(Command::Sessions(Sessions::Action(cli_action))) = opts.command {
            commands::send_action_to_session(cli_action, opts.session, config);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::Run {
            command,
            direction,
            cwd,
            floating,
            in_place,
            name,
            close_on_exit,
            start_suspended,
        })) = opts.command
        {
            let command_cli_action = CliAction::NewPane {
                command,
                plugin: None,
                direction,
                cwd,
                floating,
                in_place,
                name,
                close_on_exit,
                start_suspended,
                configuration: None,
            };
            commands::send_action_to_session(command_cli_action, opts.session, config);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::Edit {
            file,
            direction,
            line_number,
            floating,
            in_place,
            cwd,
        })) = opts.command
        {
            let mut file = file;
            let cwd = cwd.or_else(|| std::env::current_dir().ok());
            if file.is_relative() {
                if let Some(cwd) = cwd.as_ref() {
                    file = cwd.join(file);
                }
            }
            let command_cli_action = CliAction::Edit {
                file,
                direction,
                line_number,
                floating,
                in_place,
                cwd,
            };
            commands::send_action_to_session(command_cli_action, opts.session, config);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::ConvertConfig { old_config_file })) = opts.command {
            commands::convert_old_config_file(old_config_file);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::ConvertLayout { old_layout_file })) = opts.command {
            commands::convert_old_layout_file(old_layout_file);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::ConvertTheme { old_theme_file })) = opts.command {
            commands::convert_old_theme_file(old_theme_file);
            std::process::exit(0);
        }
    }

    if let Some(Command::Sessions(Sessions::ListSessions {
        no_formatting,
        short,
    })) = opts.command
    {
        commands::list_sessions(no_formatting, short);
    } else if let Some(Command::Sessions(Sessions::KillAllSessions { yes })) = opts.command {
        commands::kill_all_sessions(yes);
    } else if let Some(Command::Sessions(Sessions::KillSession { ref target_session })) =
        opts.command
    {
        commands::kill_session(target_session);
    } else if let Some(Command::Sessions(Sessions::DeleteAllSessions { yes, force })) = opts.command
    {
        commands::delete_all_sessions(yes, force);
    } else if let Some(Command::Sessions(Sessions::DeleteSession {
        ref target_session,
        force,
    })) = opts.command
    {
        commands::delete_session(target_session, force);
    } else if let Some(path) = opts.server {
        commands::start_server(path, opts.debug);
    } else {
        commands::start_client(opts);
    }
}
