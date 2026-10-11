use crate::data::Styling;

#[cfg(not(target_family = "wasm"))]
use crate::data::{KeybindPresetInfo, KeybindPresetWithError, LayoutInfo, LayoutWithError};

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceCode};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;

use std::convert::TryFrom;

use super::context_menu::ContextMenuConfig;
use super::keybind_presets::KeybindsLayers;
use super::keybinds::Keybinds;
use super::layout::RunPluginOrAlias;
use super::mousebinds::Mousebinds;
use super::options::Options;
use super::plugins::{PluginAliases, PluginsConfigError};
use super::theme::{Themes, UiConfig};
use super::web_client::WebClientConfig;
use crate::cli::{CliArgs, Command};
use crate::envs::EnvironmentVariables;
use crate::{home, setup};

pub const DEFAULT_CONFIG_FILE_NAME: &str = "config.kdl";

type ConfigResult = Result<Config, ConfigError>;

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(with = "shared_keybinds")]
    pub keybinds: Arc<Keybinds>,
    pub options: Options,
    pub themes: Themes,
    pub plugins: PluginAliases,
    pub ui: UiConfig,
    pub env: EnvironmentVariables,
    pub background_plugins: Vec<RunPluginOrAlias>,
    pub web_client: WebClientConfig,
    #[serde(default)]
    pub context_menu: ContextMenuConfig,
    #[serde(default)]
    pub keybinds_layers: KeybindsLayers,
    #[serde(default, with = "shared_mousebinds")]
    pub mousebinds: Arc<Mousebinds>,
}

mod shared_mousebinds {
    use super::Mousebinds;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S: Serializer>(
        mousebinds: &Arc<Mousebinds>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        mousebinds.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Mousebinds>, D::Error> {
        Mousebinds::deserialize(deserializer).map(Arc::new)
    }
}

mod shared_keybinds {
    use super::Keybinds;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::sync::Arc;

    pub fn serialize<S: Serializer>(
        keybinds: &Arc<Keybinds>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        keybinds.as_ref().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<Keybinds>, D::Error> {
        Keybinds::deserialize(deserializer).map(Arc::new)
    }
}

#[derive(Error, Debug, Serialize, Deserialize)]
pub struct KdlError {
    pub error_message: String,
    #[serde(skip)]
    pub src: Option<NamedSource<String>>,
    pub offset: Option<usize>,
    pub len: Option<usize>,
    pub help_message: Option<String>,
}

impl Clone for KdlError {
    fn clone(&self) -> Self {
        KdlError {
            error_message: self.error_message.clone(),
            src: None, // NamedSource doesn't implement Clone, so we skip it
            offset: self.offset,
            len: self.len,
            help_message: self.help_message.clone(),
        }
    }
}

impl PartialEq for KdlError {
    fn eq(&self, other: &Self) -> bool {
        // Compare everything except src (which doesn't implement PartialEq)
        self.error_message == other.error_message
            && self.offset == other.offset
            && self.len == other.len
            && self.help_message == other.help_message
    }
}

impl Eq for KdlError {}

impl KdlError {
    pub fn add_src(mut self, src_name: String, src_input: String) -> Self {
        self.src = Some(NamedSource::new(src_name, src_input));
        self
    }
}

impl std::fmt::Display for KdlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "Failed to parse Zellij configuration")
    }
}
use std::fmt::Display;

impl Diagnostic for KdlError {
    fn source_code(&self) -> Option<&dyn SourceCode> {
        match self.src.as_ref() {
            Some(src) => Some(src),
            None => None,
        }
    }
    fn help<'a>(&'a self) -> Option<Box<dyn Display + 'a>> {
        match &self.help_message {
            Some(help_message) => Some(Box::new(help_message)),
            None => Some(Box::new(format!("For more information, please see our configuration guide: https://zellij.dev/documentation/configuration.html")))
        }
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if let (Some(offset), Some(len)) = (self.offset, self.len) {
            let label = LabeledSpan::new(Some(self.error_message.clone()), offset, len);
            Some(Box::new(std::iter::once(label)))
        } else {
            None
        }
    }
}

#[derive(Error, Debug, Diagnostic)]
pub enum ConfigError {
    // Deserialization error
    #[error("Deserialization error: {0}")]
    KdlDeserializationError(#[from] kdl::KdlError),
    #[error("KdlDeserialization error: {0}")]
    KdlError(KdlError), // TODO: consolidate these
    #[error("Config error: {0}")]
    Std(#[from] Box<dyn std::error::Error>),
    // Io error with path context
    #[error("IoError: {0}, File: {1}")]
    IoPath(io::Error, PathBuf),
    // Internal Deserialization Error
    #[error("FromUtf8Error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    // Plugins have a semantic error, usually trying to parse two of the same tag
    #[error("PluginsError: {0}")]
    PluginsError(#[from] PluginsConfigError),
    #[error("{0}")]
    ConversionError(#[from] ConversionError),
    #[error("{0}")]
    DownloadError(String),
    #[error("failed to block on async task")]
    Async(#[from] std::io::Error),
}

impl ConfigError {
    pub fn new_kdl_error(error_message: String, offset: usize, len: usize) -> Self {
        ConfigError::KdlError(KdlError {
            error_message,
            src: None,
            offset: Some(offset),
            len: Some(len),
            help_message: None,
        })
    }
    pub fn new_layout_kdl_error(error_message: String, offset: usize, len: usize) -> Self {
        ConfigError::KdlError(KdlError {
            error_message,
            src: None,
            offset: Some(offset),
            len: Some(len),
            help_message: Some(format!("For more information, please see our layout guide: https://zellij.dev/documentation/creating-a-layout.html")),
        })
    }
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("{0}")]
    UnknownInputMode(String),
}

impl TryFrom<&CliArgs> for Config {
    type Error = ConfigError;

    fn try_from(opts: &CliArgs) -> ConfigResult {
        let mut config = Config::from_cli_args_without_keybinds_dir(opts)?;
        let config_dir = opts
            .config_dir
            .clone()
            .or_else(home::find_default_config_dir);
        if config.keybinds_layers.config_dir != config_dir {
            config.keybinds_layers.config_dir = config_dir;
            config.resolve_keybinds();
        }
        Ok(config)
    }
}

impl Config {
    fn from_cli_args_without_keybinds_dir(opts: &CliArgs) -> ConfigResult {
        if let Some(ref path) = opts.config {
            let default_config = Config::from_default_assets()?;
            return Config::from_path(path, Some(default_config));
        }

        if let Some(Command::Setup(ref setup)) = opts.command {
            if setup.clean {
                return Config::from_default_assets();
            }
        }

        let config_dir = opts
            .config_dir
            .clone()
            .or_else(home::find_default_config_dir);

        if let Some(ref config) = config_dir {
            let path = config.join(DEFAULT_CONFIG_FILE_NAME);
            if path.exists() {
                let default_config = Config::from_default_assets()?;
                Config::from_path(&path, Some(default_config))
            } else {
                Config::from_default_assets()
            }
        } else {
            Config::from_default_assets()
        }
    }
}

impl Config {
    pub fn theme_config(&self, theme_name: Option<&String>) -> Option<Styling> {
        match &theme_name {
            Some(theme_name) => self.themes.get_theme(theme_name).map(|theme| theme.palette),
            None => self.themes.get_theme("default").map(|theme| theme.palette),
        }
    }
    /// Gets default configuration from assets, layered with the configuration bundled by the
    /// running distribution (if any)
    pub fn from_default_assets() -> ConfigResult {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec())?;
        let config = match Self::from_kdl(&cfg, None) {
            Ok(config) => config,
            Err(ConfigError::KdlError(kdl_error)) => {
                return Err(ConfigError::KdlError(
                    kdl_error.add_src("Default built-in-configuration".into(), cfg),
                ))
            },
            Err(e) => return Err(e),
        };
        let distribution = crate::distribution::distribution();
        match distribution.config {
            Some(distribution_config) => match Self::from_kdl(distribution_config, Some(config)) {
                Ok(config) => Ok(config),
                Err(ConfigError::KdlError(kdl_error)) => {
                    Err(ConfigError::KdlError(kdl_error.add_src(
                        format!("{} bundled configuration", distribution.name),
                        distribution_config.to_owned(),
                    )))
                },
                Err(e) => Err(e),
            },
            None => Ok(config),
        }
    }
    pub fn from_path(path: &PathBuf, default_config: Option<Config>) -> ConfigResult {
        match File::open(path) {
            Ok(mut file) => {
                let mut kdl_config = String::new();
                file.read_to_string(&mut kdl_config)
                    .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
                match Config::from_kdl(&kdl_config, default_config) {
                    Ok(config) => Ok(config),
                    Err(ConfigError::KdlDeserializationError(kdl_error)) => {
                        let error_message = match kdl_error.kind {
                            kdl::KdlErrorKind::Context("valid node terminator") => {
                                format!("Failed to deserialize KDL node. \nPossible reasons:\n{}\n{}\n{}\n{}",
                                "- Missing `;` after a node name, eg. { node; another_node; }",
                                "- Missing quotations (\") around an argument node eg. { first_node \"argument_node\"; }",
                                "- Missing an equal sign (=) between node arguments on a title line. eg. argument=\"value\"",
                                "- Found an extraneous equal sign (=) between node child arguments and their values. eg. { argument=\"value\" }")
                            },
                            _ => {
                                String::from(kdl_error.help.unwrap_or("Kdl Deserialization Error"))
                            },
                        };
                        let kdl_error = KdlError {
                            error_message,
                            src: Some(NamedSource::new(
                                path.as_path().as_os_str().to_string_lossy(),
                                kdl_config,
                            )),
                            offset: Some(kdl_error.span.offset()),
                            len: Some(kdl_error.span.len()),
                            help_message: None,
                        };
                        Err(ConfigError::KdlError(kdl_error))
                    },
                    Err(ConfigError::KdlError(kdl_error)) => {
                        Err(ConfigError::KdlError(kdl_error.add_src(
                            path.as_path().as_os_str().to_string_lossy().to_string(),
                            kdl_config,
                        )))
                    },
                    Err(e) => Err(e),
                }
            },
            Err(e) => Err(ConfigError::IoPath(e, path.into())),
        }
    }
    pub fn merge(&mut self, other: Config) -> Result<(), ConfigError> {
        self.options = self.options.merge(other.options);
        let other_layers = other.keybinds_layers;
        if !other_layers.user.is_empty()
            || !other_layers.layout.is_empty()
            || !other_layers.user_mouse.is_empty()
            || !other_layers.layout_mouse.is_empty()
        {
            self.keybinds_layers.user.merge(other_layers.user);
            self.keybinds_layers.layout.merge(other_layers.layout);
            self.keybinds_layers
                .user_mouse
                .compose(other_layers.user_mouse);
            self.keybinds_layers
                .layout_mouse
                .compose(other_layers.layout_mouse);
            self.resolve_keybinds();
        } else if !other.keybinds.0.is_empty() && !Arc::ptr_eq(&self.keybinds, &other.keybinds) {
            Arc::make_mut(&mut self.keybinds).merge(Arc::unwrap_or_clone(other.keybinds));
        }
        self.themes = self.themes.merge(other.themes);
        self.plugins.merge(other.plugins);
        self.ui = self.ui.merge(other.ui);
        self.env = self.env.merge(other.env);
        self.context_menu = self.context_menu.merge(other.context_menu);
        Ok(())
    }
    pub fn config_file_path(opts: &CliArgs) -> Option<PathBuf> {
        opts.config.clone().or_else(|| {
            opts.config_dir
                .clone()
                .or_else(|| {
                    home::try_create_home_config_dir();
                    home::find_default_config_dir()
                })
                .map(|config_dir| config_dir.join(DEFAULT_CONFIG_FILE_NAME))
        })
    }
    pub fn default_config_file_path() -> Option<PathBuf> {
        home::find_default_config_dir().map(|config_dir| config_dir.join(DEFAULT_CONFIG_FILE_NAME))
    }
    // returns true if the config was not previously written to disk and we successfully wrote it
    pub fn write_config_to_disk_if_it_does_not_exist(
        config: String,
        config_file_path: &Option<PathBuf>,
    ) -> bool {
        let Some(config_file_path) = config_file_path.clone() else {
            log::error!("Could not find file path to write config");
            return false;
        };
        if config_file_path.exists() {
            false
        } else {
            if let Err(e) = std::fs::write(&config_file_path, config.as_bytes()) {
                log::error!("Failed to write config to disk: {}", e);
                return false;
            }
            match std::fs::read_to_string(&config_file_path) {
                Ok(written_config) => written_config == config,
                Err(e) => {
                    log::error!("Failed to read written config: {}", e);
                    false
                },
            }
        }
    }
    pub fn backup_file_path(config_file_path: &Path) -> PathBuf {
        let config_file_name = config_file_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or_else(|| DEFAULT_CONFIG_FILE_NAME);
        config_file_path.with_file_name(format!("{}.bak", config_file_name))
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn load_config_file(config_file_path: &Path, config_dir: Option<&Path>) -> Option<Config> {
    let mut cli_args = CliArgs::default();
    cli_args.config = Some(config_file_path.to_path_buf());
    cli_args.config_dir = config_dir.map(Path::to_path_buf);
    crate::setup::Setup::from_cli_args(&cli_args)
        .map(|(config, ..)| config)
        .map_err(|e| log::error!("Failed to load {}: {}", config_file_path.display(), e))
        .ok()
}

#[cfg(not(target_family = "wasm"))]
pub async fn watch_config_file_changes<F, Fut>(
    config_file_path: PathBuf,
    config_dir: Option<&Path>,
    on_config_change: F,
) where
    F: Fn(Config) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    // in a gist, what we do here is fire the `on_config_change` function whenever there is a
    // change in the config file or the configured theme directory, we do this by:
    // 1. Trying to watch the provided config file for changes
    // 2. If the file is deleted or does not exist, we periodically poll for it (manually, not
    //    through filesystem events)
    // 3. Once it exists, we start watching it for changes again
    //
    // we do this because the alternative is to watch its parent folder and this might cause the
    // classic "too many open files" issue if there are a lot of files there and/or lots of Zellij
    // instances
    use crate::setup::Setup;
    use notify::{self, Config as WatcherConfig, Event, PollWatcher, RecursiveMode, Watcher};
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn cli_args_for_config(config_file_path: &Path, config_dir: Option<&Path>) -> CliArgs {
        let mut cli_args_for_config = CliArgs::default();
        cli_args_for_config.config = Some(config_file_path.to_path_buf());
        cli_args_for_config.config_dir = config_dir.map(Path::to_path_buf);
        cli_args_for_config
    }

    fn load_config_and_watched_paths(
        config_file_path: &Path,
        config_dir: Option<&Path>,
    ) -> Option<(Config, Vec<PathBuf>, Option<PathBuf>)> {
        let cli_args_for_config = cli_args_for_config(config_file_path, config_dir);
        Setup::from_cli_args(&cli_args_for_config)
            .map(|(config, _, config_options, _, _)| {
                let mut watched_paths = vec![];
                let theme_dir = config_options.theme_dir.or_else(|| {
                    let config_dir = config_dir
                        .map(Path::to_path_buf)
                        .or_else(home::find_default_config_dir);
                    home::get_theme_dir(config_dir)
                });
                let missing_theme_dir = theme_dir.clone().filter(|dir| !dir.exists());
                watched_paths.extend(theme_dir.filter(|dir| dir.exists()));
                let keybinds_dir = config.keybinds_dir().filter(|dir| dir.exists());
                if let Some(preset_file) = config.active_keybind_preset_file() {
                    let is_in_keybinds_dir = keybinds_dir
                        .as_ref()
                        .map(|dir| preset_file.starts_with(dir))
                        .unwrap_or(false);
                    if !is_in_keybinds_dir && preset_file.exists() {
                        watched_paths.push(preset_file);
                    }
                }
                watched_paths.extend(keybinds_dir);
                (config, watched_paths, missing_theme_dir)
            })
            .ok()
    }

    fn event_is_for_config_file(event: &Event, config_file_path: &Path) -> bool {
        event.paths.iter().any(|path| path == config_file_path)
    }

    fn event_is_in_watched_paths(event: &Event, watched_paths: &[PathBuf]) -> bool {
        watched_paths.iter().any(|watched_path| {
            event
                .paths
                .iter()
                .any(|path| path.starts_with(watched_path))
        })
    }

    async fn reload_config_after_change<F, Fut>(
        config_file_path: &Path,
        config_dir: Option<&Path>,
        watched_paths: &[PathBuf],
        on_config_change: &F,
    ) -> Option<bool>
    where
        F: Fn(Config) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        tokio::time::sleep(Duration::from_millis(100)).await;

        if !config_file_path.exists() {
            return None;
        }

        let (new_config, new_watched_paths, _) =
            match load_config_and_watched_paths(config_file_path, config_dir) {
                Some(loaded) => loaded,
                None => {
                    log::error!("Failed to reload config from {:?}", config_file_path);
                    return None;
                },
            };
        on_config_change(new_config).await;
        Some(new_watched_paths.as_slice() != watched_paths)
    }

    loop {
        if config_file_path.exists() {
            let (watched_paths, missing_theme_dir) =
                load_config_and_watched_paths(config_file_path.as_path(), config_dir)
                    .map(|(_, watched_paths, missing_theme_dir)| (watched_paths, missing_theme_dir))
                    .unwrap_or_default();
            let (tx, mut rx) = mpsc::unbounded_channel();

            let mut watcher = match PollWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    let _ = tx.send(res);
                },
                WatcherConfig::default().with_poll_interval(Duration::from_secs(1)),
            ) {
                Ok(watcher) => watcher,
                Err(e) => {
                    log::error!("Failed to create config watcher: {}", e);
                    break;
                },
            };

            if let Err(e) = watcher.watch(&config_file_path, RecursiveMode::NonRecursive) {
                log::error!("Failed to watch config file {:?}: {}", config_file_path, e);
                break;
            }

            for watched_path in &watched_paths {
                if let Err(e) = watcher.watch(watched_path, RecursiveMode::NonRecursive) {
                    log::error!(
                        "Failed to watch {:?}, continuing without it: {}",
                        watched_path,
                        e,
                    );
                }
            }

            loop {
                let event_result = tokio::select! {
                    received = rx.recv() => match received {
                        Some(event_result) => event_result,
                        None => break,
                    },
                    _ = tokio::time::sleep(Duration::from_secs(3)), if missing_theme_dir.is_some() => {
                        let created = missing_theme_dir
                            .as_ref()
                            .map(|dir| dir.exists())
                            .unwrap_or(false);
                        if created {
                            reload_config_after_change(
                                config_file_path.as_path(),
                                config_dir,
                                &watched_paths,
                                &on_config_change,
                            )
                            .await;
                            break;
                        }
                        continue;
                    },
                };
                let event = match event_result {
                    Ok(event) => event,
                    Err(e) => {
                        log::error!("Config watcher event error: {}", e);
                        break;
                    },
                };

                if event_is_for_config_file(&event, config_file_path.as_path()) {
                    if event.kind.is_remove() {
                        break;
                    }

                    if event.kind.is_create() || event.kind.is_modify() {
                        if reload_config_after_change(
                            config_file_path.as_path(),
                            config_dir,
                            &watched_paths,
                            &on_config_change,
                        )
                        .await
                        .unwrap_or(false)
                        {
                            break;
                        }
                    }
                } else if event_is_in_watched_paths(&event, &watched_paths)
                    && (event.kind.is_remove() || event.kind.is_create() || event.kind.is_modify())
                {
                    let should_restart_watcher = reload_config_after_change(
                        config_file_path.as_path(),
                        config_dir,
                        &watched_paths,
                        &on_config_change,
                    )
                    .await
                    .unwrap_or(true);
                    if should_restart_watcher {
                        break;
                    }
                }
            }
        }

        while !config_file_path.exists() {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub async fn watch_layout_dir_changes<F, Fut>(
    layout_dir: PathBuf,
    default_layout_name: Option<String>,
    on_layout_change: F,
) where
    F: Fn(Vec<LayoutInfo>, Vec<LayoutWithError>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    use crate::input::layout::Layout;
    use notify::{self, Config as WatcherConfig, Event, PollWatcher, RecursiveMode, Watcher};
    use std::time::Duration;
    use tokio::sync::mpsc;

    loop {
        if layout_dir.exists() {
            let (tx, mut rx) = mpsc::unbounded_channel();

            let mut watcher = match PollWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    let _ = tx.send(res);
                },
                WatcherConfig::default().with_poll_interval(Duration::from_secs(1)),
            ) {
                Ok(watcher) => watcher,
                Err(_) => break,
            };

            if watcher
                .watch(&layout_dir, RecursiveMode::Recursive)
                .is_err()
            {
                break;
            }

            while let Some(event_result) = rx.recv().await {
                match event_result {
                    Ok(event) => {
                        if event.kind.is_remove()
                            || event.kind.is_create()
                            || event.kind.is_modify()
                        {
                            tokio::time::sleep(Duration::from_millis(100)).await;

                            if !layout_dir.exists() {
                                break;
                            }

                            let (layouts, layout_errors) = Layout::list_available_layouts(
                                Some(layout_dir.clone()),
                                &default_layout_name,
                            );
                            on_layout_change(layouts, layout_errors).await;
                        }
                    },
                    Err(_) => break,
                }
            }
        }

        while !layout_dir.exists() {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}

#[cfg(not(target_family = "wasm"))]
pub async fn watch_keybinds_dir_changes<F, Fut>(
    keybinds_dir: PathBuf,
    stop: Arc<std::sync::atomic::AtomicBool>,
    on_presets_change: F,
) where
    F: Fn(Vec<KeybindPresetInfo>, Vec<KeybindPresetWithError>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send,
{
    use super::keybind_presets::list_keybind_presets;
    use notify::{self, Config as WatcherConfig, Event, PollWatcher, RecursiveMode, Watcher};
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tokio::sync::mpsc;

    let stopped = || stop.load(Ordering::SeqCst);
    while !stopped() {
        if keybinds_dir.exists() {
            let (tx, mut rx) = mpsc::unbounded_channel();

            let mut watcher = match PollWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    let _ = tx.send(res);
                },
                WatcherConfig::default().with_poll_interval(Duration::from_secs(1)),
            ) {
                Ok(watcher) => watcher,
                Err(_) => break,
            };

            if watcher
                .watch(&keybinds_dir, RecursiveMode::NonRecursive)
                .is_err()
            {
                break;
            }
            let (presets, preset_errors) = list_keybind_presets(Some(&keybinds_dir), &[]);
            on_presets_change(presets, preset_errors).await;

            loop {
                if stopped() {
                    return;
                }
                let event_result =
                    match tokio::time::timeout(Duration::from_millis(250), rx.recv()).await {
                        Err(_) => continue,
                        Ok(None) => break,
                        Ok(Some(event_result)) => event_result,
                    };
                match event_result {
                    Ok(event) => {
                        if event.kind.is_remove()
                            || event.kind.is_create()
                            || event.kind.is_modify()
                        {
                            tokio::time::sleep(Duration::from_millis(100)).await;

                            if stopped() {
                                return;
                            }
                            if !keybinds_dir.exists() {
                                break;
                            }

                            let (presets, preset_errors) =
                                list_keybind_presets(Some(&keybinds_dir), &[]);
                            on_presets_change(presets, preset_errors).await;
                        }
                    },
                    Err(_) => break,
                }
            }
            if stopped() {
                return;
            }
            let (presets, preset_errors) = list_keybind_presets(None, &[]);
            on_presets_change(presets, preset_errors).await;
        }

        while !keybinds_dir.exists() && !stopped() {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

#[cfg(test)]
mod config_test {
    use super::*;
    use crate::data::{InputMode, Palette, PaletteColor, StyleDeclaration, Styling};
    use crate::input::layout::RunPlugin;
    use crate::input::options::{Clipboard, OnForceClose};
    use crate::input::theme::{FrameConfig, Theme, Themes, UiConfig};
    use std::collections::{BTreeMap, HashMap};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn try_from_cli_args_with_config() {
        // makes sure loading a config file with --config tries to load the config
        let arbitrary_config = PathBuf::from("nonexistent.yaml");
        let opts = CliArgs {
            config: Some(arbitrary_config),
            ..Default::default()
        };
        println!("OPTS= {:?}", opts);
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_option_clean() {
        // makes sure --clean works... TODO: how can this actually fail now?
        use crate::setup::Setup;
        let opts = CliArgs {
            command: Some(Command::Setup(Setup {
                clean: true,
                ..Setup::default()
            })),
            ..Default::default()
        };
        let result = Config::try_from(&opts);
        assert!(result.is_ok());
    }

    #[test]
    fn try_from_cli_args_with_config_dir() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        File::create(tmp.path().join(DEFAULT_CONFIG_FILE_NAME))
            .unwrap()
            .write_all(b"keybinds: invalid\n")
            .unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_config_dir_without_config() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        let mut expected = Config::from_default_assets().unwrap();
        expected.keybinds_layers.config_dir = Some(tmp.path().to_path_buf());
        assert_eq!(result.unwrap(), expected);
    }

    #[test]
    fn try_from_cli_args_default() {
        let opts = CliArgs::default();
        let result = Config::try_from(&opts);
        assert_eq!(result.unwrap(), Config::from_default_assets().unwrap());
    }

    #[test]
    fn can_define_options_in_configfile() {
        let config_contents = r#"
            simplified_ui true
            theme "my cool theme"
            default_mode "locked"
            default_shell "/path/to/my/shell"
            default_cwd "/path"
            default_layout "/path/to/my/layout.kdl"
            layout_dir "/path/to/my/layout-dir"
            theme_dir "/path/to/my/theme-dir"
            mouse_mode false
            pane_frames false
            mirror_session true
            on_force_close "quit"
            scroll_buffer_size 100000
            copy_command "/path/to/my/copy-command"
            copy_clipboard "primary"
            copy_on_select false
            scrollback_editor "/path/to/my/scrollback-editor"
            session_name "my awesome session"
            attach_to_session true
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        assert_eq!(
            config.options.simplified_ui,
            Some(true),
            "Option set in config"
        );
        assert_eq!(
            config.options.theme,
            Some(String::from("my cool theme")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_mode,
            Some(InputMode::Locked),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_shell,
            Some(PathBuf::from("/path/to/my/shell")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_cwd,
            Some(PathBuf::from("/path")),
            "Option set in config"
        );
        assert_eq!(
            config.options.default_layout,
            Some(PathBuf::from("/path/to/my/layout.kdl")),
            "Option set in config"
        );
        assert_eq!(
            config.options.layout_dir,
            Some(PathBuf::from("/path/to/my/layout-dir")),
            "Option set in config"
        );
        assert_eq!(
            config.options.theme_dir,
            Some(PathBuf::from("/path/to/my/theme-dir")),
            "Option set in config"
        );
        assert_eq!(
            config.options.mouse_mode,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.pane_frames,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.mirror_session,
            Some(true),
            "Option set in config"
        );
        assert_eq!(
            config.options.on_force_close,
            Some(OnForceClose::Quit),
            "Option set in config"
        );
        assert_eq!(
            config.options.scroll_buffer_size,
            Some(100000),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_command,
            Some(String::from("/path/to/my/copy-command")),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_clipboard,
            Some(Clipboard::Primary),
            "Option set in config"
        );
        assert_eq!(
            config.options.copy_on_select,
            Some(false),
            "Option set in config"
        );
        assert_eq!(
            config.options.scrollback_editor,
            Some(PathBuf::from("/path/to/my/scrollback-editor")),
            "Option set in config"
        );
        assert_eq!(
            config.options.session_name,
            Some(String::from("my awesome session")),
            "Option set in config"
        );
        assert_eq!(
            config.options.attach_to_session,
            Some(true),
            "Option set in config"
        );
    }

    #[test]
    fn can_define_themes_in_configfile() {
        let config_contents = r#"
            themes {
                dracula {
                    fg 248 248 242
                    bg 40 42 54
                    red 255 85 85
                    green 80 250 123
                    yellow 241 250 140
                    blue 98 114 164
                    magenta 255 121 198
                    orange 255 184 108
                    cyan 139 233 253
                    black 0 0 0
                    white 255 255 255
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "dracula".into(),
            Theme {
                palette: Palette {
                    fg: PaletteColor::Rgb((248, 248, 242)),
                    bg: PaletteColor::Rgb((40, 42, 54)),
                    red: PaletteColor::Rgb((255, 85, 85)),
                    green: PaletteColor::Rgb((80, 250, 123)),
                    yellow: PaletteColor::Rgb((241, 250, 140)),
                    blue: PaletteColor::Rgb((98, 114, 164)),
                    magenta: PaletteColor::Rgb((255, 121, 198)),
                    orange: PaletteColor::Rgb((255, 184, 108)),
                    cyan: PaletteColor::Rgb((139, 233, 253)),
                    black: PaletteColor::Rgb((0, 0, 0)),
                    white: PaletteColor::Rgb((255, 255, 255)),
                    ..Default::default()
                }
                .into(),
                sourced_from_external_file: false,
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
    }

    #[test]
    fn can_define_multiple_themes_including_hex_themes_in_configfile() {
        let config_contents = r##"
            themes {
                dracula {
                    fg 248 248 242
                    bg 40 42 54
                    red 255 85 85
                    green 80 250 123
                    yellow 241 250 140
                    blue 98 114 164
                    magenta 255 121 198
                    orange 255 184 108
                    cyan 139 233 253
                    black 0 0 0
                    white 255 255 255
                }
                nord {
                    fg "#D8DEE9"
                    bg "#2E3440"
                    black "#3B4252"
                    red "#BF616A"
                    green "#A3BE8C"
                    yellow "#EBCB8B"
                    blue "#81A1C1"
                    magenta "#B48EAD"
                    cyan "#88C0D0"
                    white "#E5E9F0"
                    orange "#D08770"
                }
            }
        "##;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "dracula".into(),
            Theme {
                palette: Palette {
                    fg: PaletteColor::Rgb((248, 248, 242)),
                    bg: PaletteColor::Rgb((40, 42, 54)),
                    red: PaletteColor::Rgb((255, 85, 85)),
                    green: PaletteColor::Rgb((80, 250, 123)),
                    yellow: PaletteColor::Rgb((241, 250, 140)),
                    blue: PaletteColor::Rgb((98, 114, 164)),
                    magenta: PaletteColor::Rgb((255, 121, 198)),
                    orange: PaletteColor::Rgb((255, 184, 108)),
                    cyan: PaletteColor::Rgb((139, 233, 253)),
                    black: PaletteColor::Rgb((0, 0, 0)),
                    white: PaletteColor::Rgb((255, 255, 255)),
                    ..Default::default()
                }
                .into(),
                sourced_from_external_file: false,
            },
        );
        expected_themes.insert(
            "nord".into(),
            Theme {
                palette: Palette {
                    fg: PaletteColor::Rgb((216, 222, 233)),
                    bg: PaletteColor::Rgb((46, 52, 64)),
                    black: PaletteColor::Rgb((59, 66, 82)),
                    red: PaletteColor::Rgb((191, 97, 106)),
                    green: PaletteColor::Rgb((163, 190, 140)),
                    yellow: PaletteColor::Rgb((235, 203, 139)),
                    blue: PaletteColor::Rgb((129, 161, 193)),
                    magenta: PaletteColor::Rgb((180, 142, 173)),
                    cyan: PaletteColor::Rgb((136, 192, 208)),
                    white: PaletteColor::Rgb((229, 233, 240)),
                    orange: PaletteColor::Rgb((208, 135, 112)),
                    ..Default::default()
                }
                .into(),
                sourced_from_external_file: false,
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
    }

    #[test]
    fn can_define_eight_bit_themes() {
        let config_contents = r#"
            themes {
                eight_bit_theme {
                    fg 248
                    bg 40
                    red 255
                    green 80
                    yellow 241
                    blue 98
                    magenta 255
                    orange 255
                    cyan 139
                    black 1
                    white 255
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "eight_bit_theme".into(),
            Theme {
                palette: Palette {
                    fg: PaletteColor::EightBit(248),
                    bg: PaletteColor::EightBit(40),
                    red: PaletteColor::EightBit(255),
                    green: PaletteColor::EightBit(80),
                    yellow: PaletteColor::EightBit(241),
                    blue: PaletteColor::EightBit(98),
                    magenta: PaletteColor::EightBit(255),
                    orange: PaletteColor::EightBit(255),
                    cyan: PaletteColor::EightBit(139),
                    black: PaletteColor::EightBit(1),
                    white: PaletteColor::EightBit(255),
                    ..Default::default()
                }
                .into(),
                sourced_from_external_file: false,
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config");
    }

    #[test]
    fn can_define_style_for_theme_with_hex() {
        let config_contents = r##"
            themes {
                named_theme {
                    text_unselected {
                        base "#DCD7BA"
                        emphasis_0 "#DCD7CD"
                        emphasis_1 "#DCD8DD"
                        emphasis_2 "#DCD899"
                        emphasis_3 "#ACD7CD"
                        background   "#1F1F28"
                    }
                    text_selected {
                        base "#16161D"
                        emphasis_0 "#16161D"
                        emphasis_1 "#16161D"
                        emphasis_2 "#16161D"
                        emphasis_3 "#16161D"
                        background   "#9CABCA"
                    }
                    ribbon_unselected {
                        base "#DCD7BA"
                        emphasis_0 "#7FB4CA"
                        emphasis_1 "#A3D4D5"
                        emphasis_2 "#7AA89F"
                        emphasis_3 "#DCD819"
                        background   "#252535"
                    }
                    ribbon_selected {
                        base "#16161D"
                        emphasis_0 "#181820"
                        emphasis_1 "#1A1A22"
                        emphasis_2 "#2A2A37"
                        emphasis_3 "#363646"
                        background   "#76946A"
                    }
                    table_title {
                        base "#DCD7BA"
                        emphasis_0 "#7FB4CA"
                        emphasis_1 "#A3D4D5"
                        emphasis_2 "#7AA89F"
                        emphasis_3 "#DCD819"
                        background   "#252535"
                    }
                    table_cell_unselected {
                        base "#DCD7BA"
                        emphasis_0 "#DCD7CD"
                        emphasis_1 "#DCD8DD"
                        emphasis_2 "#DCD899"
                        emphasis_3 "#ACD7CD"
                        background   "#1F1F28"
                    }
                    table_cell_selected {
                        base "#16161D"
                        emphasis_0 "#181820"
                        emphasis_1 "#1A1A22"
                        emphasis_2 "#2A2A37"
                        emphasis_3 "#363646"
                        background   "#76946A"
                    }
                    list_unselected {
                        base "#DCD7BA"
                        emphasis_0 "#DCD7CD"
                        emphasis_1 "#DCD8DD"
                        emphasis_2 "#DCD899"
                        emphasis_3 "#ACD7CD"
                        background   "#1F1F28"
                    }
                    list_selected {
                        base "#16161D"
                        emphasis_0 "#181820"
                        emphasis_1 "#1A1A22"
                        emphasis_2 "#2A2A37"
                        emphasis_3 "#363646"
                        background   "#76946A"
                    }
                    frame_unselected {
                        base "#DCD8DD"
                        emphasis_0 "#7FB4CA"
                        emphasis_1 "#A3D4D5"
                        emphasis_2 "#7AA89F"
                        emphasis_3 "#DCD819"
                    }
                    frame_selected {
                        base "#76946A"
                        emphasis_0 "#C34043"
                        emphasis_1 "#C8C093"
                        emphasis_2 "#ACD7CD"
                        emphasis_3 "#DCD819"
                    }
                    exit_code_success {
                        base "#76946A"
                        emphasis_0 "#76946A"
                        emphasis_1 "#76946A"
                        emphasis_2 "#76946A"
                        emphasis_3 "#76946A"
                    }
                    exit_code_error {
                        base "#C34043"
                        emphasis_0 "#C34043"
                        emphasis_1 "#C34043"
                        emphasis_2 "#C34043"
                        emphasis_3 "#C34043"
                    }
                }
            }
            "##;

        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "named_theme".into(),
            Theme {
                sourced_from_external_file: false,
                palette: Styling {
                    text_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_1: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_3: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    text_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_0: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_1: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_2: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_3: PaletteColor::Rgb((22, 22, 29)),
                        background: PaletteColor::Rgb((156, 171, 202)),
                    },
                    ribbon_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_1: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_2: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 25)),
                        background: PaletteColor::Rgb((37, 37, 53)),
                    },
                    ribbon_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_0: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_1: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_2: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_3: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    table_title: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_1: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_2: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 25)),
                        background: PaletteColor::Rgb((37, 37, 53)),
                    },
                    table_cell_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_1: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_3: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    table_cell_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_0: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_1: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_2: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_3: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    list_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_1: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_3: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    list_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((22, 22, 29)),
                        emphasis_0: PaletteColor::Rgb((24, 24, 32)),
                        emphasis_1: PaletteColor::Rgb((26, 26, 34)),
                        emphasis_2: PaletteColor::Rgb((42, 42, 55)),
                        emphasis_3: PaletteColor::Rgb((54, 54, 70)),
                        background: PaletteColor::Rgb((118, 148, 106)),
                    },
                    frame_unselected: Some(StyleDeclaration {
                        base: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_0: PaletteColor::Rgb((127, 180, 202)),
                        emphasis_1: PaletteColor::Rgb((163, 212, 213)),
                        emphasis_2: PaletteColor::Rgb((122, 168, 159)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 25)),
                        ..Default::default()
                    }),
                    frame_selected: StyleDeclaration {
                        base: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_0: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_1: PaletteColor::Rgb((200, 192, 147)),
                        emphasis_2: PaletteColor::Rgb((172, 215, 205)),
                        emphasis_3: PaletteColor::Rgb((220, 216, 25)),
                        ..Default::default()
                    },
                    exit_code_success: StyleDeclaration {
                        base: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_0: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_1: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_2: PaletteColor::Rgb((118, 148, 106)),
                        emphasis_3: PaletteColor::Rgb((118, 148, 106)),
                        ..Default::default()
                    },
                    exit_code_error: StyleDeclaration {
                        base: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_0: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_1: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_2: PaletteColor::Rgb((195, 64, 67)),
                        emphasis_3: PaletteColor::Rgb((195, 64, 67)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config")
    }

    #[test]
    fn omitting_required_style_errors() {
        let config_contents = r##"
            themes {
                named_theme {
                    text_unselected {
                        base "#DCD7BA"
                        emphasis_1 "#DCD8DD"
                        emphasis_2 "#DCD899"
                        emphasis_3 "#ACD7CD"
                        background   "#1F1F28"
                    }
                }
            }
            "##;

        let config = Config::from_kdl(config_contents, None);
        assert!(config.is_err());
        if let Err(ConfigError::KdlError(KdlError {
            error_message,
            src: _,
            offset: _,
            len: _,
            help_message: _,
        })) = config
        {
            assert_eq!(error_message, "Missing theme color: emphasis_0")
        }
    }

    #[test]
    fn partial_declaration_of_styles_defaults_omitted() {
        let config_contents = r##"
            themes {
                named_theme {
                    text_unselected {
                        base "#DCD7BA"
                        emphasis_0 "#DCD7CD"
                        emphasis_1 "#DCD8DD"
                        emphasis_2 "#DCD899"
                        emphasis_3 "#ACD7CD"
                        background   "#1F1F28"
                    }
                }
            }
            "##;

        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_themes = HashMap::new();
        expected_themes.insert(
            "named_theme".into(),
            Theme {
                sourced_from_external_file: false,
                palette: Styling {
                    text_unselected: StyleDeclaration {
                        base: PaletteColor::Rgb((220, 215, 186)),
                        emphasis_0: PaletteColor::Rgb((220, 215, 205)),
                        emphasis_1: PaletteColor::Rgb((220, 216, 221)),
                        emphasis_2: PaletteColor::Rgb((220, 216, 153)),
                        emphasis_3: PaletteColor::Rgb((172, 215, 205)),
                        background: PaletteColor::Rgb((31, 31, 40)),
                    },
                    ..Default::default()
                },
            },
        );
        let expected_themes = Themes::from_data(expected_themes);
        assert_eq!(config.themes, expected_themes, "Theme defined in config")
    }

    #[test]
    fn can_define_plugin_configuration_in_configfile() {
        let config_contents = r#"
            plugins {
                tab-bar location="zellij:tab-bar"
                status-bar location="zellij:status-bar"
                strider location="zellij:strider"
                compact-bar location="zellij:compact-bar"
                session-manager location="zellij:session-manager"
                welcome-screen location="zellij:session-manager" {
                    welcome_screen true
                }
                filepicker location="zellij:strider"
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_plugin_configuration = BTreeMap::new();
        expected_plugin_configuration.insert(
            "tab-bar".to_owned(),
            RunPlugin::from_url("zellij:tab-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "status-bar".to_owned(),
            RunPlugin::from_url("zellij:status-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "strider".to_owned(),
            RunPlugin::from_url("zellij:strider").unwrap(),
        );
        expected_plugin_configuration.insert(
            "compact-bar".to_owned(),
            RunPlugin::from_url("zellij:compact-bar").unwrap(),
        );
        expected_plugin_configuration.insert(
            "session-manager".to_owned(),
            RunPlugin::from_url("zellij:session-manager").unwrap(),
        );
        let mut welcome_screen_configuration = BTreeMap::new();
        welcome_screen_configuration.insert("welcome_screen".to_owned(), "true".to_owned());
        expected_plugin_configuration.insert(
            "welcome-screen".to_owned(),
            RunPlugin::from_url("zellij:session-manager")
                .unwrap()
                .with_configuration(welcome_screen_configuration),
        );
        expected_plugin_configuration.insert(
            "filepicker".to_owned(),
            RunPlugin::from_url("zellij:strider").unwrap(),
        );
        assert_eq!(
            config.plugins,
            PluginAliases::from_data(expected_plugin_configuration),
            "Plugins defined in config"
        );
    }

    #[test]
    fn can_define_ui_configuration_in_configfile() {
        let config_contents = r#"
            ui {
                pane_frames {
                    rounded_corners true
                    hide_session_name true
                }
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let expected_ui_config = UiConfig {
            pane_frames: FrameConfig {
                rounded_corners: true,
                hide_session_name: true,
                ..Default::default()
            },
        };
        assert_eq!(config.ui, expected_ui_config, "Ui config defined in config");
    }

    #[test]
    fn can_define_env_variables_in_config_file() {
        let config_contents = r#"
            env {
                RUST_BACKTRACE 1
                SOME_OTHER_VAR "foo"
            }
        "#;
        let config = Config::from_kdl(config_contents, None).unwrap();
        let mut expected_env_config = HashMap::new();
        expected_env_config.insert("RUST_BACKTRACE".into(), "1".into());
        expected_env_config.insert("SOME_OTHER_VAR".into(), "foo".into());
        assert_eq!(
            config.env,
            EnvironmentVariables::from_data(expected_env_config),
            "Env variables defined in config"
        );
    }
}
