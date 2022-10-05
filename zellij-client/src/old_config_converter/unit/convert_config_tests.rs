use crate::old_config_converter::config_yaml_to_config_kdl;
use insta::assert_snapshot;
use std::path::PathBuf;
use std::{fs::File, io::prelude::*};

#[test]
fn properly_convert_default_config() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_default_yaml_config.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_custom_options() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_custom_options.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_keybind_unbinds_in_mode() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_unbinds_in_mode.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_global_keybind_unbinds() -> Result<(), String> {
    let fixture = PathBuf::from(format!("{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_global_keybind_unbinds.yaml", env!("CARGO_MANIFEST_DIR")));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_unbind_all_keys_per_mode() -> Result<(), String> {
    let fixture = PathBuf::from(format!("{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_unbind_all_keys_per_mode.yaml", env!("CARGO_MANIFEST_DIR")));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_env_variables() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_env_variables.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_ui_config() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_ui.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn convert_config_with_themes_config() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_config_with_themes.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = config_yaml_to_config_kdl(&raw_config_file, false)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}
