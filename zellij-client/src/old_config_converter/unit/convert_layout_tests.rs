use crate::old_config_converter::layout_yaml_to_layout_kdl;
use insta::assert_snapshot;
use std::path::PathBuf;
use std::{fs::File, io::prelude::*};

#[test]
fn properly_convert_default_layout() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_default_yaml_layout.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_with_session_name() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_layout_with_session_name.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_with_session_name_and_attach_false() -> Result<(), String> {
    let fixture = PathBuf::from(format!("{}/src/old_config_converter/unit/fixtures/old_yaml_layout_with_session_name_and_attach_false.yaml", env!("CARGO_MANIFEST_DIR")));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_with_config() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_layout_with_config.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_with_config_and_session_name() -> Result<(), String> {
    let fixture = PathBuf::from(format!("{}/src/old_config_converter/unit/fixtures/old_yaml_layout_with_config_and_session_name.yaml", env!("CARGO_MANIFEST_DIR")));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_example_1() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/multiple_tabs_layout.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_example_2() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/multiple_tabs_layout_htop_command.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_example_3() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/run_htop_layout.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_example_4() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/run_htop_layout_with_plugins.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}

#[test]
fn properly_convert_layout_with_command_quoted_args() -> Result<(), String> {
    let fixture = PathBuf::from(format!(
        "{}/src/old_config_converter/unit/fixtures/old_yaml_layout_with_quoted_args.yaml",
        env!("CARGO_MANIFEST_DIR")
    ));
    let mut handle = File::open(&fixture).map_err(|e| format!("{}", e))?;
    let mut raw_config_file = String::new();
    handle
        .read_to_string(&mut raw_config_file)
        .map_err(|e| format!("{}", e))?;
    let kdl_config = layout_yaml_to_layout_kdl(&raw_config_file)?;
    assert_snapshot!(format!("{}", kdl_config));
    Ok(())
}
