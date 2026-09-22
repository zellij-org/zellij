use std::path::{Path, PathBuf};

use zellij_utils::consts::{
    system_default_config_dir, ZELLIJ_CACHE_DIR, ZELLIJ_PLUGIN_PERMISSIONS_CACHE, ZELLIJ_TMP_DIR,
    ZELLIJ_TMP_LOG_FILE,
};
use zellij_utils::distribution::{self, ZELLIJ_BUILTIN_PLUGIN_NAMES};
use zellij_utils::home::{home_config_dir, system_data_dir};
use zellij_utils::input::config::Config;
use zellij_utils::input::layout::{Layout, RunPlugin};
use zellij_utils::input::plugins::PluginConfig;

#[test]
fn the_default_distribution_is_stock_zellij() {
    let distribution = distribution::distribution();
    assert!(distribution.is_stock_zellij());
    assert_eq!(distribution.name, "zellij");
    assert_eq!(distribution.display_name, "Zellij");
    assert_eq!(distribution.version, zellij_utils::consts::VERSION);
}

#[test]
fn stock_zellij_bundles_nothing_of_its_own() {
    assert!(distribution::own_plugin_names().is_empty());
    assert!(distribution::removed_builtin_plugin_names().is_empty());
    assert!(distribution::bundled_config().is_none());
    assert!(distribution::builtin_layout("anything").is_none());
}

#[test]
fn every_builtin_plugin_resolves_and_nothing_else_does() {
    for name in ZELLIJ_BUILTIN_PLUGIN_NAMES {
        assert!(
            distribution::is_builtin_plugin_name(name),
            "'{}' should be a builtin",
            name
        );
        let run_plugin =
            RunPlugin::from_url(&format!("zellij:{}", name)).expect("the url should parse");
        let plugin = PluginConfig::from_run_plugin(&run_plugin)
            .unwrap_or_else(|| panic!("'{}' should resolve to a plugin", name));
        if cfg!(feature = "disable_automatic_asset_installation") {
            assert!(
                distribution::builtin_plugin_bytes(name).is_none(),
                "'{}' should be left to the plugin directory in this build",
                name
            );
        } else {
            let bytes = plugin
                .resolve_wasm_bytes(Path::new("/nonexistent-plugin-dir"))
                .unwrap_or_else(|e| panic!("'{}' should load from the executable: {}", name, e));
            assert!(!bytes.is_empty(), "'{}' should have bytes", name);
        }
    }
    assert!(!distribution::is_builtin_plugin_name("sidebar"));
    let unknown = RunPlugin::from_url("zellij:sidebar").expect("the url should parse");
    assert!(PluginConfig::from_run_plugin(&unknown).is_none());
}

#[test]
fn the_default_config_is_zellijs_own() {
    let config = Config::from_default_assets().expect("the default config should parse");
    for name in [
        "tab-bar",
        "status-bar",
        "strider",
        "compact-bar",
        "session-manager",
        "configuration",
        "plugin-manager",
        "about",
    ] {
        assert!(
            config.plugins.aliases.contains_key(name),
            "the '{}' alias should be present",
            name
        );
    }
    assert!(!config.plugins.aliases.contains_key("sidebar"));
    assert_eq!(config.options.default_layout, None);
}

#[test]
fn only_zellijs_own_layouts_are_addressable() {
    for name in ["default", "compact", "strider", "classic", "welcome"] {
        assert!(
            Layout::stringified_from_default_assets(Path::new(name)).is_ok(),
            "'{}' should be a builtin layout",
            name
        );
    }
    assert!(Layout::stringified_from_default_assets(Path::new("bundled")).is_err());
}

#[test]
fn every_path_keeps_its_historical_name() {
    let paths: Vec<(&str, PathBuf)> = vec![
        ("home config dir", home_config_dir().expect("a home dir")),
        ("system config dir", system_default_config_dir()),
        ("system data dir", system_data_dir()),
        ("cache dir", ZELLIJ_CACHE_DIR.clone()),
        ("permissions cache", ZELLIJ_PLUGIN_PERMISSIONS_CACHE.clone()),
        ("tmp dir", ZELLIJ_TMP_DIR.clone()),
        ("log file", ZELLIJ_TMP_LOG_FILE.clone()),
    ];
    for (what, path) in paths {
        let path = path.display().to_string();
        assert!(
            path.contains("zellij"),
            "the {} should be named after zellij, got '{}'",
            what,
            path
        );
    }
    assert!(home_config_dir()
        .expect("a home dir")
        .ends_with(".config/zellij"));
    assert_eq!(system_default_config_dir(), PathBuf::from("/etc/zellij"));
    assert!(ZELLIJ_TMP_LOG_FILE.ends_with("zellij.log"));
}

#[test]
fn the_command_is_named_zellij_and_carries_the_zellij_version() {
    let (name, version) = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let command = zellij_utils::cli::CliArgs::command_for_distribution();
            (
                command.get_name().to_owned(),
                command.get_version().map(|v| v.to_string()),
            )
        })
        .expect("failed to spawn the cli test thread")
        .join()
        .expect("the cli test thread panicked");
    assert_eq!(name, "zellij");
    assert_eq!(
        version.expect("a version should be set"),
        zellij_utils::consts::VERSION
    );
}
