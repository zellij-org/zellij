use std::path::{Path, PathBuf};
use std::sync::Once;

use zellij_utils::consts::{
    system_default_config_dir, ZELLIJ_CACHE_DIR, ZELLIJ_PLUGIN_PERMISSIONS_CACHE, ZELLIJ_TMP_DIR,
    ZELLIJ_TMP_LOG_FILE,
};
use zellij_utils::distribution::{
    self, Distribution, DistributionError, DistributionLayout, DistributionPlugin,
};
use zellij_utils::home::{home_config_dir, system_data_dir};
use zellij_utils::input::config::Config;
use zellij_utils::input::layout::{Layout, RunPlugin};
use zellij_utils::input::plugins::PluginConfig;

const NAME: &str = "testdist";
const SIDEBAR_BYTES: &[u8] = b"sidebar plugin bytes";
const REPLACED_STATUS_BAR_BYTES: &[u8] = b"replacement status-bar bytes";
const REMOVED_BUILTIN: &str = "strider";
const REPLACED_BUILTIN: &str = "status-bar";
const UNTOUCHED_BUILTIN: &str = "tab-bar";

const BUNDLED_CONFIG: &str = r#"
plugins {
    sidebar location="zellij:sidebar"
}
default_layout "bundled"
scroll_buffer_size 4321
"#;

const BUNDLED_LAYOUT: &str = r#"
layout {
    pane
    pane size=1 borderless=true {
        plugin location="sidebar"
    }
}
"#;

const BUNDLED_SWAP_LAYOUT: &str = r#"
swap_tiled_layout name="bundled swap" {
    tab max_panes=1 { pane }
}
"#;

const CLI_STACK_SIZE: usize = 16 * 1024 * 1024;

fn on_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(CLI_STACK_SIZE)
        .spawn(f)
        .expect("failed to spawn the cli test thread")
        .join()
        .expect("the cli test thread panicked")
}

static INSTALL: Once = Once::new();

fn install() {
    INSTALL.call_once(|| {
        distribution::set_distribution(
            Distribution::new(NAME, "9.9.9")
                .with_display_name("Test Distribution")
                .with_plugin("sidebar", SIDEBAR_BYTES)
                .without_plugin(REMOVED_BUILTIN)
                .without_plugin(REPLACED_BUILTIN)
                .with_plugin(REPLACED_BUILTIN, REPLACED_STATUS_BAR_BYTES)
                .with_config(BUNDLED_CONFIG)
                .with_layout(
                    DistributionLayout::new("bundled", BUNDLED_LAYOUT)
                        .with_swap_layout(BUNDLED_SWAP_LAYOUT),
                ),
        )
        .expect("the test distribution should install");
    });
}

fn bytes_for(name: &str) -> Vec<u8> {
    install();
    let run_plugin =
        RunPlugin::from_url(&format!("zellij:{}", name)).expect("the url should parse");
    PluginConfig::from_run_plugin(&run_plugin)
        .unwrap_or_else(|| panic!("'{}' should resolve to a plugin", name))
        .resolve_wasm_bytes(Path::new("/nonexistent-plugin-dir"))
        .unwrap_or_else(|e| panic!("'{}' should load from the executable: {}", name, e))
}

#[test]
fn the_installed_distribution_is_reported() {
    install();
    let distribution = distribution::distribution();
    assert!(!distribution.is_stock_zellij());
    assert_eq!(distribution.name, NAME);
    assert_eq!(distribution.display_name, "Test Distribution");
    assert_eq!(distribution.version, "9.9.9");
}

#[test]
fn a_distribution_may_only_be_installed_once() {
    install();
    assert_eq!(
        distribution::set_distribution(Distribution::new("other", "0.0.0")),
        Err(DistributionError::AlreadySet)
    );
}

#[test]
fn bundled_plugins_are_addressable_as_builtins() {
    install();
    assert!(distribution::is_builtin_plugin_name("sidebar"));
    assert!(distribution::is_builtin_plugin_name(UNTOUCHED_BUILTIN));
    assert!(distribution::is_builtin_plugin_name(REPLACED_BUILTIN));
    assert!(!distribution::is_builtin_plugin_name(REMOVED_BUILTIN));
    assert!(!distribution::is_builtin_plugin_name("not-a-plugin"));
}

#[test]
fn a_bundled_plugin_loads_its_bytes_from_the_executable() {
    assert_eq!(bytes_for("sidebar"), SIDEBAR_BYTES);
}

#[test]
fn a_replaced_builtin_loads_the_replacement_bytes() {
    assert_eq!(bytes_for(REPLACED_BUILTIN), REPLACED_STATUS_BAR_BYTES);
}

#[test]
#[cfg(not(feature = "disable_automatic_asset_installation"))]
fn an_untouched_builtin_still_loads_zellij_bytes() {
    let bytes = bytes_for(UNTOUCHED_BUILTIN);
    assert!(!bytes.is_empty());
    assert_ne!(bytes, SIDEBAR_BYTES);
    assert_ne!(bytes, REPLACED_STATUS_BAR_BYTES);
}

#[test]
#[cfg(feature = "disable_automatic_asset_installation")]
fn an_untouched_builtin_is_still_registered_but_has_no_embedded_bytes() {
    install();
    assert!(distribution::is_builtin_plugin_name(UNTOUCHED_BUILTIN));
    assert!(distribution::builtin_plugin_bytes(UNTOUCHED_BUILTIN).is_none());
    assert_eq!(
        distribution::builtin_plugin_bytes("sidebar"),
        Some(SIDEBAR_BYTES)
    );
}

#[test]
fn a_removed_builtin_no_longer_resolves() {
    install();
    let run_plugin =
        RunPlugin::from_url(&format!("zellij:{}", REMOVED_BUILTIN)).expect("the url should parse");
    assert!(
        PluginConfig::from_run_plugin(&run_plugin).is_none(),
        "'{}' was removed and should not resolve",
        REMOVED_BUILTIN
    );
}

#[test]
fn own_and_removed_plugins_are_listed_separately() {
    install();
    let mut own = distribution::own_plugin_names();
    own.sort();
    assert_eq!(own, vec!["sidebar", REPLACED_BUILTIN]);
    assert_eq!(
        distribution::removed_builtin_plugin_names(),
        vec![REMOVED_BUILTIN]
    );
}

#[test]
fn embedded_plugins_include_the_bundled_ones_and_exclude_the_removed_ones() {
    install();
    let embedded: Vec<&str> = distribution::embedded_plugins()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(embedded.contains(&"sidebar"));
    assert!(embedded.contains(&REPLACED_BUILTIN));
    assert!(!embedded.contains(&REMOVED_BUILTIN));
    if cfg!(feature = "disable_automatic_asset_installation") {
        assert!(!embedded.contains(&UNTOUCHED_BUILTIN));
    } else {
        assert!(embedded.contains(&UNTOUCHED_BUILTIN));
    }
}

#[test]
fn the_bundled_config_is_layered_over_zellij_defaults() {
    install();
    let config = Config::from_default_assets().expect("the layered config should parse");

    assert!(
        config.plugins.aliases.contains_key("sidebar"),
        "the bundled alias should be present"
    );
    assert!(
        config.plugins.aliases.contains_key(UNTOUCHED_BUILTIN),
        "Zellij's default aliases should survive the layering"
    );
    assert_eq!(
        config.options.default_layout,
        Some(PathBuf::from("bundled")),
        "a bundled option should override Zellij's default"
    );
    assert_eq!(config.options.scroll_buffer_size, Some(4321));
    assert!(
        !config.keybinds.0.is_empty(),
        "Zellij's default keybinds should survive the layering"
    );
}

#[test]
fn a_bundled_layout_is_addressable_by_name() {
    install();
    let (description, layout, swap_layout) =
        Layout::stringified_from_default_assets(Path::new("bundled"))
            .expect("the bundled layout should be found");
    assert_eq!(description, "bundled layout");
    assert_eq!(layout, BUNDLED_LAYOUT);
    let (swap_description, swap_layout) = swap_layout.expect("the swap layout should be present");
    assert_eq!(swap_description, "bundled swap layout");
    assert_eq!(swap_layout, BUNDLED_SWAP_LAYOUT);
}

#[test]
fn zellijs_own_layouts_are_still_addressable() {
    install();
    let (_, layout, _) = Layout::stringified_from_default_assets(Path::new("compact"))
        .expect("Zellij's compact layout should still be found");
    assert!(layout.contains("compact-bar"));
}

#[test]
fn every_path_is_named_after_the_distribution() {
    install();
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
            path.contains(NAME),
            "the {} should be named after the distribution, got '{}'",
            what,
            path
        );
        assert!(
            !path.contains("zellij"),
            "the {} should not refer to zellij, got '{}'",
            what,
            path
        );
    }
}

#[test]
fn the_command_is_named_after_the_distribution() {
    install();
    let (name, version) = on_large_stack(|| {
        let command = zellij_utils::cli::CliArgs::command_for_distribution();
        (
            command.get_name().to_owned(),
            command.get_version().map(|v| v.to_string()),
        )
    });
    assert_eq!(name, NAME);
    assert_eq!(
        version.expect("a version should be set"),
        format!("9.9.9 (zellij {})", zellij_utils::consts::VERSION)
    );
}

#[test]
fn a_plugin_declared_by_the_distribution_carries_its_own_bytes() {
    install();
    let sidebar = distribution::distribution()
        .plugins
        .iter()
        .find(|plugin: &&DistributionPlugin| plugin.name == "sidebar")
        .expect("the sidebar plugin should be registered");
    assert_eq!(sidebar.bytes, Some(SIDEBAR_BYTES));
}
