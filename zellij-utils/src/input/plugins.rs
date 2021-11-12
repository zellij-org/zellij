//! Plugins configuration metadata
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use url::Url;

use super::config::ConfigFromYaml;
use super::layout::{RunPlugin, RunPluginLocation};
use crate::setup;
pub use zellij_tile::data::PluginTag;

lazy_static! {
    static ref DEFAULT_CONFIG_PLUGINS: PluginsConfig = {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec()).unwrap();
        let cfg_yaml: ConfigFromYaml = serde_yaml::from_str(cfg.as_str()).unwrap();
        PluginsConfig::try_from(cfg_yaml.plugins).unwrap()
    };
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct PluginsConfigFromYaml(Vec<PluginConfigFromYaml>);

/// Used in the config struct for plugin metadata
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PluginsConfig(HashMap<PluginTag, PluginConfig>);

impl PluginsConfig {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Entrypoint from the config module
    pub fn get_plugins_with_default(user_plugins: Self) -> Self {
        let mut base_plugins = DEFAULT_CONFIG_PLUGINS.clone();
        base_plugins.0.extend(user_plugins.0);
        base_plugins
    }

    /// Get plugin config from run configuration specified in layout files.
    pub fn get(&self, run: impl Borrow<RunPlugin>) -> Option<PluginConfig> {
        let run = run.borrow();
        match &run.location {
            RunPluginLocation::File(path) => Some(PluginConfig {
                path: path.clone(),
                run: PluginType::Pane(None),
                _allow_exec_host_cmd: run._allow_exec_host_cmd,
                location: run.location.clone(),
            }),
            RunPluginLocation::Zellij(tag) => self.0.get(tag).cloned().map(|plugin| PluginConfig {
                _allow_exec_host_cmd: run._allow_exec_host_cmd,
                ..plugin
            }),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &PluginConfig> {
        self.0.values()
    }
}

impl Default for PluginsConfig {
    fn default() -> Self {
        Self::get_plugins_with_default(PluginsConfig::new())
    }
}

impl TryFrom<PluginsConfigFromYaml> for PluginsConfig {
    type Error = PluginsConfigError;

    fn try_from(yaml: PluginsConfigFromYaml) -> Result<Self, PluginsConfigError> {
        let mut plugins = HashMap::new();
        for plugin in yaml.0 {
            if plugins.contains_key(&plugin.tag) {
                return Err(PluginsConfigError::DuplicatePlugins(plugin.tag));
            }
            plugins.insert(plugin.tag.clone(), plugin.into());
        }

        Ok(PluginsConfig(plugins))
    }
}

impl From<PluginConfigFromYaml> for PluginConfig {
    fn from(plugin: PluginConfigFromYaml) -> Self {
        PluginConfig {
            path: plugin.path,
            run: match plugin.run {
                PluginTypeFromYaml::Pane => PluginType::Pane(None),
                PluginTypeFromYaml::Headless => PluginType::Headless,
            },
            _allow_exec_host_cmd: plugin._allow_exec_host_cmd,
            location: RunPluginLocation::Zellij(plugin.tag),
        }
    }
}

/// Plugin metadata
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PluginConfig {
    /// Path of the plugin, see resolve_wasm_bytes for resolution semantics
    pub path: PathBuf,
    /// Plugin type
    pub run: PluginType,
    /// Allow command execution from plugin
    pub _allow_exec_host_cmd: bool,
    /// Original location of the
    pub location: RunPluginLocation,
}

impl PluginConfig {
    /// Resolve wasm plugin bytes for the plugin path and given plugin directory. Attempts to first
    /// resolve the plugin path as an absolute path, then adds a ".wasm" extension to the path and
    /// resolves that, finally we use the plugin directoy joined with the path with an appended
    /// ".wasm" extension. So if our path is "tab-bar" and the given plugin dir is
    /// "/home/bob/.zellij/plugins" the lookup chain will be this:
    ///
    /// ```bash
    ///   /tab-bar
    ///   /tab-bar.wasm
    ///   /home/bob/.zellij/plugins/tab-bar.wasm
    /// ```
    ///
    pub fn resolve_wasm_bytes(&self, plugin_dir: &Path) -> Option<Vec<u8>> {
        fs::read(&self.path)
            .or_else(|_| fs::read(&self.path.with_extension("wasm")))
            .or_else(|_| fs::read(plugin_dir.join(&self.path).with_extension("wasm")))
            .ok()
    }

    /// Sets the tab index inside of the plugin type of the run field.
    pub fn set_tab_index(&mut self, tab_index: usize) {
        match self.run {
            PluginType::Pane(..) => {
                self.run = PluginType::Pane(Some(tab_index));
            }
            PluginType::Headless => {}
        }
    }
}

/// Type of the plugin. Defaults to Pane.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    // TODO: A plugin with output thats cloned across every pane in a tab, or across the entire
    // application might be useful
    // Tab
    // Static
    /// Starts immediately when Zellij is started and runs without a visible pane
    Headless,
    /// Runs once per pane declared inside a layout file
    Pane(Option<usize>), // tab_index
}

impl Default for PluginType {
    fn default() -> Self {
        Self::Pane(None)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
pub struct PluginConfigFromYaml {
    pub path: PathBuf,
    pub tag: PluginTag,
    #[serde(default)]
    pub run: PluginTypeFromYaml,
    #[serde(default)]
    pub config: serde_yaml::Value,
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginTypeFromYaml {
    Headless,
    Pane,
}

impl Default for PluginTypeFromYaml {
    fn default() -> Self {
        Self::Pane
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum PluginsConfigError {
    #[error("Duplication in plugin tag names is not allowed: '{}'", String::from(.0.clone()))]
    DuplicatePlugins(PluginTag),
    #[error("Only 'file:' and 'zellij:' url schemes are supported for plugin lookup. '{0}' does not match either.")]
    InvalidUrl(Url),
    #[error("Could not find plugin at the path: '{0:?}'")]
    InvalidPluginLocation(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::config::ConfigError;
    use std::convert::TryInto;

    #[test]
    fn run_plugin_permissions_are_inherited() -> Result<(), ConfigError> {
        let yaml_plugins: PluginsConfigFromYaml = serde_yaml::from_str(
            "
            - path: boo.wasm
              tag: boo
              _allow_exec_host_cmd: false
        ",
        )?;
        let plugins = PluginsConfig::try_from(yaml_plugins)?;

        assert_eq!(
            plugins.get(RunPlugin {
                _allow_exec_host_cmd: true,
                location: RunPluginLocation::Zellij(PluginTag::new("boo"))
            }),
            Some(PluginConfig {
                _allow_exec_host_cmd: true,
                path: PathBuf::from("boo.wasm"),
                location: RunPluginLocation::Zellij(PluginTag::new("boo")),
                run: PluginType::Pane(None),
            })
        );

        Ok(())
    }

    #[test]
    fn try_from_yaml_fails_when_duplicate_tag_names_are_present() -> Result<(), ConfigError> {
        let ConfigFromYaml { plugins, .. } = serde_yaml::from_str(
            "
            plugins:
                - path: /foo/bar/baz.wasm
                  tag: boo
                - path: /foo/bar/boo.wasm
                  tag: boo
        ",
        )?;

        assert_eq!(
            PluginsConfig::try_from(plugins),
            Err(PluginsConfigError::DuplicatePlugins(PluginTag::new("boo")))
        );

        Ok(())
    }

    #[test]
    fn default_plugins() -> Result<(), ConfigError> {
        let ConfigFromYaml { plugins, .. } = serde_yaml::from_str(
            "
            plugins:
                - path: boo.wasm
                  tag: boo
        ",
        )?;
        let plugins = PluginsConfig::get_plugins_with_default(plugins.try_into()?);

        assert_eq!(plugins.iter().collect::<Vec<_>>().len(), 4);
        Ok(())
    }

    #[test]
    fn default_plugins_allow_overriding() -> Result<(), ConfigError> {
        let ConfigFromYaml { plugins, .. } = serde_yaml::from_str(
            "
            plugins:
                - path: boo.wasm
                  tag: tab-bar
        ",
        )?;
        let plugins = PluginsConfig::get_plugins_with_default(plugins.try_into()?);

        assert_eq!(
            plugins.get(RunPlugin {
                _allow_exec_host_cmd: false,
                location: RunPluginLocation::Zellij(PluginTag::new("tab-bar"))
            }),
            Some(PluginConfig {
                _allow_exec_host_cmd: false,
                path: PathBuf::from("boo.wasm"),
                location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                run: PluginType::Pane(None),
            })
        );

        Ok(())
    }
}
