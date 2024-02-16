use crate::panes::PaneId;
use std::collections::HashMap;
use std::path::PathBuf;
use zellij_utils::common_path::common_path_all;
use zellij_utils::pane_size::PaneGeom;
use zellij_utils::{
    input::command::RunCommand,
    input::layout::{Layout, Run, RunPlugin, RunPluginOrAlias},
    session_serialization::{GlobalLayoutManifest, PaneLayoutManifest, TabLayoutManifest},
};

#[derive(Default, Debug, Clone)]
pub struct SessionLayoutMetadata {
    default_layout: Box<Layout>,
    global_cwd: Option<PathBuf>,
    pub default_shell: Option<PathBuf>,
    tabs: Vec<TabLayoutMetadata>,
}

impl SessionLayoutMetadata {
    pub fn new(default_layout: Box<Layout>) -> Self {
        SessionLayoutMetadata {
            default_layout,
            ..Default::default()
        }
    }
    pub fn update_default_shell(&mut self, default_shell: PathBuf) {
        if self.default_shell.is_none() {
            self.default_shell = Some(default_shell);
        }
        for tab in self.tabs.iter_mut() {
            for tiled_pane in tab.tiled_panes.iter_mut() {
                if let Some(Run::Command(run_command)) = tiled_pane.run.as_mut() {
                    if Self::is_default_shell(
                        self.default_shell.as_ref(),
                        &run_command.command.display().to_string(),
                        &run_command.args,
                    ) {
                        tiled_pane.run = None;
                    }
                }
            }
            for floating_pane in tab.floating_panes.iter_mut() {
                if let Some(Run::Command(run_command)) = floating_pane.run.as_mut() {
                    if Self::is_default_shell(
                        self.default_shell.as_ref(),
                        &run_command.command.display().to_string(),
                        &run_command.args,
                    ) {
                        floating_pane.run = None;
                    }
                }
            }
        }
    }
    fn is_default_shell(
        default_shell: Option<&PathBuf>,
        command_name: &String,
        args: &Vec<String>,
    ) -> bool {
        default_shell
            .as_ref()
            .map(|c| c.display().to_string())
            .as_ref()
            == Some(command_name)
            && args.is_empty()
    }
}

impl SessionLayoutMetadata {
    pub fn add_tab(
        &mut self,
        name: String,
        is_focused: bool,
        hide_floating_panes: bool,
        tiled_panes: Vec<PaneLayoutMetadata>,
        floating_panes: Vec<PaneLayoutMetadata>,
    ) {
        self.tabs.push(TabLayoutMetadata {
            name: Some(name),
            is_focused,
            hide_floating_panes,
            tiled_panes,
            floating_panes,
        })
    }
    pub fn all_terminal_ids(&self) -> Vec<u32> {
        let mut terminal_ids = vec![];
        for tab in &self.tabs {
            for pane_layout_metadata in &tab.tiled_panes {
                if let PaneId::Terminal(id) = pane_layout_metadata.id {
                    terminal_ids.push(id);
                }
            }
            for pane_layout_metadata in &tab.floating_panes {
                if let PaneId::Terminal(id) = pane_layout_metadata.id {
                    terminal_ids.push(id);
                }
            }
        }
        terminal_ids
    }
    pub fn all_plugin_ids(&self) -> Vec<u32> {
        let mut plugin_ids = vec![];
        for tab in &self.tabs {
            for pane_layout_metadata in &tab.tiled_panes {
                if let PaneId::Plugin(id) = pane_layout_metadata.id {
                    plugin_ids.push(id);
                }
            }
            for pane_layout_metadata in &tab.floating_panes {
                if let PaneId::Plugin(id) = pane_layout_metadata.id {
                    plugin_ids.push(id);
                }
            }
        }
        plugin_ids
    }
    pub fn update_terminal_commands(
        &mut self,
        mut terminal_ids_to_commands: HashMap<u32, Vec<String>>,
    ) {
        let mut update_cmd_in_pane_metadata = |pane_layout_metadata: &mut PaneLayoutMetadata| {
            if let PaneId::Terminal(id) = pane_layout_metadata.id {
                if let Some(command) = terminal_ids_to_commands.remove(&id) {
                    let mut command_line = command.iter();
                    if let Some(command_name) = command_line.next() {
                        let args: Vec<String> = command_line.map(|c| c.to_owned()).collect();
                        if Self::is_default_shell(self.default_shell.as_ref(), &command_name, &args)
                        {
                            pane_layout_metadata.run = None;
                        } else {
                            let mut run_command = RunCommand::new(PathBuf::from(command_name));
                            run_command.args = args;
                            pane_layout_metadata.run = Some(Run::Command(run_command));
                        }
                    }
                }
            }
        };
        for tab in self.tabs.iter_mut() {
            for pane_layout_metadata in tab.tiled_panes.iter_mut() {
                update_cmd_in_pane_metadata(pane_layout_metadata);
            }
            for pane_layout_metadata in tab.floating_panes.iter_mut() {
                update_cmd_in_pane_metadata(pane_layout_metadata);
            }
        }
    }
    pub fn update_terminal_cwds(&mut self, mut terminal_ids_to_cwds: HashMap<u32, PathBuf>) {
        if let Some(common_path_between_cwds) =
            common_path_all(terminal_ids_to_cwds.values().map(|p| p.as_path()))
        {
            terminal_ids_to_cwds.values_mut().for_each(|p| {
                if let Ok(stripped) = p.strip_prefix(&common_path_between_cwds) {
                    *p = PathBuf::from(stripped)
                }
            });
            self.global_cwd = Some(PathBuf::from(common_path_between_cwds));
        }
        let mut update_cwd_in_pane_metadata = |pane_layout_metadata: &mut PaneLayoutMetadata| {
            if let PaneId::Terminal(id) = pane_layout_metadata.id {
                if let Some(cwd) = terminal_ids_to_cwds.remove(&id) {
                    pane_layout_metadata.cwd = Some(cwd);
                }
            }
        };
        for tab in self.tabs.iter_mut() {
            for pane_layout_metadata in tab.tiled_panes.iter_mut() {
                update_cwd_in_pane_metadata(pane_layout_metadata);
            }
            for pane_layout_metadata in tab.floating_panes.iter_mut() {
                update_cwd_in_pane_metadata(pane_layout_metadata);
            }
        }
    }
    pub fn update_plugin_cmds(&mut self, mut plugin_ids_to_run_plugins: HashMap<u32, RunPlugin>) {
        let mut update_cmd_in_pane_metadata = |pane_layout_metadata: &mut PaneLayoutMetadata| {
            if let PaneId::Plugin(id) = pane_layout_metadata.id {
                if let Some(run_plugin) = plugin_ids_to_run_plugins.remove(&id) {
                    // TODO: handle plugin alias
                    pane_layout_metadata.run = Some(Run::Plugin(RunPluginOrAlias::RunPlugin(run_plugin)));
                }
            }
        };
        for tab in self.tabs.iter_mut() {
            for pane_layout_metadata in tab.tiled_panes.iter_mut() {
                update_cmd_in_pane_metadata(pane_layout_metadata);
            }
            for pane_layout_metadata in tab.floating_panes.iter_mut() {
                update_cmd_in_pane_metadata(pane_layout_metadata);
            }
        }
    }
}

impl Into<GlobalLayoutManifest> for SessionLayoutMetadata {
    fn into(self) -> GlobalLayoutManifest {
        GlobalLayoutManifest {
            default_layout: self.default_layout,
            default_shell: self.default_shell,
            global_cwd: self.global_cwd,
            tabs: self
                .tabs
                .into_iter()
                .map(|t| (t.name.clone().unwrap_or_default(), t.into()))
                .collect(),
        }
    }
}

impl Into<TabLayoutManifest> for TabLayoutMetadata {
    fn into(self) -> TabLayoutManifest {
        TabLayoutManifest {
            tiled_panes: self.tiled_panes.into_iter().map(|t| t.into()).collect(),
            floating_panes: self.floating_panes.into_iter().map(|t| t.into()).collect(),
            is_focused: self.is_focused,
            hide_floating_panes: self.hide_floating_panes,
        }
    }
}

impl Into<PaneLayoutManifest> for PaneLayoutMetadata {
    fn into(self) -> PaneLayoutManifest {
        PaneLayoutManifest {
            geom: self.geom,
            run: self.run,
            cwd: self.cwd,
            is_borderless: self.is_borderless,
            title: self.title,
            is_focused: self.is_focused,
            pane_contents: self.pane_contents,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct TabLayoutMetadata {
    name: Option<String>,
    tiled_panes: Vec<PaneLayoutMetadata>,
    floating_panes: Vec<PaneLayoutMetadata>,
    is_focused: bool,
    hide_floating_panes: bool,
}

#[derive(Debug, Clone)]
pub struct PaneLayoutMetadata {
    id: PaneId,
    geom: PaneGeom,
    run: Option<Run>,
    cwd: Option<PathBuf>,
    is_borderless: bool,
    title: Option<String>,
    is_focused: bool,
    pane_contents: Option<String>,
}

impl PaneLayoutMetadata {
    pub fn new(
        id: PaneId,
        geom: PaneGeom,
        is_borderless: bool,
        run: Option<Run>,
        title: Option<String>,
        is_focused: bool,
        pane_contents: Option<String>,
    ) -> Self {
        PaneLayoutMetadata {
            id,
            geom,
            run,
            cwd: None,
            is_borderless,
            title,
            is_focused,
            pane_contents,
        }
    }
}
