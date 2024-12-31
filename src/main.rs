mod commands;
mod sessions;
#[cfg(test)]
mod tests;

use zellij_utils::{
    clap::Parser,
    cli::{CliAction, CliArgs, Command, Sessions},
    consts::create_config_and_cache_folders,
    envs,
    input::config::Config,
    logging::*,
    setup::Setup,
};

fn main() {
    configure_logger();
    create_config_and_cache_folders();
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
            x,
            y,
            width,
            height,
        })) = opts.command
        {
            let cwd = cwd.or_else(|| std::env::current_dir().ok());
            let skip_plugin_cache = false; // N/A for this action
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
                skip_plugin_cache,
                x,
                y,
                width,
                height,
            };
            commands::send_action_to_session(command_cli_action, opts.session, config);
            std::process::exit(0);
        }
        if let Some(Command::Sessions(Sessions::Plugin {
            url,
            floating,
            in_place,
            configuration,
            skip_plugin_cache,
            x,
            y,
            width,
            height,
        })) = opts.command
        {
            let cwd = None;
            let command_cli_action = CliAction::NewPane {
                command: vec![],
                plugin: Some(url),
                direction: None,
                cwd,
                floating,
                in_place,
                name: None,
                close_on_exit: false,
                start_suspended: false,
                configuration,
                skip_plugin_cache,
                x,
                y,
                width,
                height,
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
            x,
            y,
            width,
            height,
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
                x,
                y,
                width,
                height,
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
        if let Some(Command::Sessions(Sessions::Pipe {
            name,
            payload,
            args,
            plugin,
            plugin_configuration,
        })) = opts.command
        {
            let command_cli_action = CliAction::Pipe {
                name,
                payload,
                args,
                plugin,
                plugin_configuration,

                force_launch_plugin: false,
                skip_plugin_cache: false,
                floating_plugin: None,
                in_place_plugin: None,
                plugin_cwd: None,
                plugin_title: None,
            };
            commands::send_action_to_session(command_cli_action, opts.session, config);
            std::process::exit(0);
        }
    }

    if let Some(Command::Sessions(Sessions::ListSessions {
        no_formatting,
        short,
        reverse,
    })) = opts.command
    {
        commands::list_sessions(no_formatting, short, reverse);
    } else if let Some(Command::Sessions(Sessions::ListAliases)) = opts.command {
        commands::list_aliases(opts);
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
    } else if let Some(layout) = &opts.layout {
        if let Some(session_name) = opts
            .session
            .as_ref()
            .cloned()
            .or_else(|| envs::get_session_name().ok())
        {
            let config = Config::try_from(&opts).ok();
            let options = Setup::from_cli_args(&opts).ok().map(|r| r.2);
            let new_layout_cli_action = CliAction::NewTab {
                layout: Some(layout.clone()),
                layout_dir: options.as_ref().and_then(|o| o.layout_dir.clone()),
                name: None,
                cwd: options.as_ref().and_then(|o| o.default_cwd.clone()),
            };
            commands::send_action_to_session(new_layout_cli_action, Some(session_name), config);
        } else {
            commands::start_client(opts);
        }
    } else if let Some(layout_for_new_session) = &opts.new_session_with_layout {
        let mut opts = opts.clone();
        opts.new_session_with_layout = None;
        opts.layout = Some(layout_for_new_session.clone());
        commands::start_client(opts);
    } else {
        commands::start_client(opts);
    }
}
