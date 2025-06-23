//! Plugins configuration metadata
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

use serde::{Deserialize, Serialize};
use url::Url;

use super::layout::{PluginUserConfiguration, RunPlugin, RunPluginLocation};
#[cfg(not(target_family = "wasm"))]
use crate::consts::ASSET_MAP;
pub use crate::data::PluginTag;
use crate::errors::prelude::*;

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct PluginAliases {
    pub aliases: BTreeMap<String, RunPlugin>,
}

impl PluginAliases {
    pub fn merge(&mut self, other: Self) {
        self.aliases.extend(other.aliases);
    }
    pub fn from_data(aliases: BTreeMap<String, RunPlugin>) -> Self {
        PluginAliases { aliases }
    }
    pub fn list(&self) -> Vec<String> {
        self.aliases.keys().cloned().collect()
    }
}

/// Plugin metadata
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginConfig {
    /// Path of the plugin, see resolve_wasm_bytes for resolution semantics
    pub path: PathBuf,
    /// Allow command execution from plugin
    pub _allow_exec_host_cmd: bool,
    /// Original location of the
    pub location: RunPluginLocation,
    /// Custom configuration for this plugin
    pub userspace_configuration: PluginUserConfiguration,
    /// plugin initial working directory
    pub initial_cwd: Option<PathBuf>,
}

impl PluginConfig {
    pub fn from_run_plugin(run_plugin: &RunPlugin) -> Option<PluginConfig> {
        match &run_plugin.location {
            RunPluginLocation::File(path) => Some(PluginConfig {
                path: path.clone(),
                _allow_exec_host_cmd: run_plugin._allow_exec_host_cmd,
                location: run_plugin.location.clone(),
                userspace_configuration: run_plugin.configuration.clone(),
                initial_cwd: run_plugin.initial_cwd.clone(),
            }),
            RunPluginLocation::Zellij(tag) => {
                let tag = tag.to_string();
                if tag == "status-bar"
                    || tag == "tab-bar"
                    || tag == "compact-bar"
                    || tag == "strider"
                    || tag == "session-manager"
                    || tag == "configuration"
                    || tag == "plugin-manager"
                    || tag == "about"
                    || tag == "share"
                    || tag == "multiple-select"
                {
                    Some(PluginConfig {
                        path: PathBuf::from(&tag),
                        _allow_exec_host_cmd: run_plugin._allow_exec_host_cmd,
                        location: RunPluginLocation::parse(&format!("zellij:{}", tag), None)
                            .ok()?,
                        userspace_configuration: run_plugin.configuration.clone(),
                        initial_cwd: run_plugin.initial_cwd.clone(),
                    })
                } else {
                    None
                }
            },
            RunPluginLocation::Remote(_) => Some(PluginConfig {
                path: PathBuf::new(),
                _allow_exec_host_cmd: run_plugin._allow_exec_host_cmd,
                location: run_plugin.location.clone(),
                userspace_configuration: run_plugin.configuration.clone(),
                initial_cwd: run_plugin.initial_cwd.clone(),
            }),
        }
    }
    /// Resolve wasm plugin bytes for the plugin path and given plugin directory.
    ///
    /// If zellij was built without the 'disable_automatic_asset_installation' feature, builtin
    /// plugins (Starting with 'zellij:' in the layout file) are loaded directly from the
    /// binary-internal asset map. Otherwise:
    ///
    /// Attempts to first resolve the plugin path as an absolute path, then adds a ".wasm"
    /// extension to the path and resolves that, finally we use the plugin directory joined with
    /// the path with an appended ".wasm" extension. So if our path is "tab-bar" and the given
    /// plugin dir is "/home/bob/.zellij/plugins" the lookup chain will be this:
    ///
    /// ```bash
    ///   /tab-bar
    ///   /tab-bar.wasm
    /// ```
    ///
    pub fn resolve_wasm_bytes(&self, plugin_dir: &Path) -> Result<Vec<u8>> {
        let err_context =
            |err: std::io::Error, path: &PathBuf| format!("{}: '{}'", err, path.display());

        // Locations we check for valid plugins
        let paths_arr = [
            &self.path,
            &self.path.with_extension("wasm"),
            &plugin_dir.join(&self.path).with_extension("wasm"),
        ];
        // Throw out dupes, because it's confusing to read that zellij checked the same plugin
        // location multiple times. Do NOT sort the vector here, because it will break the lookup!
        let mut paths = paths_arr.to_vec();
        paths.dedup();

        // This looks weird and usually we would handle errors like this differently, but in this
        // case it's helpful for users and developers alike. This way we preserve all the lookup
        // errors and can report all of them back. We must initialize `last_err` with something,
        // and since the user will only get to see it when loading a plugin failed, we may as well
        // spell it out right here.
        let mut last_err: Result<Vec<u8>> = Err(anyhow!("failed to load plugin from disk"));
        for path in paths {
            // Check if the plugin path matches an entry in the asset map. If so, load it directly
            // from memory, don't bother with the disk.
            #[cfg(not(target_family = "wasm"))]
            if !cfg!(feature = "disable_automatic_asset_installation") && self.is_builtin() {
                let asset_path = PathBuf::from("plugins").join(path);
                if let Some(bytes) = ASSET_MAP.get(&asset_path) {
                    log::debug!("Loaded plugin '{}' from internal assets", path.display());

                    if plugin_dir.join(path).with_extension("wasm").exists() {
                        log::info!(
                            "Plugin '{}' exists in the 'PLUGIN DIR' at '{}' but is being ignored",
                            path.display(),
                            plugin_dir.display()
                        );
                    }

                    return Ok(bytes.to_vec());
                }
            }

            // Try to read from disk
            match fs::read(&path) {
                Ok(val) => {
                    log::debug!("Loaded plugin '{}' from disk", path.display());
                    return Ok(val);
                },
                Err(err) => {
                    last_err = last_err.with_context(|| err_context(err, &path));
                },
            }
        }

        // Not reached if a plugin is found!
        #[cfg(not(target_family = "wasm"))]
        if self.is_builtin() {
            // Layout requested a builtin plugin that wasn't found
            let plugin_path = self.path.with_extension("wasm");

            if cfg!(feature = "disable_automatic_asset_installation")
                && ASSET_MAP.contains_key(&PathBuf::from("plugins").join(&plugin_path))
            {
                return Err(ZellijError::BuiltinPluginMissing {
                    plugin_path,
                    plugin_dir: plugin_dir.to_owned(),
                    source: last_err.unwrap_err(),
                })
                .context("failed to load a plugin");
            } else {
                return Err(ZellijError::BuiltinPluginNonexistent {
                    plugin_path,
                    source: last_err.unwrap_err(),
                })
                .context("failed to load a plugin");
            }
        }

        return last_err;
    }

    pub fn is_builtin(&self) -> bool {
        matches!(self.location, RunPluginLocation::Zellij(_))
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum PluginsConfigError {
    #[error("Duplication in plugin tag names is not allowed: '{}'", String::from(.0.clone()))]
    DuplicatePlugins(PluginTag),
    #[error("Failed to parse url: {0:?}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Only 'file:', 'http(s):' and 'zellij:' url schemes are supported for plugin lookup. '{0}' does not match either.")]
    InvalidUrlScheme(Url),
    #[error("Could not find plugin at the path: '{0:?}'")]
    InvalidPluginLocation(PathBuf),
}
