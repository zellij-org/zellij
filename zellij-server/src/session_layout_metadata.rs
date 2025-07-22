use crate::panes::PaneId;
use crate::ClientId;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use zellij_utils::common_path::common_path_all;
use zellij_utils::pane_size::PaneGeom;
use zellij_utils::{
    input::command::RunCommand,
    input::layout::{Layout, Run, RunPlugin, RunPluginOrAlias},
    input::plugins::PluginAliases,
    session_serialization::{
        extract_command_and_args, extract_edit_and_line_number, extract_plugin_and_config,
        GlobalLayoutManifest, PaneLayoutManifest, TabLayoutManifest,
    },
};

#[derive(Default, Debug, Clone)]
pub struct SessionLayoutMetadata {
    default_layout: Box<Layout>,
    global_cwd: Option<PathBuf>,
    pub default_shell: Option<PathBuf>,
    pub default_editor: Option<PathBuf>,
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
    pub fn list_clients_metadata(&self) -> String {
        let mut clients_metadata: BTreeMap<ClientId, ClientMetadata> = BTreeMap::new();
        for tab in &self.tabs {
            let panes = if tab.hide_floating_panes {
                &tab.tiled_panes
            } else {
                &tab.floating_panes
            };
            for pane in panes {
                for focused_client in &pane.focused_clients {
                    clients_metadata.insert(
                        *focused_client,
                        ClientMetadata {
                            pane_id: pane.id.clone(),
                            command: pane.run.clone(),
                        },
                    );
                }
            }
        }

        ClientMetadata::render_many(clients_metadata, &self.default_editor)
    }
    pub fn all_clients_metadata(&self) -> BTreeMap<ClientId, ClientMetadata> {
        let mut clients_metadata: BTreeMap<ClientId, ClientMetadata> = BTreeMap::new();
        for tab in &self.tabs {
            let panes = if tab.hide_floating_panes {
                &tab.tiled_panes
            } else {
                &tab.floating_panes
            };
            for pane in panes {
                for focused_client in &pane.focused_clients {
                    clients_metadata.insert(
                        *focused_client,
                        ClientMetadata {
                            pane_id: pane.id.clone(),
                            command: pane.run.clone(),
                        },
                    );
                }
            }
        }
        clients_metadata
    }
    pub fn is_dirty(&self) -> bool {
        // here we check to see if the serialized layout would be different than the base one, and
        // thus is "dirty". A layout is considered dirty if one of the following is true:
        // 1. The current number of panes is different than the number of panes in the base layout
        //    (meaning a pane was opened or closed)
        // 2. One or more terminal panes are running a command that is not the default shell
        let base_layout_pane_count = self.default_layout.pane_count();
        let current_pane_count = self.pane_count();
        if current_pane_count != base_layout_pane_count {
            return true;
        }
        for tab in &self.tabs {
            for tiled_pane in &tab.tiled_panes {
                if let Some(Run::Command(run_command)) = tiled_pane.run.as_ref() {
                    if !Self::is_default_shell(
                        self.default_shell.as_ref(),
                        &run_command.command.display().to_string(),
                        &run_command.args,
                    ) {
                        return true;
                    }
                }
            }
            for floating_pane in &tab.floating_panes {
                if let Some(Run::Command(run_command)) = floating_pane.run.as_ref() {
                    if !Self::is_default_shell(
                        self.default_shell.as_ref(),
                        &run_command.command.display().to_string(),
                        &run_command.args,
                    ) {
                        return true;
                    }
                }
            }
        }
        false
    }
    fn pane_count(&self) -> usize {
        let mut pane_count = 0;
        for tab in &self.tabs {
            for tiled_pane in &tab.tiled_panes {
                if !self.should_exclude_from_count(tiled_pane) {
                    pane_count += 1;
                }
            }
            for floating_pane in &tab.floating_panes {
                if !self.should_exclude_from_count(floating_pane) {
                    pane_count += 1;
                }
            }
        }
        pane_count
    }
    fn should_exclude_from_count(&self, pane: &PaneLayoutMetadata) -> bool {
        if let Some(Run::Plugin(run_plugin)) = &pane.run {
            let location_string = run_plugin.location_string();
            if location_string == "zellij:about" {
                return true;
            }
            if location_string == "zellij:session-manager" {
                return true;
            }
            if location_string == "zellij:plugin-manager" {
                return true;
            }
            if location_string == "zellij:configuration-manager" {
                return true;
            }
            if location_string == "zellij:share" {
                return true;
            }
        }
        false
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
                    pane_layout_metadata.run =
                        Some(Run::Plugin(RunPluginOrAlias::RunPlugin(run_plugin)));
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
    pub fn update_default_editor(&mut self, default_editor: &Option<PathBuf>) {
        let default_editor = default_editor.clone().unwrap_or_else(|| {
            PathBuf::from(
                std::env::var("EDITOR")
                    .unwrap_or_else(|_| std::env::var("VISUAL").unwrap_or_else(|_| "vi".into())),
            )
        });
        self.default_editor = Some(default_editor);
    }
    pub fn update_plugin_aliases_in_default_layout(&mut self, plugin_aliases: &PluginAliases) {
        self.default_layout
            .populate_plugin_aliases_in_layout(&plugin_aliases);
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
    focused_clients: Vec<ClientId>,
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
        focused_clients: Vec<ClientId>,
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
            focused_clients,
        }
    }
}

pub struct ClientMetadata {
    pane_id: PaneId,
    command: Option<Run>,
}
impl ClientMetadata {
    pub fn stringify_pane_id(&self) -> String {
        match self.pane_id {
            PaneId::Terminal(terminal_id) => format!("terminal_{}", terminal_id),
            PaneId::Plugin(plugin_id) => format!("plugin_{}", plugin_id),
        }
    }
    pub fn stringify_command(&self, editor: &Option<PathBuf>) -> String {
        let stringified = match &self.command {
            Some(Run::Command(..)) => {
                let (command, args) = extract_command_and_args(&self.command);
                command.map(|c| format!("{} {}", c, args.join(" ")))
            },
            Some(Run::EditFile(..)) => {
                let (file_to_edit, _line_number) = extract_edit_and_line_number(&self.command);
                editor.as_ref().and_then(|editor| {
                    file_to_edit
                        .map(|file_to_edit| format!("{} {}", editor.display(), file_to_edit))
                })
            },
            Some(Run::Plugin(..)) => {
                let (plugin, _plugin_config) = extract_plugin_and_config(&self.command);
                plugin.map(|p| format!("{}", p))
            },
            _ => None,
        };
        stringified.unwrap_or("N/A".to_owned())
    }
    pub fn get_pane_id(&self) -> PaneId {
        self.pane_id
    }
    pub fn render_many(
        clients_metadata: BTreeMap<ClientId, ClientMetadata>,
        default_editor: &Option<PathBuf>,
    ) -> String {
        let mut lines = vec![];
        lines.push(String::from("CLIENT_ID ZELLIJ_PANE_ID RUNNING_COMMAND"));

        for (client_id, client_metadata) in clients_metadata.iter() {
            // 9 - CLIENT_ID, 14 - ZELLIJ_PANE_ID, 15 - RUNNING_COMMAND
            lines.push(format!(
                "{} {} {}",
                format!("{0: <9}", client_id),
                format!("{0: <14}", client_metadata.stringify_pane_id()),
                format!(
                    "{0: <15}",
                    client_metadata.stringify_command(default_editor)
                )
            ));
        }
        lines.join("\n")
    }
}
