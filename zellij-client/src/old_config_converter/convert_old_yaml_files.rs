use super::{config_yaml_to_config_kdl, layout_yaml_to_layout_kdl};
use std::path::PathBuf;
use zellij_utils::{
    cli::CliArgs,
    home::{find_default_config_dir, get_layout_dir, get_theme_dir},
};

const OLD_CONFIG_NAME: &str = "config.yaml";

pub fn convert_old_yaml_files(opts: &CliArgs) {
    let config_dir = opts.config_dir.clone().or_else(find_default_config_dir);
    let layout_dir = get_layout_dir(config_dir.clone());
    let theme_dir = get_theme_dir(find_default_config_dir());
    let specified_config_location = opts.config.as_ref();

    let mut layout_files_to_convert = vec![];
    let mut theme_files_to_convert = vec![];
    if let Some(layout) = opts.layout.as_ref() {
        if layout.extension().map(|s| s.to_string_lossy().to_string()) == Some("yaml".into()) {
            if layout.exists() {
                layout_files_to_convert.push((layout.clone(), true));
            }
        }
    }
    layout_files_to_convert.dedup();
    if let Some(layout_dir) = layout_dir {
        if let Ok(files) = std::fs::read_dir(layout_dir) {
            for file in files {
                if let Ok(file) = file {
                    if file
                        .path()
                        .extension()
                        .map(|s| s.to_string_lossy().to_string())
                        == Some("yaml".into())
                    {
                        let mut new_file_path = file.path().clone();
                        new_file_path.set_extension("kdl");
                        if !new_file_path.exists() {
                            layout_files_to_convert.push((file.path().clone(), false));
                        }
                    }
                }
            }
        }
    }

    if let Some(theme_dir) = theme_dir {
        if theme_dir.is_dir() {
            if let Ok(files) = std::fs::read_dir(theme_dir) {
                for entry in files.flatten() {
                    if let Some(extension) = entry.path().extension() {
                        if extension == "yaml" {
                            let mut new_file_path = entry.path().clone();
                            new_file_path.set_extension("kdl");
                            if !new_file_path.exists() {
                                theme_files_to_convert.push(entry.path())
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(config_dir) = config_dir {
        let yaml_config_location = specified_config_location.cloned().filter(|c| {
            c.extension().map(|s| s.to_string_lossy().to_string()) == Some("yaml".into())
        });
        let specified_yaml_config_location = yaml_config_location.is_some();
        let config_location =
            yaml_config_location.unwrap_or_else(|| config_dir.join(OLD_CONFIG_NAME));
        match convert_yaml(
            config_location,
            layout_files_to_convert,
            theme_files_to_convert,
            specified_yaml_config_location,
        ) {
            Ok(should_exit) => {
                if should_exit {
                    std::process::exit(0);
                }
            },
            Err(e) => {
                eprintln!("");
                eprintln!("\u{1b}[1;31mFailed to convert yaml config\u{1b}[m: {}", e);
                eprintln!("");
                std::process::exit(1);
            },
        }
    }
}

fn print_conversion_title_message() {
    println!("");
    println!("\u{1b}[1mZellij has moved to a new configuration format (KDL - https://kdl.dev) and has now been run with an old YAML configuration/layout/theme file.\u{1b}[m");
}

fn print_converting_config_message(old_file_name: String, new_file_name: String) {
    println!(
        "- Converting configuration file: \u{1b}[1;36m{}\u{1b}[m to the new configuration format at the same location: \u{1b}[1;36m{}\u{1b}[m",
        old_file_name,
        new_file_name
    );
}

fn print_conversion_layouts_message(layout_files_to_convert: Vec<(PathBuf, bool)>) {
    println!("- Converting the following layout YAML files to KDL files in the same location:");
    for (layout_file, _was_explicitly_set) in layout_files_to_convert.iter() {
        let mut new_layout_file_name = layout_file.clone();
        new_layout_file_name.set_extension("kdl");
        println!(
            "\u{1b}[1;36m{}\u{1b}[m => \u{1b}[1;36m{}\u{1b}[m",
            layout_file.as_path().as_os_str().to_string_lossy(),
            new_layout_file_name.as_path().as_os_str().to_string_lossy()
        );
    }
}

fn print_conversion_themes_message(theme_files_to_convert: Vec<PathBuf>) {
    println!("- Converting the following theme YAML files to KDL files in the same location:");
    for theme_file in theme_files_to_convert.iter() {
        let mut new_theme_file_name = theme_file.clone();
        new_theme_file_name.set_extension("kdl");
        println!(
            "\u{1b}[1;36m{}\u{1b}[m => \u{1b}[1;36m{}\u{1b}[m",
            theme_file.as_path().as_os_str().to_string_lossy(),
            new_theme_file_name.as_path().as_os_str().to_string_lossy()
        );
    }
}

fn print_no_actions_and_wait_for_user_input() -> Result<(), String> {
    println!("\u{1b}[1;32mNo actions are required of you. Press ENTER to continue.\u{1b}[m");
    std::io::stdin()
        .read_line(&mut String::new())
        .map_err(|e| format!("Failed to read from STDIN: {:?}", e))?;
    Ok(())
}

fn print_remain_unmodified_message(will_exit: bool) {
    println!("The original file(s) will remain unmodified.");
    if !will_exit {
        println!("Will then use the new converted file(s) for this and the next runs.");
    }
    println!("");
}

fn print_flag_help_message(
    layout_files_to_convert: Vec<(PathBuf, bool)>,
    yaml_config_file: &PathBuf,
    yaml_config_was_explicitly_set: bool,
) -> Result<(), String> {
    println!("\u{1b}[1;32mWhat do you need to do?\u{1b}[m");
    match layout_files_to_convert
        .iter()
        .find(|(_f, explicit)| *explicit)
    {
        Some((explicitly_specified_layout, _)) => {
            let mut kdl_config_file_path = yaml_config_file.clone();
            let mut kdl_explicitly_specified_layout = explicitly_specified_layout.clone();
            kdl_config_file_path.set_extension("kdl");
            kdl_explicitly_specified_layout.set_extension("kdl");
            if yaml_config_was_explicitly_set {
                println!("Since both the YAML config and a YAML layout file were explicitly specified, you'll need to re-run Zellij and point it to the new files:");
                println!(
                    "\u{1b}[1;33mzellij --config {} --layout {}\u{1b}[m",
                    kdl_config_file_path
                        .as_path()
                        .as_os_str()
                        .to_string_lossy()
                        .to_string(),
                    kdl_explicitly_specified_layout
                        .as_path()
                        .as_os_str()
                        .to_string_lossy()
                        .to_string(),
                );
            } else {
                println!("Since a YAML layout was explicitly specified, you'll need to re-run Zellij and point it to the new layout:");
                println!(
                    "\u{1b}[1;33mzellij --layout {}\u{1b}[m",
                    kdl_explicitly_specified_layout
                        .as_path()
                        .as_os_str()
                        .to_string_lossy()
                        .to_string(),
                );
            }
        },
        None => {
            if yaml_config_was_explicitly_set {
                let mut kdl_config_file_path = yaml_config_file.clone();
                kdl_config_file_path.set_extension("kdl");
                println!("Since the YAML config was explicitly specified, you'll need to re-run Zellij and point it to the new config:");
                println!(
                    "\u{1b}[1;33mzellij --config {}\u{1b}[m",
                    kdl_config_file_path
                        .as_path()
                        .as_os_str()
                        .to_string_lossy()
                        .to_string(),
                );
            }
        },
    }
    println!("");
    println!("\u{1b}[1;32mPress ENTER to continue.\u{1b}[m");
    std::io::stdin()
        .read_line(&mut String::new())
        .map_err(|e| format!("Failed to read from STDIN: {:?}", e))?;
    Ok(())
}

fn convert_layouts(layout_files_to_convert: Vec<(PathBuf, bool)>) -> Result<(), String> {
    for (layout_file, _was_explicitly_set) in layout_files_to_convert {
        let raw_layout_file = std::fs::read_to_string(&layout_file)
            .map_err(|e| format!("Failed to read layout file {:?}: {:?}", layout_file, e))?;
        let kdl_layout = layout_yaml_to_layout_kdl(&raw_layout_file)?;
        let mut new_layout_file = layout_file.clone();
        new_layout_file.set_extension("kdl");
        std::fs::write(&new_layout_file, kdl_layout).map_err(|e| {
            format!(
                "Failed to write new layout file to {:?}: {:?}",
                new_layout_file, e
            )
        })?;
    }
    Ok(())
}

fn convert_themes(theme_files_to_convert: Vec<PathBuf>) -> Result<(), String> {
    for theme_file in theme_files_to_convert {
        let raw_theme_file = std::fs::read_to_string(&theme_file)
            .map_err(|e| format!("Failed to read theme file {:?}: {:?}", theme_file, e))?;
        let kdl_theme = config_yaml_to_config_kdl(&raw_theme_file, true)?;
        let mut new_theme_file = theme_file.clone();
        new_theme_file.set_extension("kdl");
        std::fs::write(&new_theme_file, kdl_theme).map_err(|e| {
            format!(
                "Failed to write new theme file to {:?}: {:?}",
                new_theme_file, e
            )
        })?;
    }
    Ok(())
}

fn convert_config(yaml_config_file: PathBuf, new_config_file: PathBuf) -> Result<(), String> {
    if yaml_config_file.exists() && !new_config_file.exists() {
        let raw_config_file = std::fs::read_to_string(&yaml_config_file)
            .map_err(|e| format!("Failed to read config file {:?}: {:?}", yaml_config_file, e))?;
        let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
        std::fs::write(&new_config_file, kdl_config).map_err(|e| {
            format!(
                "Failed to write new config file to {:?}: {:?}",
                new_config_file, e
            )
        })?;
    }
    Ok(())
}

fn convert_yaml(
    yaml_config_file: PathBuf,
    layout_files_to_convert: Vec<(PathBuf, bool)>,
    theme_files_to_convert: Vec<PathBuf>,
    yaml_config_was_explicitly_set: bool,
) -> Result<bool, String> {
    let mut should_exit = false;
    let mut new_config_file = yaml_config_file.clone();
    new_config_file.set_extension("kdl");
    let yaml_config_file_exists = yaml_config_file.exists();
    let explicitly_set_layout_file = layout_files_to_convert
        .iter()
        .find(|(_l, was_explicitly_set)| *was_explicitly_set);
    let layout_was_explicitly_set = explicitly_set_layout_file.is_some();
    let explicitly_set_layout_files_kdl_equivalent_exists = explicitly_set_layout_file
        .map(|(layout_file, _)| {
            let mut layout_file = layout_file.clone();
            layout_file.set_extension("kdl");
            if layout_file.exists() {
                Some(true)
            } else {
                None
            }
        })
        .is_some();
    let new_config_file_exists = new_config_file.exists();
    let no_need_to_convert_config =
        (new_config_file_exists && !yaml_config_was_explicitly_set) || !yaml_config_file_exists;
    if no_need_to_convert_config
        && layout_files_to_convert.is_empty()
        && theme_files_to_convert.is_empty()
        && !layout_was_explicitly_set
    {
        // Nothing to do...
        return Ok(should_exit);
    }
    print_conversion_title_message();
    if yaml_config_file_exists && !new_config_file_exists {
        print_converting_config_message(
            yaml_config_file
                .as_path()
                .as_os_str()
                .to_string_lossy()
                .to_string(),
            new_config_file
                .as_path()
                .as_os_str()
                .to_string_lossy()
                .to_string(),
        );
    } else if yaml_config_file_exists && new_config_file_exists && yaml_config_was_explicitly_set {
        return Err(
            format!(
                "Specified old YAML format config (--config {}) but a new KDL file exists in that location. To fix, point to it the new file instead: zellij --config {}",
                yaml_config_file.as_path().as_os_str().to_string_lossy().to_string(),
                new_config_file.as_path().as_os_str().to_string_lossy().to_string()
            )
        );
    } else if layout_was_explicitly_set && explicitly_set_layout_files_kdl_equivalent_exists {
        let explicitly_set_layout_file = explicitly_set_layout_file.unwrap().0.clone();
        let mut explicitly_set_layout_file_kdl_equivalent = explicitly_set_layout_file.clone();
        explicitly_set_layout_file_kdl_equivalent.set_extension("kdl");
        return Err(
            format!(
                "Specified old YAML format layout (--layout {}) but a new KDL file exists in that location. To fix, point to it the new file instead: zellij --layout {}",
                explicitly_set_layout_file.display(),
                explicitly_set_layout_file_kdl_equivalent.display()
            )
        );
    }
    if !layout_files_to_convert.is_empty() {
        print_conversion_layouts_message(layout_files_to_convert.clone());
    }
    if !theme_files_to_convert.is_empty() {
        print_conversion_themes_message(theme_files_to_convert.clone());
    }
    print_remain_unmodified_message(layout_was_explicitly_set || yaml_config_was_explicitly_set);
    if layout_was_explicitly_set || yaml_config_was_explicitly_set {
        print_flag_help_message(
            layout_files_to_convert.clone(),
            &yaml_config_file,
            yaml_config_was_explicitly_set,
        )?;
        should_exit = true;
    } else {
        print_no_actions_and_wait_for_user_input()?;
    }
    convert_layouts(layout_files_to_convert)?;
    convert_themes(theme_files_to_convert)?;
    convert_config(yaml_config_file, new_config_file)?;
    Ok(should_exit)
}
