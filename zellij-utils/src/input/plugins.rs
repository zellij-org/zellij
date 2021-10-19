//! Plugins configuration metadata
use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::{self, Display};
use std::fs;
use std::path::{Path, PathBuf};

use lazy_static::lazy_static;
use url::Url;

use super::config::ConfigFromYaml;
use super::layout::{RunPlugin, RunPluginLocation};
use crate::{serde, serde_yaml, setup};
use serde::{de::Deserializer, ser::Serializer, Deserialize, Serialize};
use serde_yaml::Mapping;
pub use zellij_tile::data::PluginTag;

lazy_static! {
    static ref DEFAULT_CONFIG_PLUGINS: PluginsConfig = {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec()).unwrap();
        let cfg_yaml: ConfigFromYaml = serde_yaml::from_str(cfg.as_str()).unwrap();
        PluginsConfig::try_from(cfg_yaml.plugins).unwrap()
    };
}
type JsonObject = serde_json::Map<String, serde_json::Value>;

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
                options: run.options.clone(),
            }),
            RunPluginLocation::Zellij(tag) => self.0.get(tag).cloned().map(|plugin| PluginConfig {
                _allow_exec_host_cmd: run._allow_exec_host_cmd,
                options: plugin.options.merge(run.options.clone()),
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
            plugins.insert(plugin.tag.clone(), plugin.try_into()?);
        }

        Ok(PluginsConfig(plugins))
    }
}

impl TryFrom<PluginConfigFromYaml> for PluginConfig {
    type Error = PluginsConfigError;

    fn try_from(plugin: PluginConfigFromYaml) -> Result<Self, Self::Error> {
        Ok(PluginConfig {
            path: plugin.path,
            run: match plugin.run {
                PluginTypeFromYaml::Pane => PluginType::Pane(None),
                PluginTypeFromYaml::Headless => PluginType::Headless,
            },
            _allow_exec_host_cmd: plugin._allow_exec_host_cmd,
            location: RunPluginLocation::Zellij(plugin.tag),
            options: PluginOptions::Map({
                serde_yaml::from_value(serde_yaml::Value::Mapping(plugin.options))?
            }),
        })
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
    /// Custom plugin options
    #[serde(default)]
    pub options: PluginOptions,
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
    pub options: Mapping,
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

/// PluginOptions are arbitrary options passed into plugins as they are instantiated. The variants
/// of this enum are designed to work around a limitation Zellij has in client/server
/// communications. Zellij uses the `bincode` crate to send messages between the client and server
/// but it does not support serializing/deserializing `serde_json::Map` values. Therefore we
/// have to implement custom Serialize/Deserialize traits that transcode the `PluginOptions::Map`
/// variant to a `PluginOptions::Serialized` as it's being serialized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginOptions {
    Serialized(String),
    Map(JsonObject),
}

impl PluginOptions {
    /// Merges two `PluginOptions` together to produce a new `PluginOptions` that is a union of the
    /// two. If a `PluginOptions::Serialized` variant is used, it is converted into a
    /// `PluginOptions::Map` variant, and a `PluginOptions::Map` is returned. If two maps have the
    /// same field than the later map will override the first map.
    fn merge(self, other: Self) -> Self {
        let a = JsonObject::from(self);
        let b = JsonObject::from(other);

        PluginOptions::Map(a.into_iter().chain(b.into_iter()).collect())
    }
}

impl Default for PluginOptions {
    fn default() -> Self {
        Self::Map(serde_json::Map::default())
    }
}

impl From<PluginOptions> for String {
    fn from(options: PluginOptions) -> Self {
        match options {
            PluginOptions::Serialized(string) => string,
            PluginOptions::Map(mapping) => serde_json::to_string(&mapping).unwrap(),
        }
    }
}

impl From<PluginOptions> for JsonObject {
    fn from(options: PluginOptions) -> Self {
        match options {
            PluginOptions::Serialized(string) => serde_json::from_str(&string).unwrap(),
            PluginOptions::Map(mapping) => mapping,
        }
    }
}

impl Serialize for PluginOptions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Serialized(string) => string.serialize(serializer),
            Self::Map(mappings) => serde_json::to_string(&mappings)
                .unwrap()
                .serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PluginOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let options_yaml = serde_yaml::Mapping::deserialize(deserializer);
            let options_json: JsonObject = options_yaml.map(|mapping| {
                serde_yaml::from_value(serde_yaml::Value::Mapping(mapping.clone())).unwrap_or_else(|_| {
                    panic!("Could not serialize the Yaml options into a JSON object. This can happen because Yaml supports keys that are not strings, and JSON does not. The Yaml options that couldn't be reserialized are: {:?}", mapping)
                })
            })?;

            Ok(PluginOptions::Map(options_json))
        } else {
            String::deserialize(deserializer).map(PluginOptions::Serialized)
        }
    }
}

#[derive(Debug)]
pub enum PluginsConfigError {
    DuplicatePlugins(PluginTag),
    InvalidUrl(Url),
    InvalidPluginLocation(PathBuf),
    YamlError(serde_yaml::Error),
}

impl std::error::Error for PluginsConfigError {}
impl Display for PluginsConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PluginsConfigError::DuplicatePlugins(tag) => write!(
                formatter,
                "Duplication in plugin tag names is not allowed: '{}'",
                String::from(tag.clone())
            ),
            PluginsConfigError::InvalidUrl(url) => write!(
                formatter,
                "Only 'file:' and 'zellij:' url schemes are supported for plugin lookup. '{}' does not match either.",
                url
            ),
            PluginsConfigError::InvalidPluginLocation(path) => write!(
                formatter,
                "Could not find plugin at the path: '{:?}'", path
            ),
            PluginsConfigError::YamlError(err) => write!(
                formatter,
                "{}", err
            ),
        }
    }
}

impl From<serde_yaml::Error> for PluginsConfigError {
    fn from(err: serde_yaml::Error) -> Self {
        Self::YamlError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::config::ConfigError;
    use std::convert::TryInto;

    #[test]
    fn plugin_options_merge_together() -> Result<(), ConfigError> {
        let a = PluginOptions::Map(serde_yaml::from_str(
            "
            foo: bar
            new: car
            ",
        )?);
        let b = PluginOptions::Map(serde_yaml::from_str(
            "
            boo: bar
            foo: boo
            ",
        )?);

        assert_eq!(
            a.merge(b),
            PluginOptions::Map(serde_yaml::from_str(
                "
                new: car
                boo: bar
                foo: boo
                "
            )?)
        );
        Ok(())
    }

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
                location: RunPluginLocation::Zellij(PluginTag::new("boo")),
                options: PluginOptions::default(),
            }),
            Some(PluginConfig {
                _allow_exec_host_cmd: true,
                path: PathBuf::from("boo.wasm"),
                location: RunPluginLocation::Zellij(PluginTag::new("boo")),
                options: PluginOptions::default(),
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

        assert!(matches!(
                PluginsConfig::try_from(plugins).unwrap_err(),
                PluginsConfigError::DuplicatePlugins(p) if p == PluginTag::new("boo")));
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
                location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                options: PluginOptions::default(),
            }),
            Some(PluginConfig {
                _allow_exec_host_cmd: false,
                path: PathBuf::from("boo.wasm"),
                location: RunPluginLocation::Zellij(PluginTag::new("tab-bar")),
                options: PluginOptions::default(),
                run: PluginType::Pane(None),
            })
        );

        Ok(())
    }
}
