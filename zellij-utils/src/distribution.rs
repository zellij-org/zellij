//! Zellij distributions.
//!
//! A distribution is a build of Zellij that bundles its own plugins, configuration and layouts
//! into the executable. Bundled plugins are indistinguishable from Zellij's own builtin plugins:
//! they are referenced as `zellij:<name>`, loaded from memory rather than from disk, and are not
//! subject to the plugin permission system.
//!
//! Stock Zellij is itself expressed as a distribution (see [`Distribution::zellij`]), so there is
//! only ever one code path.
//!
//! Distributions are declared at compile time by depending on the `zellij` crate and calling
//! `zellij::run` with a [`Distribution`] instead of using the `zellij` binary directly.

use std::sync::OnceLock;

/// A plugin bundled into the executable.
///
/// `bytes` is `None` when the build does not embed plugin assets (the
/// `disable_automatic_asset_installation` feature), in which case the plugin is looked up in the
/// plugin directory on disk instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionPlugin {
    pub name: &'static str,
    pub bytes: Option<&'static [u8]>,
}

impl DistributionPlugin {
    pub fn new(name: &'static str, bytes: &'static [u8]) -> Self {
        DistributionPlugin {
            name,
            bytes: Some(bytes),
        }
    }
    pub fn from_plugin_dir(name: &'static str) -> Self {
        DistributionPlugin { name, bytes: None }
    }
}

/// A layout bundled into the executable, addressable by name wherever a builtin layout name is
/// accepted (`--layout`, `default_layout`, `zellij setup --dump-layout`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionLayout {
    pub name: &'static str,
    pub layout: &'static str,
    pub swap_layout: Option<&'static str>,
}

impl DistributionLayout {
    pub fn new(name: &'static str, layout: &'static str) -> Self {
        DistributionLayout {
            name,
            layout,
            swap_layout: None,
        }
    }
    pub fn with_swap_layout(mut self, swap_layout: &'static str) -> Self {
        self.swap_layout = Some(swap_layout);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distribution {
    /// Lowercase, filesystem-safe identifier. Every path this distribution owns is named after
    /// it: `~/.config/<name>`, `/etc/<name>`, `<prefix>/share/<name>`, the temporary directory,
    /// the log file, and the command name shown in help output.
    pub name: &'static str,
    /// Human-readable name, shown in `zellij setup --check`. Defaults to `name`.
    pub display_name: &'static str,
    pub version: &'static str,
    /// `(qualifier, organization, application)` for the platform-specific config, cache, data and
    /// runtime directories. Defaults to `("", "", name)`.
    pub project_dirs: Option<(&'static str, &'static str, &'static str)>,
    /// Plugins this distribution adds on top of Zellij's builtins
    pub plugins: Vec<DistributionPlugin>,
    /// Zellij builtins this distribution drops
    pub removed_builtin_plugins: Vec<&'static str>,
    /// Layered between Zellij's default configuration and the user's `config.kdl`
    pub config: Option<&'static str>,
    pub layouts: Vec<DistributionLayout>,
    is_stock_zellij: bool,
}

impl Distribution {
    pub fn new(name: &'static str, version: &'static str) -> Self {
        Distribution {
            name,
            display_name: name,
            version,
            project_dirs: None,
            plugins: vec![],
            removed_builtin_plugins: vec![],
            config: None,
            layouts: vec![],
            is_stock_zellij: false,
        }
    }
    pub fn with_display_name(mut self, display_name: &'static str) -> Self {
        self.display_name = display_name;
        self
    }
    /// Override the `(qualifier, organization, application)` triple used to derive the platform
    /// config, cache, data and runtime directories. Only `application` is significant on Linux,
    /// where it is lowercased.
    pub fn with_project_dirs(
        mut self,
        qualifier: &'static str,
        organization: &'static str,
        application: &'static str,
    ) -> Self {
        self.project_dirs = Some((qualifier, organization, application));
        self
    }
    pub fn with_plugin(mut self, name: &'static str, bytes: &'static [u8]) -> Self {
        self.plugins.push(DistributionPlugin::new(name, bytes));
        self
    }
    /// Drop one of Zellij's builtin plugins. `zellij:<name>` then stops resolving, and anything
    /// still referring to it (a keybinding, an alias, a layout) fails to load with an error in
    /// that pane. Combine with [`Self::with_plugin`] under the same name to replace it, and
    /// remember to point the corresponding alias in the bundled configuration somewhere valid.
    ///
    /// This does not make the executable smaller: the bytes of every Zellij builtin are linked in
    /// regardless.
    pub fn without_plugin(mut self, name: &'static str) -> Self {
        self.removed_builtin_plugins.push(name);
        self
    }
    /// Drop every one of Zellij's builtin plugins, leaving only the ones this distribution
    /// bundles itself. Zellij's default configuration refers to several of them, so a
    /// distribution doing this almost certainly wants to supply its own configuration and layouts.
    pub fn without_builtin_plugins(mut self) -> Self {
        self.removed_builtin_plugins
            .extend_from_slice(ZELLIJ_BUILTIN_PLUGIN_NAMES);
        self
    }
    pub fn with_config(mut self, config: &'static str) -> Self {
        self.config = Some(config);
        self
    }
    pub fn with_layout(mut self, layout: DistributionLayout) -> Self {
        self.layouts.push(layout);
        self
    }
    /// Stock Zellij, with its own builtin plugins and no bundled configuration or layouts.
    ///
    /// Its `project_dirs` are pinned to Zellij's historical values: changing them would move
    /// every existing installation's configuration and cache.
    pub fn zellij() -> Self {
        Distribution {
            name: "zellij",
            display_name: "Zellij",
            version: crate::consts::VERSION,
            project_dirs: None,
            plugins: zellij_builtin_plugins(),
            removed_builtin_plugins: vec![],
            config: None,
            layouts: vec![],
            is_stock_zellij: true,
        }
    }
    pub fn is_stock_zellij(&self) -> bool {
        self.is_stock_zellij
    }
    /// The `(qualifier, organization, application)` triple for the platform directories
    pub fn project_dirs(&self) -> (&'static str, &'static str, &'static str) {
        if let Some(project_dirs) = self.project_dirs {
            return project_dirs;
        }
        if self.is_stock_zellij {
            if cfg!(windows) {
                return ("", "", "Zellij");
            }
            return ("org", "Zellij Contributors", "Zellij");
        }
        ("", "", self.name)
    }
    fn plugin(&self, name: &str) -> Option<&DistributionPlugin> {
        self.plugins.iter().find(|plugin| plugin.name == name)
    }
    fn layout(&self, name: &str) -> Option<&DistributionLayout> {
        self.layouts.iter().find(|layout| layout.name == name)
    }
}

pub const ZELLIJ_BUILTIN_PLUGIN_NAMES: &[&str] = &[
    "compact-bar",
    "status-bar",
    "tab-bar",
    "strider",
    "session-manager",
    "configuration",
    "plugin-manager",
    "about",
    "share",
    "multiple-select",
    "layout-manager",
    "link",
];

// Plugins are taken from:
//
// - `zellij-utils/assets/plugins`: When building in release mode OR when the
//   `plugins_from_target` feature IS NOT set
// - `zellij-utils/../target/wasm32-wasip1/debug`: When building in debug mode AND the
//   `plugins_from_target` feature IS set
#[cfg(all(
    not(target_family = "wasm"),
    not(feature = "disable_automatic_asset_installation")
))]
fn zellij_builtin_plugins() -> Vec<DistributionPlugin> {
    macro_rules! builtin_plugin {
        ($name:literal) => {
            DistributionPlugin::new(
                $name,
                #[cfg(any(not(feature = "plugins_from_target"), not(debug_assertions)))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/plugins/",
                    $name,
                    ".wasm"
                )),
                #[cfg(all(feature = "plugins_from_target", debug_assertions))]
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../target/wasm32-wasip1/debug/",
                    $name,
                    ".wasm"
                )),
            )
        };
    }
    vec![
        builtin_plugin!("compact-bar"),
        builtin_plugin!("status-bar"),
        builtin_plugin!("tab-bar"),
        builtin_plugin!("strider"),
        builtin_plugin!("session-manager"),
        builtin_plugin!("configuration"),
        builtin_plugin!("plugin-manager"),
        builtin_plugin!("about"),
        builtin_plugin!("share"),
        builtin_plugin!("multiple-select"),
        builtin_plugin!("layout-manager"),
        builtin_plugin!("link"),
    ]
}

// When the `disable_automatic_asset_installation` feature is set, no plugins are embedded at all.
// Builtin plugins must then be provided in the plugin directory.
#[cfg(any(
    target_family = "wasm",
    feature = "disable_automatic_asset_installation"
))]
fn zellij_builtin_plugins() -> Vec<DistributionPlugin> {
    ZELLIJ_BUILTIN_PLUGIN_NAMES
        .iter()
        .map(|name| DistributionPlugin::from_plugin_dir(name))
        .collect()
}

static DISTRIBUTION: OnceLock<Distribution> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributionError {
    AlreadySet,
    InvalidName(String),
    PluginNameCollidesWithBuiltin(String),
    DuplicatePluginName(String),
    DuplicateLayoutName(String),
    UnknownBuiltinPlugin(String),
}

impl std::fmt::Display for DistributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributionError::AlreadySet => {
                write!(f, "A distribution has already been set for this process")
            },
            DistributionError::InvalidName(name) => write!(
                f,
                "'{}' is not a usable distribution name. It names the configuration, cache, data \
                 and temporary directories as well as the command itself, so it must be non-empty \
                 and contain only lowercase letters, digits, '-' and '_'. Use with_display_name() \
                 for a human-readable name.",
                name
            ),
            DistributionError::PluginNameCollidesWithBuiltin(name) => write!(
                f,
                "Plugin name '{}' collides with a Zellij builtin plugin. To replace it, call \
                 without_plugin(\"{}\") as well. To leave the builtin alone and use a different \
                 implementation, give your plugin its own name and point the '{}' alias at it in \
                 the 'plugins' block of the bundled configuration.",
                name, name, name
            ),
            DistributionError::UnknownBuiltinPlugin(name) => write!(
                f,
                "'{}' is not a Zellij builtin plugin, so it cannot be removed. The builtins are: \
                 {}",
                name,
                ZELLIJ_BUILTIN_PLUGIN_NAMES.join(", ")
            ),
            DistributionError::DuplicatePluginName(name) => {
                write!(f, "Plugin name '{}' is declared more than once", name)
            },
            DistributionError::DuplicateLayoutName(name) => {
                write!(f, "Layout name '{}' is declared more than once", name)
            },
        }
    }
}

impl std::error::Error for DistributionError {}

/// Install a distribution for this process. Zellij's own builtin plugins are always included in
/// addition to the distribution's own.
///
/// Must be called before anything reads the distribution, and may only be called once.
pub fn set_distribution(mut distribution: Distribution) -> Result<(), DistributionError> {
    distribution.plugins = resolve_plugins(zellij_builtin_plugins(), &distribution)?;
    DISTRIBUTION
        .set(distribution)
        .map_err(|_| DistributionError::AlreadySet)
}

/// Validate a distribution and compute its effective plugin set: `builtins`, minus the builtins
/// it removes, plus the plugins it supplies itself.
fn resolve_plugins(
    builtins: Vec<DistributionPlugin>,
    distribution: &Distribution,
) -> Result<Vec<DistributionPlugin>, DistributionError> {
    if !is_usable_name(distribution.name) {
        return Err(DistributionError::InvalidName(distribution.name.to_owned()));
    }
    for removed in &distribution.removed_builtin_plugins {
        if !builtins.iter().any(|builtin| builtin.name == *removed) {
            return Err(DistributionError::UnknownBuiltinPlugin(
                (*removed).to_owned(),
            ));
        }
    }
    for (i, layout) in distribution.layouts.iter().enumerate() {
        if distribution.layouts[..i]
            .iter()
            .any(|existing| existing.name == layout.name)
        {
            return Err(DistributionError::DuplicateLayoutName(
                layout.name.to_owned(),
            ));
        }
    }
    let builtin_names: Vec<&'static str> = builtins.iter().map(|plugin| plugin.name).collect();
    let mut plugins = builtins;
    plugins.retain(|plugin| !distribution.removed_builtin_plugins.contains(&plugin.name));
    for plugin in &distribution.plugins {
        if plugins.iter().any(|existing| existing.name == plugin.name) {
            return if builtin_names.contains(&plugin.name) {
                Err(DistributionError::PluginNameCollidesWithBuiltin(
                    plugin.name.to_owned(),
                ))
            } else {
                Err(DistributionError::DuplicatePluginName(
                    plugin.name.to_owned(),
                ))
            };
        }
        plugins.push(plugin.clone());
    }
    Ok(plugins)
}

fn is_usable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// The distribution this process is running, defaulting to stock Zellij
pub fn distribution() -> &'static Distribution {
    DISTRIBUTION.get_or_init(Distribution::zellij)
}

/// Whether the given name refers to a plugin bundled into this executable, and may therefore be
/// referenced as `zellij:<name>`
pub fn is_builtin_plugin_name(name: &str) -> bool {
    distribution().plugin(name).is_some()
}

/// The bytes of a bundled plugin, if this build embeds them
pub fn builtin_plugin_bytes(name: &str) -> Option<&'static [u8]> {
    distribution().plugin(name).and_then(|plugin| plugin.bytes)
}

pub fn builtin_plugin_names() -> Vec<&'static str> {
    distribution()
        .plugins
        .iter()
        .map(|plugin| plugin.name)
        .collect()
}

/// Every bundled plugin whose bytes are embedded in this executable
pub fn embedded_plugins() -> Vec<(&'static str, &'static [u8])> {
    distribution()
        .plugins
        .iter()
        .filter_map(|plugin| plugin.bytes.map(|bytes| (plugin.name, bytes)))
        .collect()
}

/// The configuration bundled by this distribution, layered between Zellij's defaults and the
/// user's own configuration
pub fn bundled_config() -> Option<&'static str> {
    distribution().config
}

/// A layout bundled by this distribution, addressable by name
pub fn builtin_layout(name: &str) -> Option<&'static DistributionLayout> {
    distribution().layout(name)
}

/// The plugins this distribution supplies itself, including any replacing a Zellij builtin
pub fn own_plugin_names() -> Vec<&'static str> {
    let distribution = distribution();
    distribution
        .plugins
        .iter()
        .map(|plugin| plugin.name)
        .filter(|name| {
            !ZELLIJ_BUILTIN_PLUGIN_NAMES.contains(name)
                || distribution.removed_builtin_plugins.contains(name)
        })
        .collect()
}

/// The Zellij builtin plugins this distribution dropped and did not replace
pub fn removed_builtin_plugin_names() -> Vec<&'static str> {
    let distribution = distribution();
    distribution
        .removed_builtin_plugins
        .iter()
        .copied()
        .filter(|name| distribution.plugin(name).is_none())
        .collect()
}

/// The lowercase identifier every path and the command name are derived from
pub fn name() -> &'static str {
    distribution().name
}

/// The human-readable name
pub fn display_name() -> &'static str {
    distribution().display_name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zellij_distribution_declares_every_builtin_plugin() {
        let distribution = Distribution::zellij();
        let mut declared: Vec<&str> = distribution.plugins.iter().map(|p| p.name).collect();
        let mut expected: Vec<&str> = ZELLIJ_BUILTIN_PLUGIN_NAMES.to_vec();
        declared.sort();
        expected.sort();
        assert_eq!(declared, expected);
    }

    #[test]
    #[cfg(not(feature = "disable_automatic_asset_installation"))]
    fn zellij_builtin_plugins_are_embedded() {
        for plugin in Distribution::zellij().plugins {
            assert!(
                plugin.bytes.map(|b| !b.is_empty()).unwrap_or(false),
                "builtin plugin '{}' has no embedded bytes",
                plugin.name
            );
        }
    }

    #[test]
    fn only_bundled_plugin_names_are_builtin() {
        assert!(is_builtin_plugin_name("tab-bar"));
        assert!(!is_builtin_plugin_name("yazelix-sidebar"));
        assert!(!is_builtin_plugin_name(""));
    }

    fn fake_builtins() -> Vec<DistributionPlugin> {
        vec![
            DistributionPlugin::new("tab-bar", b"tab-bar bytes"),
            DistributionPlugin::new("status-bar", b"status-bar bytes"),
            DistributionPlugin::new("strider", b"strider bytes"),
        ]
    }

    fn resolved_names(distribution: &Distribution) -> Vec<&'static str> {
        let mut names: Vec<&'static str> = resolve_plugins(fake_builtins(), distribution)
            .expect("distribution should be valid")
            .iter()
            .map(|plugin| plugin.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_distribution_adds_its_plugins_to_the_builtins() {
        let distribution = Distribution::new("test", "0.0.0").with_plugin("sidebar", b"sidebar");
        assert_eq!(
            resolved_names(&distribution),
            vec!["sidebar", "status-bar", "strider", "tab-bar"]
        );
    }

    #[test]
    fn a_distribution_can_remove_a_builtin() {
        let distribution = Distribution::new("test", "0.0.0").without_plugin("strider");
        assert_eq!(resolved_names(&distribution), vec!["status-bar", "tab-bar"]);
    }

    #[test]
    fn a_distribution_can_remove_every_builtin() {
        let distribution = Distribution::new("test", "0.0.0")
            .without_plugin("tab-bar")
            .without_plugin("status-bar")
            .without_plugin("strider")
            .with_plugin("only-mine", b"mine");
        assert_eq!(resolved_names(&distribution), vec!["only-mine"]);
    }

    #[test]
    fn without_builtin_plugins_removes_all_of_them() {
        let distribution = Distribution::new("test", "0.0.0").without_builtin_plugins();
        let resolved = resolve_plugins(zellij_builtin_plugins(), &distribution)
            .expect("distribution should be valid");
        assert!(resolved.is_empty(), "resolved to {:?}", resolved);
    }

    #[test]
    fn a_removed_builtin_may_be_replaced_under_the_same_name() {
        let distribution = Distribution::new("test", "0.0.0")
            .without_plugin("tab-bar")
            .with_plugin("tab-bar", b"replacement");
        let resolved =
            resolve_plugins(fake_builtins(), &distribution).expect("distribution should be valid");
        let tab_bar = resolved
            .iter()
            .find(|plugin| plugin.name == "tab-bar")
            .expect("tab-bar should still be present");
        assert_eq!(tab_bar.bytes, Some(b"replacement".as_slice()));
        assert_eq!(
            resolved.iter().filter(|p| p.name == "tab-bar").count(),
            1,
            "the replacement should not sit alongside the original"
        );
    }

    #[test]
    fn removal_order_does_not_matter() {
        let after = Distribution::new("test", "0.0.0")
            .with_plugin("tab-bar", b"replacement")
            .without_plugin("tab-bar");
        let before = Distribution::new("test", "0.0.0")
            .without_plugin("tab-bar")
            .with_plugin("tab-bar", b"replacement");
        assert_eq!(resolved_names(&after), resolved_names(&before));
    }

    #[test]
    fn distribution_plugin_may_not_shadow_a_builtin() {
        let distribution = Distribution::new("test", "0.0.0").with_plugin("tab-bar", b"mine");
        assert_eq!(
            resolve_plugins(fake_builtins(), &distribution),
            Err(DistributionError::PluginNameCollidesWithBuiltin(
                "tab-bar".to_owned()
            ))
        );
    }

    #[test]
    fn a_distribution_may_not_declare_the_same_plugin_twice() {
        let distribution = Distribution::new("test", "0.0.0")
            .with_plugin("sidebar", b"one")
            .with_plugin("sidebar", b"two");
        assert_eq!(
            resolve_plugins(fake_builtins(), &distribution),
            Err(DistributionError::DuplicatePluginName("sidebar".to_owned()))
        );
    }

    #[test]
    fn a_distribution_may_not_declare_the_same_layout_twice() {
        let distribution = Distribution::new("test", "0.0.0")
            .with_layout(DistributionLayout::new("mine", "layout {}"))
            .with_layout(DistributionLayout::new("mine", "layout {}"));
        assert_eq!(
            resolve_plugins(fake_builtins(), &distribution),
            Err(DistributionError::DuplicateLayoutName("mine".to_owned()))
        );
    }

    #[test]
    fn only_real_builtins_can_be_removed() {
        let distribution = Distribution::new("test", "0.0.0").without_plugin("not-a-plugin");
        assert_eq!(
            resolve_plugins(fake_builtins(), &distribution),
            Err(DistributionError::UnknownBuiltinPlugin(
                "not-a-plugin".to_owned()
            ))
        );
    }

    #[test]
    fn distribution_name_must_be_usable_as_a_directory_name() {
        for name in ["", "Yazelix", "my distribution", "dist/ro", "diström"] {
            assert_eq!(
                resolve_plugins(fake_builtins(), &Distribution::new(name, "0.0.0")),
                Err(DistributionError::InvalidName(name.to_owned())),
                "'{}' should have been rejected",
                name
            );
        }
        assert!(is_usable_name("yazelix"));
        assert!(is_usable_name("my-dist_2"));
    }

    #[test]
    fn stock_zellij_keeps_its_historical_paths() {
        let zellij = Distribution::zellij();
        assert_eq!(zellij.name, "zellij");
        assert_eq!(zellij.display_name, "Zellij");
        if cfg!(windows) {
            assert_eq!(zellij.project_dirs(), ("", "", "Zellij"));
        } else {
            assert_eq!(
                zellij.project_dirs(),
                ("org", "Zellij Contributors", "Zellij")
            );
        }
    }

    #[test]
    fn a_distribution_derives_its_platform_dirs_from_its_name() {
        let distribution = Distribution::new("yazelix", "0.9.0").with_display_name("Yazelix");
        assert_eq!(distribution.project_dirs(), ("", "", "yazelix"));
        let overridden = distribution.with_project_dirs("dev", "Yazelix Authors", "Yazelix");
        assert_eq!(
            overridden.project_dirs(),
            ("dev", "Yazelix Authors", "Yazelix")
        );
    }
}
