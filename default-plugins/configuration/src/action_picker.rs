use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use zellij_tile::prelude::*;

use crate::page::{
    action_argument_range, action_display_text, is_click, is_move_key, is_plain, is_shift_tab,
    mode_label, print_dim, short_action_text, truncate, typed, ColumnLayout, RowLook, RowScroll,
    DIM,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    Text,
    Number,
    Choice(&'static [&'static str]),
    Bool,
    Pairs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgPlace {
    Positional,
    Rest,
    RestNumbers,
    Child,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ArgKind,
    pub place: ArgPlace,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub args: &'static [ArgSpec],
    pub menu_only: bool,
}

const fn positional(label: &'static str, kind: ArgKind, required: bool) -> ArgSpec {
    ArgSpec {
        name: "",
        label,
        kind,
        place: ArgPlace::Positional,
        required,
    }
}

const fn child(name: &'static str, label: &'static str, kind: ArgKind) -> ArgSpec {
    ArgSpec {
        name,
        label,
        kind,
        place: ArgPlace::Child,
        required: false,
    }
}

const fn config_pairs() -> ArgSpec {
    ArgSpec {
        name: "",
        label: "Plugin config (key=value, …)",
        kind: ArgKind::Pairs,
        place: ArgPlace::Child,
        required: false,
    }
}

const fn plain(name: &'static str, description: &'static str) -> ActionSpec {
    ActionSpec {
        name,
        description,
        args: &[],
        menu_only: false,
    }
}

const fn with_args(
    name: &'static str,
    description: &'static str,
    args: &'static [ArgSpec],
) -> ActionSpec {
    ActionSpec {
        name,
        description,
        args,
        menu_only: false,
    }
}

const fn menu_only(
    name: &'static str,
    description: &'static str,
    args: &'static [ArgSpec],
) -> ActionSpec {
    ActionSpec {
        name,
        description,
        args,
        menu_only: true,
    }
}

pub const MODES: &[&str] = &[
    "normal",
    "locked",
    "pane",
    "tab",
    "resize",
    "move",
    "scroll",
    "search",
    "entersearch",
    "renametab",
    "renamepane",
    "session",
    "tmux",
];
const DIRECTIONS: &[&str] = &["left", "right", "up", "down"];
const OPTIONAL_DIRECTIONS: &[&str] = &["", "left", "right", "up", "down"];
const NEW_PANE_PLACES: &[&str] = &["", "right", "down", "stacked"];
const LEFT_RIGHT: &[&str] = &["left", "right"];
const RESIZES: &[&str] = &[
    "Increase",
    "Decrease",
    "Increase left",
    "Increase right",
    "Increase up",
    "Increase down",
    "Decrease left",
    "Decrease right",
    "Decrease up",
    "Decrease down",
];
const SEARCH_DIRECTIONS: &[&str] = &["down", "up"];
const SEARCH_OPTIONS: &[&str] = &["CaseSensitivity", "WholeWord", "Wrap"];
const FRAME_STYLES: &[&str] = &["full", "titles", "none"];

pub const RAW_KDL: ActionSpec = ActionSpec {
    name: "KDL",
    description: "Any action written as KDL",
    args: &[ArgSpec {
        name: "",
        label: "KDL",
        kind: ArgKind::Text,
        place: ArgPlace::Raw,
        required: true,
    }],
    menu_only: false,
};

pub const ACTION_SPECS: &[ActionSpec] = &[
    with_args(
        "SwitchToMode",
        "Switch to an input mode",
        &[positional("Mode", ArgKind::Choice(MODES), true)],
    ),
    with_args(
        "NewPane",
        "Open a new pane",
        &[positional("Where", ArgKind::Choice(NEW_PANE_PLACES), false)],
    ),
    plain("NewFloatingPane", "Open a new floating pane"),
    with_args(
        "NewTab",
        "Open a new tab",
        &[child("name", "Tab name", ArgKind::Text)],
    ),
    plain("CloseFocus", "Close the focused pane"),
    plain("CloseTab", "Close the current tab"),
    with_args(
        "MoveFocus",
        "Move focus to a neighbouring pane",
        &[positional("Direction", ArgKind::Choice(DIRECTIONS), true)],
    ),
    with_args(
        "MoveFocusOrTab",
        "Move focus, or to the next tab at the edge",
        &[positional("Direction", ArgKind::Choice(DIRECTIONS), true)],
    ),
    with_args(
        "MovePane",
        "Move the focused pane",
        &[positional(
            "Direction",
            ArgKind::Choice(OPTIONAL_DIRECTIONS),
            false,
        )],
    ),
    plain("MovePaneBackwards", "Move the focused pane backwards"),
    with_args(
        "Resize",
        "Resize the focused pane",
        &[positional("Resize", ArgKind::Choice(RESIZES), true)],
    ),
    with_args(
        "MoveTab",
        "Move the current tab",
        &[positional("Direction", ArgKind::Choice(LEFT_RIGHT), true)],
    ),
    with_args(
        "GoToTab",
        "Go to a tab by position",
        &[positional("Tab number", ArgKind::Number, true)],
    ),
    plain("GoToNextTab", "Go to the next tab"),
    plain("GoToPreviousTab", "Go to the previous tab"),
    plain("ToggleTab", "Go back to the previously focused tab"),
    plain("FocusNextPane", "Focus the next pane"),
    plain("FocusPreviousPane", "Focus the previous pane"),
    plain("SwitchFocus", "Switch focus between panes"),
    plain(
        "ToggleFocusFullscreen",
        "Toggle fullscreen for the focused pane",
    ),
    plain("ToggleFocusNoUiFullscreen", "Fullscreen without any UI"),
    plain("TogglePaneFrames", "Show or hide pane frames"),
    plain(
        "TogglePaneEmbedOrFloating",
        "Float or embed the focused pane",
    ),
    plain("ToggleFloatingPanes", "Show or hide floating panes"),
    plain("ShowFloatingPanes", "Show floating panes"),
    plain("HideFloatingPanes", "Hide floating panes"),
    plain("TogglePanePinned", "Pin or unpin a floating pane"),
    plain("ToggleActiveSyncTab", "Send input to every pane in the tab"),
    plain("TogglePaneInGroup", "Add or remove the pane from the group"),
    plain(
        "ToggleGroupMarking",
        "Start or stop marking panes for a group",
    ),
    plain("BreakPane", "Move the pane to a new tab"),
    plain("BreakPaneRight", "Move the pane to the tab on the right"),
    plain("BreakPaneLeft", "Move the pane to the tab on the left"),
    plain("PreviousSwapLayout", "Previous swap layout"),
    plain("NextSwapLayout", "Next swap layout"),
    with_args(
        "ApplyTiledSwapLayout",
        "Apply a tiled swap layout by name",
        &[positional("Layout name", ArgKind::Text, true)],
    ),
    with_args(
        "ApplyFloatingSwapLayout",
        "Apply a floating swap layout by name",
        &[positional("Layout name", ArgKind::Text, true)],
    ),
    with_args(
        "PaneNameInput",
        "Start renaming the pane",
        &[positional("Byte", ArgKind::Number, true)],
    ),
    plain("UndoRenamePane", "Undo the pane rename"),
    with_args(
        "TabNameInput",
        "Start renaming the tab",
        &[positional("Byte", ArgKind::Number, true)],
    ),
    plain("UndoRenameTab", "Undo the tab rename"),
    plain("ScrollUp", "Scroll up one line"),
    plain("ScrollDown", "Scroll down one line"),
    plain("PageScrollUp", "Scroll up one page"),
    plain("PageScrollDown", "Scroll down one page"),
    plain("HalfPageScrollUp", "Scroll up half a page"),
    plain("HalfPageScrollDown", "Scroll down half a page"),
    plain("ScrollToTop", "Scroll to the top"),
    plain("ScrollToBottom", "Scroll to the bottom"),
    plain(
        "ScrollToPreviousPrompt",
        "Scroll to the previous shell prompt",
    ),
    plain("ScrollToNextPrompt", "Scroll to the next shell prompt"),
    plain(
        "SelectCommandAtScrollPosition",
        "Select the command output at the scroll position",
    ),
    plain(
        "CopyLastCommandOutput",
        "Copy the output of the last command",
    ),
    plain("EditScrollback", "Open the scrollback in an editor"),
    with_args(
        "SearchInput",
        "Type into the search field",
        &[positional("Byte", ArgKind::Number, true)],
    ),
    with_args(
        "Search",
        "Find the next match",
        &[positional(
            "Direction",
            ArgKind::Choice(SEARCH_DIRECTIONS),
            true,
        )],
    ),
    with_args(
        "SearchToggleOption",
        "Toggle a search option",
        &[positional("Option", ArgKind::Choice(SEARCH_OPTIONS), true)],
    ),
    plain("Copy", "Copy the selection"),
    with_args(
        "WriteChars",
        "Type text into the pane",
        &[positional("Text", ArgKind::Text, true)],
    ),
    with_args(
        "Write",
        "Write bytes into the pane",
        &[ArgSpec {
            name: "",
            label: "Bytes (numbers)",
            kind: ArgKind::Text,
            place: ArgPlace::RestNumbers,
            required: true,
        }],
    ),
    with_args(
        "DumpScreen",
        "Write the pane contents to a file",
        &[positional("File", ArgKind::Text, true)],
    ),
    with_args(
        "SetPaneFrameStyle",
        "Set the pane frame style",
        &[positional("Style", ArgKind::Choice(FRAME_STYLES), true)],
    ),
    plain("ToggleMouseMode", "Turn mouse support on or off"),
    plain("SetDarkTheme", "Use the dark theme"),
    plain("SetLightTheme", "Use the light theme"),
    plain("ToggleTheme", "Switch between dark and light themes"),
    plain("DismissInfoPopups", "Close information popups"),
    plain("ToggleSessionCard", "Show, focus or hide the running sessions card"),
    plain(
        "OpenContextMenu",
        "Open the right-click menu for the focused pane",
    ),
    with_args(
        "Run",
        "Run a command in a new pane",
        &[
            positional("Command", ArgKind::Text, true),
            ArgSpec {
                name: "",
                label: "Arguments",
                kind: ArgKind::Text,
                place: ArgPlace::Rest,
                required: false,
            },
            child("cwd", "Folder", ArgKind::Text),
            child("name", "Pane name", ArgKind::Text),
            child(
                "direction",
                "Direction",
                ArgKind::Choice(OPTIONAL_DIRECTIONS),
            ),
            child("floating", "Floating", ArgKind::Bool),
            child("in_place", "In place", ArgKind::Bool),
            child("close_on_exit", "Close on exit", ArgKind::Bool),
            child("start_suspended", "Start suspended", ArgKind::Bool),
        ],
    ),
    with_args(
        "LaunchOrFocusPlugin",
        "Open a plugin, or focus it if open",
        &[
            positional("Plugin", ArgKind::Text, true),
            child("floating", "Floating", ArgKind::Bool),
            child("move_to_focused_tab", "Move to focused tab", ArgKind::Bool),
            child("in_place", "In place", ArgKind::Bool),
            child("skip_plugin_cache", "Skip plugin cache", ArgKind::Bool),
            config_pairs(),
        ],
    ),
    with_args(
        "LaunchPlugin",
        "Open a new plugin instance",
        &[
            positional("Plugin", ArgKind::Text, true),
            child("floating", "Floating", ArgKind::Bool),
            child("in_place", "In place", ArgKind::Bool),
            child("skip_plugin_cache", "Skip plugin cache", ArgKind::Bool),
            config_pairs(),
        ],
    ),
    with_args(
        "MessagePlugin",
        "Send a message to a plugin",
        &[
            positional("Plugin", ArgKind::Text, false),
            child("name", "Message name", ArgKind::Text),
            child("payload", "Payload", ArgKind::Text),
            child("title", "Pane title", ArgKind::Text),
            child("launch_new", "Launch new", ArgKind::Bool),
            child("skip_cache", "Skip cache", ArgKind::Bool),
            child("floating", "Floating", ArgKind::Bool),
            config_pairs(),
        ],
    ),
    with_args(
        "SwitchSession",
        "Switch to another session",
        &[child("name", "Session name", ArgKind::Text)],
    ),
    plain("Detach", "Detach from the session"),
    plain("Quit", "Quit Zellij"),
    plain("FocusHostSession", "Focus the host session"),
    plain("FocusGuestSession", "Focus the guest session"),
    plain("ToggleHostFullscreen", "Toggle the host session fullscreen"),
    menu_only("CloseFocusByPaneId", "Close the clicked pane", &[]),
    menu_only(
        "ToggleFocusFullscreenByPaneId",
        "Fullscreen the clicked pane",
        &[],
    ),
    menu_only(
        "TogglePaneEmbedOrFloatingByPaneId",
        "Float or embed the clicked pane",
        &[],
    ),
    menu_only("TogglePanePinnedByPaneId", "Pin the clicked pane", &[]),
    menu_only(
        "TogglePaneInGroupByPaneId",
        "Add the clicked pane to the group",
        &[],
    ),
    menu_only("StartRenamePaneByPaneId", "Rename the clicked pane", &[]),
    menu_only("CloseTabById", "Close the clicked tab", &[]),
    menu_only("StartRenameTabByTabId", "Rename the clicked tab", &[]),
    menu_only(
        "MoveTabByTabId",
        "Move the clicked tab",
        &[positional("Direction", ArgKind::Choice(LEFT_RIGHT), true)],
    ),
];

pub fn specs_for(for_menu: bool) -> Vec<ActionSpec> {
    let mut specs: Vec<ActionSpec> = ACTION_SPECS
        .iter()
        .filter(|spec| for_menu || !spec.menu_only)
        .copied()
        .collect();
    specs.push(RAW_KDL);
    specs
}

fn spec_named(name: &str) -> Option<ActionSpec> {
    ACTION_SPECS.iter().find(|spec| spec.name == name).copied()
}

fn kdl_text_value(text: &str) -> String {
    KdlValue::String(text.to_owned()).to_string()
}

fn pairs_from_text(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut pairs = vec![];
    for part in text.split(|c| c == ',' || c == ';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part
            .split_once('=')
            .ok_or_else(|| format!("\"{}\" is not key=value", part))?;
        let key = key.trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return Err(format!("\"{}\" is not a valid config key", key));
        }
        pairs.push((key.to_owned(), value.trim().to_owned()));
    }
    Ok(pairs)
}

fn pairs_text(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn build_action(spec: &ActionSpec, values: &[String]) -> Result<String, String> {
    let mut text = spec.name.to_owned();
    let mut children: Vec<String> = vec![];
    for (arg, value) in spec.args.iter().zip(values.iter()) {
        let value = value.trim();
        if value.is_empty() || (arg.kind == ArgKind::Bool && value != "true") {
            if arg.required {
                return Err(format!("{} is required", arg.label));
            }
            continue;
        }
        match (arg.place, arg.kind) {
            (ArgPlace::Raw, _) => {
                let document: KdlDocument = value
                    .parse()
                    .map_err(|e: kdl::KdlError| format!("Not valid KDL: {}", e))?;
                if document.nodes().len() != 1 {
                    return Err("Write exactly one action".to_owned());
                }
                return Ok(value.to_owned());
            },
            (ArgPlace::Positional, ArgKind::Number) => {
                let number: i64 = value
                    .parse()
                    .map_err(|_| format!("{} must be a number", arg.label))?;
                text.push_str(&format!(" {}", number));
            },
            (ArgPlace::Positional, _) => text.push_str(&format!(" {}", kdl_text_value(value))),
            (ArgPlace::Rest, _) => {
                for word in value.split_whitespace() {
                    text.push_str(&format!(" {}", kdl_text_value(word)));
                }
            },
            (ArgPlace::RestNumbers, _) => {
                for word in value.split_whitespace() {
                    let number: u8 = word
                        .parse()
                        .map_err(|_| format!("{} must be numbers from 0 to 255", arg.label))?;
                    text.push_str(&format!(" {}", number));
                }
            },
            (ArgPlace::Child, ArgKind::Bool) => children.push(format!("{} true", arg.name)),
            (ArgPlace::Child, ArgKind::Number) => {
                let number: i64 = value
                    .parse()
                    .map_err(|_| format!("{} must be a number", arg.label))?;
                children.push(format!("{} {}", arg.name, number));
            },
            (ArgPlace::Child, ArgKind::Pairs) => {
                for (key, value) in pairs_from_text(value)? {
                    children.push(format!("{} {}", key, kdl_text_value(&value)));
                }
            },
            (ArgPlace::Child, _) => {
                children.push(format!("{} {}", arg.name, kdl_text_value(value)))
            },
        }
    }
    if !children.is_empty() {
        text.push_str(" {");
        for child in children {
            text.push_str(&format!(" {};", child));
        }
        text.push_str(" }");
    }
    text.parse::<KdlDocument>()
        .map_err(|e| format!("Not valid KDL: {}", e))?;
    Ok(text)
}

fn entry_text(entry: &KdlEntry) -> String {
    match entry.value() {
        KdlValue::String(text) | KdlValue::RawString(text) => text.clone(),
        other => other.to_string(),
    }
}

fn raw_values(text: &str) -> (ActionSpec, Vec<String>) {
    (RAW_KDL, vec![short_action_text(text)])
}

pub fn size_button(button: &mut Button, width: usize) {
    let resized = std::mem::replace(button, Button::new("")).width(width);
    *button = resized;
}

pub fn parse_action(text: &str) -> (ActionSpec, Vec<String>) {
    let Ok(document) = text.parse::<KdlDocument>() else {
        return raw_values(text);
    };
    let Some(node) = document.nodes().first() else {
        return raw_values(text);
    };
    let Some(spec) = spec_named(node.name().value()) else {
        return raw_values(text);
    };
    match values_for(&spec, node) {
        Some(values) => (spec, values),
        None => raw_values(text),
    }
}

fn values_for(spec: &ActionSpec, node: &KdlNode) -> Option<Vec<String>> {
    let mut values = vec![String::new(); spec.args.len()];
    let mut arguments: Vec<String> = node
        .entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .map(entry_text)
        .collect();
    for (index, arg) in spec.args.iter().enumerate() {
        match arg.place {
            ArgPlace::Positional => {
                if !arguments.is_empty() {
                    values[index] = arguments.remove(0);
                }
            },
            ArgPlace::Rest | ArgPlace::RestNumbers => {
                values[index] = arguments.join(" ");
                arguments.clear();
            },
            _ => {},
        }
    }
    if !arguments.is_empty() {
        return None;
    }
    let mut pairs = vec![];
    let mut named: Vec<(String, String)> = node
        .entries()
        .iter()
        .filter_map(|entry| {
            entry
                .name()
                .map(|name| (name.value().to_owned(), entry_text(entry)))
        })
        .collect();
    if let Some(children) = node.children() {
        for child_node in children.nodes() {
            named.push((
                child_node.name().value().to_owned(),
                child_node
                    .entries()
                    .first()
                    .map(entry_text)
                    .unwrap_or_default(),
            ));
        }
    }
    for (name, value) in named {
        match spec
            .args
            .iter()
            .position(|arg| arg.place == ArgPlace::Child && arg.name == name)
        {
            Some(index) => values[index] = value,
            None => pairs.push((name, value)),
        }
    }
    if !pairs.is_empty() {
        let index = spec
            .args
            .iter()
            .position(|arg| arg.kind == ArgKind::Pairs)?;
        values[index] = pairs_text(&pairs);
    }
    for (index, arg) in spec.args.iter().enumerate() {
        if let ArgKind::Choice(choices) = arg.kind {
            if !values[index].is_empty() {
                let matched = choices
                    .iter()
                    .find(|choice| choice.eq_ignore_ascii_case(&values[index]))?;
                values[index] = matched.to_string();
            }
        }
    }
    Some(values)
}

pub fn matching_specs(for_menu: bool, query: &str) -> Vec<ActionSpec> {
    let query = query.trim().to_lowercase();
    specs_for(for_menu)
        .into_iter()
        .filter(|spec| {
            query.is_empty()
                || fuzzy_match_indices(&query, &spec.name.to_lowercase()).is_some()
                || spec.description.to_lowercase().contains(&query)
        })
        .collect()
}

pub fn base_mode_switch(mode: InputMode) -> String {
    format!(
        "SwitchToMode {}",
        kdl_text_value(&format!("{:?}", mode).to_lowercase())
    )
}

pub fn ends_with_mode_switch(actions: &[String], mode: InputMode) -> bool {
    actions
        .last()
        .map(|last| {
            let (spec, values) = parse_action(last);
            spec.name == "SwitchToMode"
                && values
                    .first()
                    .map(|value| value.eq_ignore_ascii_case(&format!("{:?}", mode)))
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn toggle_mode_switch(actions: &mut Vec<String>, mode: InputMode) {
    if ends_with_mode_switch(actions, mode) {
        actions.pop();
    } else {
        actions.push(base_mode_switch(mode));
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerResponse {
    Pending,
    Done(Vec<String>),
    Cancelled,
}

const BACK_BUTTON: usize = usize::MAX - 1;
const APPLY_BUTTON: usize = usize::MAX;

enum Stage {
    List,
    Choose {
        search: TextInput,
        highlighted: usize,
    },
    Args {
        spec: ActionSpec,
        editing: Option<usize>,
        group: FocusGroup<usize>,
        error: Option<String>,
        label_width: usize,
    },
}

pub struct ActionPicker {
    pub actions: Vec<String>,
    base_mode: Option<InputMode>,
    for_menu: bool,
    selected: usize,
    stage: Stage,
    title: String,
    scroll: RowScroll,
    label_width: Option<usize>,
    button_width: Option<usize>,
    active: bool,
    returned_to_list: bool,
    add_button: Button,
    action_buttons: Vec<Button>,
    delete_buttons: Vec<Button>,
    done_button: Button,
    cancel_button: Button,
}

const ADD_ROW: &str = "+ Add action";
const DONE_ROW: &str = "Done";
const CANCEL_ROW: &str = "Cancel";
const BUTTON_GAP: usize = 2;
pub const DELETE_WIDTH: usize = 4;
const ARG_TEXT_WIDTH: usize = 28;

impl ActionPicker {
    pub fn new(
        title: impl Into<String>,
        actions: Vec<String>,
        base_mode: Option<InputMode>,
        for_menu: bool,
    ) -> Self {
        let mut picker = ActionPicker {
            actions,
            base_mode,
            for_menu,
            selected: 0,
            stage: Stage::List,
            title: title.into(),
            scroll: RowScroll::on_top(),
            label_width: None,
            button_width: None,
            active: true,
            returned_to_list: false,
            add_button: Button::new(ADD_ROW).accent_brackets().left_aligned(),
            action_buttons: vec![],
            delete_buttons: vec![],
            done_button: Button::new(DONE_ROW).accent_brackets(),
            cancel_button: Button::new(CANCEL_ROW).accent_brackets(),
        };
        if picker.actions.is_empty() {
            picker.open_chooser();
        }
        picker
    }
    pub fn set_label_width(&mut self, label_width: usize) {
        self.label_width = Some(label_width);
    }
    pub fn natural_button_width(&self) -> usize {
        let field_width = match &self.stage {
            Stage::Args { spec, .. } => spec
                .args
                .iter()
                .map(|arg| match arg.kind {
                    ArgKind::Choice(choices) => {
                        choices
                            .iter()
                            .map(|choice| choice.chars().count().max(6))
                            .max()
                            .unwrap_or(6)
                            + 6
                    },
                    ArgKind::Bool => 0,
                    _ => ARG_TEXT_WIDTH,
                })
                .max()
                .unwrap_or(0),
            _ => 0,
        };
        let list_width = self
            .actions
            .iter()
            .map(|action| action_display_text(action).chars().count())
            .chain(std::iter::once(ADD_ROW.chars().count()))
            .max()
            .unwrap_or(0)
            + 4;
        list_width.max(field_width)
    }
    pub fn wanted_height(&self) -> Option<usize> {
        match &self.stage {
            Stage::List => Some(self.actions.len().max(1) + 3),
            Stage::Args { spec, .. } => Some(spec.args.len() + 3),
            Stage::Choose { .. } => None,
        }
    }
    fn delete_action(&mut self, row: usize) {
        if row < self.actions.len() {
            self.actions.remove(row);
            self.selected = self.selected.min(self.row_count() - 1);
        }
    }
    pub fn set_button_width(&mut self, width: usize) {
        self.button_width = Some(width);
    }
    pub fn take_returned_to_list(&mut self) -> bool {
        std::mem::take(&mut self.returned_to_list)
    }
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    pub fn selected_row(&self) -> usize {
        self.selected
    }
    pub fn select_row(&mut self, row: usize) {
        self.selected = row.min(self.row_count() - 1);
        self.scroll.follow();
    }
    fn number_width(&self) -> usize {
        if self.actions.len() > 1 {
            self.actions.len().to_string().len() + 2
        } else {
            0
        }
    }
    pub fn content_column(&self) -> usize {
        match &self.stage {
            Stage::Args { label_width, .. } => *label_width,
            _ => self.label_column() + self.number_width(),
        }
    }
    pub fn action_name_width(&self) -> usize {
        match &self.stage {
            Stage::Args { spec, .. } => spec.name.chars().count(),
            _ => 0,
        }
    }
    pub fn is_editing_args(&self) -> bool {
        matches!(self.stage, Stage::Args { .. })
    }
    fn label_column(&self) -> usize {
        self.label_width
            .unwrap_or(0)
            .max(self.title.chars().count() + 2)
    }
    pub fn show_list(&mut self) {
        self.stage = Stage::List;
        self.returned_to_list = false;
    }
    pub fn is_listing(&self) -> bool {
        matches!(self.stage, Stage::List)
    }
    fn row_count(&self) -> usize {
        self.actions.len() + 3
    }
    fn open_args(&mut self, spec: ActionSpec, values: Vec<String>, editing: Option<usize>) {
        let mut group = FocusGroup::new().wrap(true);
        let label_width = (spec
            .args
            .iter()
            .map(|arg| arg.label.chars().count())
            .max()
            .unwrap_or(0)
            .min(24)
            + 2)
        .max(self.label_column() + self.number_width());
        for (index, arg) in spec.args.iter().enumerate() {
            let value = values.get(index).cloned().unwrap_or_default();
            match arg.kind {
                ArgKind::Choice(choices) => {
                    let selected = choices.iter().position(|c| *c == value).unwrap_or(0);
                    let labels: Vec<String> = choices
                        .iter()
                        .map(|c| {
                            if c.is_empty() {
                                "(none)".to_owned()
                            } else if choices == MODES {
                                mode_label(c).unwrap_or(c).to_owned()
                            } else {
                                c.to_string()
                            }
                        })
                        .collect();
                    group.add(
                        index,
                        Dropdown::new(arg.label, labels)
                            .selected(selected)
                            .label_width(label_width),
                    );
                },
                ArgKind::Bool => {
                    group.add(
                        index,
                        Toggle::new(arg.label, value == "true").label_width(label_width),
                    );
                },
                _ => {
                    group.add(
                        index,
                        TextInput::new(value)
                            .label(arg.label)
                            .label_width(label_width),
                    );
                },
            }
        }
        group.add(APPLY_BUTTON, Button::new("Apply").accent_brackets());
        group.add(BACK_BUTTON, Button::new("Back").accent_brackets());
        group.focus_first();
        self.stage = Stage::Args {
            spec,
            editing,
            group,
            error: None,
            label_width,
        };
    }
    fn arg_values(spec: &ActionSpec, group: &FocusGroup<usize>) -> Vec<String> {
        spec.args
            .iter()
            .enumerate()
            .map(|(index, arg)| match arg.kind {
                ArgKind::Choice(choices) => group
                    .dropdown(&index)
                    .map(|dropdown| {
                        choices
                            .get(dropdown.selected_index())
                            .map(|c| c.to_string())
                            .unwrap_or_default()
                    })
                    .unwrap_or_default(),
                ArgKind::Bool => group
                    .toggle(&index)
                    .map(|toggle| toggle.is_on().to_string())
                    .unwrap_or_default(),
                _ => group
                    .text_input(&index)
                    .map(|input| input.get_text().to_owned())
                    .unwrap_or_default(),
            })
            .collect()
    }
    fn apply_args(&mut self) {
        let Stage::Args {
            spec,
            editing,
            group,
            error,
            ..
        } = &mut self.stage
        else {
            return;
        };
        let values = Self::arg_values(spec, group);
        match build_action(spec, &values) {
            Ok(text) => {
                match editing {
                    Some(index) if *index < self.actions.len() => self.actions[*index] = text,
                    _ => {
                        self.actions.push(text);
                        self.selected = self.actions.len() - 1;
                    },
                }
                self.stage = Stage::List;
                self.returned_to_list = true;
                self.scroll.follow();
            },
            Err(message) => *error = Some(message),
        }
    }
    fn back_from_stage(&mut self) -> PickerResponse {
        self.stage = Stage::List;
        self.returned_to_list = true;
        self.scroll.follow();
        if self.actions.is_empty() {
            PickerResponse::Cancelled
        } else {
            PickerResponse::Pending
        }
    }
    fn choose(&mut self, spec: ActionSpec) {
        if spec.args.is_empty() {
            self.actions.push(spec.name.to_owned());
            self.selected = self.actions.len() - 1;
            self.stage = Stage::List;
            self.returned_to_list = true;
            self.scroll.follow();
        } else {
            self.open_args(spec, vec![], None);
        }
    }
    fn activate_row(&mut self, row: usize) -> PickerResponse {
        if row < self.actions.len() {
            let (spec, values) = parse_action(&self.actions[row]);
            self.open_args(spec, values, Some(row));
        } else if row == self.actions.len() {
            self.open_chooser();
        } else if row == self.actions.len() + 1 {
            if self.actions.is_empty() {
                return PickerResponse::Cancelled;
            }
            return PickerResponse::Done(self.actions.clone());
        } else {
            return PickerResponse::Cancelled;
        }
        PickerResponse::Pending
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> PickerResponse {
        let for_menu = self.for_menu;
        match &mut self.stage {
            Stage::List => self.handle_list_key(key),
            Stage::Choose {
                search,
                highlighted,
            } => {
                let query = search.get_text().to_owned();
                let matches = matching_specs(for_menu, &query);
                if is_plain(key, BareKey::Esc) {
                    return self.back_from_stage();
                } else if is_plain(key, BareKey::Down) {
                    *highlighted = (*highlighted + 1).min(matches.len().saturating_sub(1));
                    self.scroll.follow();
                } else if is_plain(key, BareKey::Up) {
                    *highlighted = highlighted.saturating_sub(1);
                    self.scroll.follow();
                } else if is_plain(key, BareKey::Enter) {
                    if let Some(spec) = matches.get(*highlighted).copied() {
                        self.choose(spec);
                    }
                } else if let UiResponse::Changed(_) = search.handle_key(key) {
                    *highlighted = 0;
                    self.scroll.reset();
                }
                PickerResponse::Pending
            },
            Stage::Args { group, .. } => {
                let dropdown_open = group
                    .focused_key()
                    .and_then(|focused| group.dropdown(focused))
                    .map(|dropdown| dropdown.is_open())
                    .unwrap_or(false);
                if is_plain(key, BareKey::Esc) && !dropdown_open {
                    return self.back_from_stage();
                }
                let focused_text = group
                    .focused_key()
                    .map(|focused| group.text_input(focused).is_some())
                    .unwrap_or(false);
                if !dropdown_open && (is_plain(key, BareKey::Down) || is_plain(key, BareKey::Up)) {
                    if is_plain(key, BareKey::Down) {
                        group.focus_next();
                    } else {
                        group.focus_prev();
                    }
                    return PickerResponse::Pending;
                }
                let event = group.handle_key(key);
                self.args_event(event, focused_text)
            },
        }
    }
    fn args_event(&mut self, event: FocusEvent<usize>, submit_on_text: bool) -> PickerResponse {
        match event {
            FocusEvent::Element {
                key: BACK_BUTTON,
                response: UiResponse::Activated,
            } => return self.back_from_stage(),
            FocusEvent::Element {
                response: UiResponse::Activated,
                ..
            } => self.apply_args(),
            FocusEvent::Element {
                response: UiResponse::Submitted(_),
                ..
            } if submit_on_text => self.apply_args(),
            _ => {},
        }
        PickerResponse::Pending
    }
    fn handle_list_key(&mut self, key: &KeyWithModifier) -> PickerResponse {
        let rows = self.row_count();
        if is_plain(key, BareKey::Esc) {
            return PickerResponse::Cancelled;
        }
        if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
            self.selected = (self.selected + 1).min(rows - 1);
        } else if is_plain(key, BareKey::Up) || is_shift_tab(key) {
            self.selected = self.selected.saturating_sub(1);
        } else if is_plain(key, BareKey::Right) && self.selected == self.actions.len() + 1 {
            self.selected += 1;
        } else if is_plain(key, BareKey::Left) && self.selected == self.actions.len() + 2 {
            self.selected -= 1;
        } else if is_move_key(key, BareKey::Down) && self.selected + 1 < self.actions.len() {
            self.actions.swap(self.selected, self.selected + 1);
            self.selected += 1;
        } else if is_move_key(key, BareKey::Up)
            && self.selected > 0
            && self.selected < self.actions.len()
        {
            self.actions.swap(self.selected, self.selected - 1);
            self.selected -= 1;
        } else if is_plain(key, BareKey::Delete) && self.selected < self.actions.len() {
            self.actions.remove(self.selected);
            self.selected = self.selected.min(self.row_count() - 1);
        } else if typed(key, 'a') {
            self.open_chooser();
        } else if typed(key, 'b') {
            self.toggle_base_mode();
        } else if is_plain(key, BareKey::Enter) {
            return self.activate_row(self.selected);
        }
        self.scroll.follow();
        PickerResponse::Pending
    }
    fn toggle_base_mode(&mut self) {
        if let Some(mode) = self.base_mode {
            toggle_mode_switch(&mut self.actions, mode);
            self.selected = self.selected.min(self.row_count() - 1);
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> PickerResponse {
        let for_menu = self.for_menu;
        if matches!(self.stage, Stage::Args { .. }) {
            if let Stage::Args { group, .. } = &mut self.stage {
                let event = group.handle_mouse(mouse);
                return self.args_event(event, false);
            }
        }
        if matches!(self.stage, Stage::List) {
            let count = self.actions.len();
            let mut deleted = None;
            for (row, button) in self.delete_buttons.iter_mut().enumerate() {
                if row < count && matches!(button.handle_mouse(mouse), UiResponse::Activated) {
                    deleted = Some(row);
                }
            }
            if let Some(row) = deleted {
                self.delete_action(row);
                return PickerResponse::Pending;
            }
            let mut activated = None;
            for (row, button) in self.action_buttons.iter_mut().enumerate() {
                if row < count && matches!(button.handle_mouse(mouse), UiResponse::Activated) {
                    activated = Some(row);
                }
            }
            for (row, button) in [
                (count, &mut self.add_button),
                (count + 1, &mut self.done_button),
                (count + 2, &mut self.cancel_button),
            ] {
                if matches!(button.handle_mouse(mouse), UiResponse::Activated) {
                    activated = Some(row);
                }
            }
            if let Some(row) = activated {
                self.selected = row;
                return self.activate_row(row);
            }
        }
        if self.scroll.hover(&mouse).is_some() || self.scroll.handle_wheel(&mouse).is_some() {
            return PickerResponse::Pending;
        }
        match &mut self.stage {
            Stage::List => {
                let Some((line, column)) = is_click(&mouse) else {
                    return PickerResponse::Pending;
                };
                match self.scroll.row_at(line, column) {
                    Some(row) if row >= self.actions.len() => PickerResponse::Pending,
                    Some(row) if row == self.selected => {
                        self.selected = row;
                        self.activate_row(row)
                    },
                    Some(row) => {
                        self.selected = row;
                        PickerResponse::Pending
                    },
                    None => PickerResponse::Pending,
                }
            },
            Stage::Choose {
                search,
                highlighted,
            } => {
                search.handle_mouse(mouse);
                let Some((line, column)) = is_click(&mouse) else {
                    return PickerResponse::Pending;
                };
                let matches = matching_specs(for_menu, search.get_text());
                if let Some(row) = self.scroll.row_at(line, column) {
                    *highlighted = row;
                    if let Some(spec) = matches.get(row).copied() {
                        self.choose(spec);
                    }
                }
                PickerResponse::Pending
            },
            Stage::Args { .. } => PickerResponse::Pending,
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        match &mut self.stage {
            Stage::Args { group, .. } => group.handle_timer(),
            Stage::List => {
                let mut changed = false;
                for button in self
                    .action_buttons
                    .iter_mut()
                    .chain(self.delete_buttons.iter_mut())
                {
                    changed |= button.handle_timer();
                }
                for button in [
                    &mut self.add_button,
                    &mut self.done_button,
                    &mut self.cancel_button,
                ] {
                    changed |= button.handle_timer();
                }
                changed
            },
            _ => false,
        }
    }
    pub fn open_chooser(&mut self) {
        self.scroll.reset();
        self.stage = Stage::Choose {
            search: TextInput::empty().placeholder("type to search").focused(),
            highlighted: 0,
        };
    }
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match &self.stage {
            Stage::List => {
                let mut hints = vec![
                    ("<Enter>", "edit / choose"),
                    ("<a>", "add"),
                    ("<Del>", "delete"),
                    ("<Shift ↓↑>", "order"),
                ];
                if self.base_mode.is_some() {
                    hints.push(("<b>", "then back to base mode"));
                }
                hints.push(("<Esc>", "cancel"));
                hints
            },
            Stage::Choose { .. } => {
                vec![("<↓↑>", "choose"), ("<Enter>", "pick"), ("<Esc>", "back")]
            },
            Stage::Args { .. } => vec![
                ("<Tab/↓↑>", "field"),
                ("<Enter>", "apply"),
                ("<Esc>", "back"),
            ],
        }
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if matches!(self.stage, Stage::Choose { .. }) {
            print_text_with_coordinates(
                Text::from(truncate(&self.title, width)).color_all(0),
                x,
                y,
                None,
                None,
            );
        }
        let body_y = y + 1;
        let for_menu = self.for_menu;
        let actions = self.actions.clone();
        let selected = if self.active {
            self.selected
        } else {
            usize::MAX
        };
        let label_column = self.label_column();
        let number_width = self.number_width();
        let content_column = self.content_column();
        let natural_button_width = self
            .button_width
            .unwrap_or_else(|| self.natural_button_width());
        let title = self.title.clone();
        let active = self.active;
        match &mut self.stage {
            Stage::List => {
                let label = Text::from(truncate(&title, width));
                let label = if active && selected < actions.len() {
                    label.color_all(0)
                } else {
                    label
                };
                print_text_with_coordinates(label, x, y, None, None);
                let count = actions.len();
                let number_x = x + label_column;
                let list_x = x + content_column;
                let list_width = width.saturating_sub(content_column);
                let list_y = y;
                let bottom = y + height.saturating_sub(1);
                let list_space = bottom.saturating_sub(list_y).saturating_sub(2).max(1);
                let list_height = count.max(1).min(list_space);
                let gutter = RowScroll::fitted_width(count, list_height, 0, usize::MAX);
                let button_width = natural_button_width
                    .min(list_width.saturating_sub(gutter + DELETE_WIDTH))
                    .max(5);
                if count == 0 {
                    self.scroll.clear();
                    self.action_buttons.clear();
                    self.delete_buttons.clear();
                    print_dim("No actions yet", list_x, list_y, list_width);
                } else {
                    let selected_action = if selected < count {
                        Some(selected)
                    } else {
                        None
                    };
                    let visible = self.scroll.layout(
                        list_x,
                        list_y,
                        (button_width + DELETE_WIDTH + gutter).min(list_width),
                        list_height,
                        count,
                        selected_action,
                    );
                    self.action_buttons
                        .resize_with(count, || Button::new("").accent_brackets().left_aligned());
                    self.delete_buttons
                        .resize_with(count, || Button::new("x").width(3).accent_brackets());
                    let label_width = button_width.saturating_sub(4);
                    for (row, (button, delete)) in self
                        .action_buttons
                        .iter_mut()
                        .zip(self.delete_buttons.iter_mut())
                        .enumerate()
                    {
                        size_button(button, button_width);
                        match self
                            .scroll
                            .screen_row(row)
                            .filter(|_| visible.contains(&row))
                        {
                            Some(screen_y) => {
                                let text = action_display_text(&actions[row]);
                                let mut label = Text::from(truncate(&text, label_width));
                                if let Some(range) = action_argument_range(&text) {
                                    label = label.color_range(0, range);
                                }
                                button.set_label(label);
                                if number_width > 0 {
                                    let number = format!("{}.", row + 1);
                                    print_text_with_coordinates(
                                        Text::from(format!(
                                            "{:>width$}",
                                            number,
                                            width = number_width - 1
                                        )),
                                        number_x,
                                        screen_y,
                                        None,
                                        None,
                                    );
                                }
                                button.set_focused(row == selected);
                                button.render(list_x, screen_y);
                                delete.render(list_x + button_width + 1, screen_y);
                            },
                            None => {
                                button.clear_area();
                                delete.clear_area();
                            },
                        }
                    }
                }
                let add_y = list_y + list_height;
                size_button(&mut self.add_button, button_width);
                self.add_button.set_focused(selected == count);
                self.add_button.render(list_x, add_y);
                let buttons_y = bottom.max(add_y + 2);
                self.done_button.set_focused(selected == count + 1);
                self.cancel_button.set_focused(selected == count + 2);
                let done_width = self.done_button.natural_width();
                let buttons_width = done_width + BUTTON_GAP + self.cancel_button.natural_width();
                let buttons_x = x + width.saturating_sub(buttons_width) / 2;
                self.done_button.render(buttons_x, buttons_y);
                self.cancel_button
                    .render(buttons_x + done_width + BUTTON_GAP, buttons_y);
            },
            Stage::Choose {
                search,
                highlighted,
            } => {
                let list_x = x + content_column;
                let list_width = width.saturating_sub(content_column);
                search.set_show_cursor(true);
                search.render(list_x, y, natural_button_width.min(list_width));
                let query = search.get_text().to_owned();
                let matches = matching_specs(for_menu, &query);
                let list_height = height.saturating_sub(2);
                let visible = self.scroll.layout(
                    list_x,
                    y + 2,
                    list_width,
                    list_height,
                    matches.len(),
                    Some(*highlighted),
                );
                let rows: Vec<Vec<String>> = matches
                    .iter()
                    .map(|spec| vec![spec.name.to_owned(), spec.description.to_owned()])
                    .collect();
                let row_width = self.scroll.row_width();
                let layout = ColumnLayout::new(
                    rows.iter().map(|columns| (columns.as_slice(), "")),
                    row_width,
                )
                .with_indent(0);
                for row in visible {
                    if let Some(screen_y) = self.scroll.screen_row(row) {
                        let look = RowLook::new(row == *highlighted, self.scroll.is_hovered(row));
                        let description_length = rows[row][1].chars().count();
                        layout.print_styled(
                            &rows[row],
                            "",
                            list_x,
                            screen_y,
                            row_width,
                            look,
                            &[(1, DIM, 0..description_length)],
                        );
                    }
                }
                if matches.is_empty() {
                    print_dim("No matching action", list_x, y + 2, list_width);
                }
            },
            Stage::Args {
                spec,
                group,
                error,
                label_width,
                ..
            } => {
                self.scroll.clear();
                group.clear_areas();
                let label_width = *label_width;
                print_text_with_coordinates(Text::from("Action").color_all(0), x, y, None, None);
                print_text_with_coordinates(
                    Text::from(truncate(spec.name, width.saturating_sub(label_width))),
                    x + label_width,
                    y,
                    None,
                    None,
                );
                let field_width = (label_width + natural_button_width).min(width);
                let bottom = y + height.saturating_sub(1);
                let mut row = body_y;
                for index in 0..spec.args.len() {
                    if row + 2 > bottom {
                        break;
                    }
                    let focused = group.is_focused(&index);
                    match group.get_mut(&index) {
                        Some(Element::TextInput(input)) => {
                            input.set_show_cursor(focused);
                            input.render(x, row, field_width);
                        },
                        Some(Element::Dropdown(dropdown)) => dropdown.render(x, row, field_width),
                        Some(Element::Toggle(toggle)) => toggle.render(x, row),
                        _ => {},
                    }
                    row += 1;
                }
                if let Some(error) = error {
                    print_text_with_coordinates(
                        Text::from(truncate(error, width.saturating_sub(label_width)))
                            .error_color_all(),
                        x + label_width,
                        row,
                        None,
                        None,
                    );
                }
                let buttons_y = bottom.max(row + 1);
                let widths: Vec<usize> = [APPLY_BUTTON, BACK_BUTTON]
                    .iter()
                    .filter_map(|key| group.button(key).map(|button| button.natural_width()))
                    .collect();
                let total =
                    widths.iter().sum::<usize>() + BUTTON_GAP * widths.len().saturating_sub(1);
                let mut column = x + width.saturating_sub(total) / 2;
                for key in [APPLY_BUTTON, BACK_BUTTON] {
                    if let Some(Element::Button(button)) = group.get_mut(&key) {
                        let button_width = button.natural_width();
                        button.render(column, buttons_y);
                        column += button_width + BUTTON_GAP;
                    }
                }
            },
        }
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        if let Stage::Args { group, .. } = &mut self.stage {
            group.render_overlays(rows, cols);
            crate::page::note_group(group);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::input::options::Options;

    fn parsed(text: &str) -> ContextMenuAction {
        let document: KdlDocument = text.parse().unwrap();
        ContextMenuAction::from_kdl(document.nodes().first().unwrap(), &Options::default())
            .unwrap_or_else(|e| panic!("{} does not parse: {:?}", text, e))
    }

    fn sample_values(spec: &ActionSpec) -> Vec<String> {
        spec.args
            .iter()
            .map(|arg| match (arg.place, arg.kind) {
                (ArgPlace::Raw, _) => "NewPane \"right\"".to_owned(),
                (ArgPlace::RestNumbers, _) => "10 13".to_owned(),
                (ArgPlace::Rest, _) => "-l -a".to_owned(),
                (_, ArgKind::Number) => "2".to_owned(),
                (_, ArgKind::Choice(choices)) => {
                    choices.iter().find(|c| !c.is_empty()).unwrap().to_string()
                },
                (_, ArgKind::Bool) => {
                    if spec.name == "Run" && matches!(arg.name, "floating" | "in_place") {
                        "false".to_owned()
                    } else {
                        "true".to_owned()
                    }
                },
                (_, ArgKind::Pairs) => "role=link, size=3".to_owned(),
                (_, ArgKind::Text) => match arg.label {
                    "Plugin" => "zellij:strider".to_owned(),
                    "Folder" | "File" => "/tmp".to_owned(),
                    _ => "word".to_owned(),
                },
            })
            .collect()
    }

    #[test]
    fn every_action_form_writes_kdl_that_parses_back_to_the_same_action() {
        for spec in specs_for(true) {
            let values = sample_values(&spec);
            let text = build_action(&spec, &values)
                .unwrap_or_else(|e| panic!("{} did not build: {}", spec.name, e));
            let action = parsed(&text);
            if spec.name == "MessagePlugin" {
                continue;
            }
            let written = action
                .to_kdl()
                .unwrap_or_else(|| panic!("{} cannot be written back", text));
            let reparsed = parsed(&written.to_string());
            assert_eq!(action, reparsed, "{} changed when written back", text);
            let (read_spec, read_values) = parse_action(&written.to_string());
            if spec.name != "KDL" {
                assert_eq!(read_spec.name, spec.name, "{}", text);
                let rebuilt = build_action(&read_spec, &read_values).unwrap();
                assert_eq!(parsed(&rebuilt), action, "{} reads back differently", text);
            }
        }
    }

    #[test]
    fn a_message_plugin_action_with_a_name_round_trips() {
        let spec = spec_named("MessagePlugin").unwrap();
        let text = build_action(
            &spec,
            &[
                "zellij:strider".to_owned(),
                "msg".to_owned(),
                "hi".to_owned(),
                String::new(),
                "false".to_owned(),
                "false".to_owned(),
                "true".to_owned(),
                "a=b".to_owned(),
            ],
        )
        .unwrap();
        let action = parsed(&text);
        assert_eq!(parsed(&action.to_kdl().unwrap().to_string()), action);
    }

    #[test]
    fn a_required_argument_is_checked() {
        let spec = spec_named("SwitchToMode").unwrap();
        assert!(build_action(&spec, &[String::new()]).is_err());
        assert_eq!(
            build_action(&spec, &["locked".to_owned()]).unwrap(),
            "SwitchToMode \"locked\""
        );
    }

    #[test]
    fn unknown_actions_are_edited_as_kdl() {
        let (spec, values) = parse_action("Clear");
        assert_eq!(spec.name, "KDL");
        assert_eq!(values, vec!["Clear".to_owned()]);
    }

    #[test]
    fn the_base_mode_switch_is_added_and_removed() {
        let mut actions = vec!["NewTab".to_owned()];
        toggle_mode_switch(&mut actions, InputMode::Normal);
        assert_eq!(
            actions,
            vec!["NewTab".to_owned(), "SwitchToMode \"normal\"".to_owned()]
        );
        assert!(ends_with_mode_switch(&actions, InputMode::Normal));
        toggle_mode_switch(&mut actions, InputMode::Normal);
        assert_eq!(actions, vec!["NewTab".to_owned()]);
    }

    #[test]
    fn the_picker_adds_an_action_by_search() {
        let mut picker = ActionPicker::new("Actions", vec![], None, false);
        for character in "NewFloat".chars() {
            picker.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
        picker.handle_key(&KeyWithModifier::new(BareKey::Enter));
        assert_eq!(picker.actions, vec!["NewFloatingPane".to_owned()]);
        assert!(picker.is_listing());
        picker.handle_key(&KeyWithModifier::new(BareKey::Down));
        picker.handle_key(&KeyWithModifier::new(BareKey::Down));
        assert_eq!(
            picker.handle_key(&KeyWithModifier::new(BareKey::Enter)),
            PickerResponse::Done(vec!["NewFloatingPane".to_owned()])
        );
    }

    #[test]
    fn actions_are_numbered_only_when_there_are_several() {
        let mut picker = ActionPicker::new("Actions", vec!["NewTab".to_owned()], None, false);
        picker.set_label_width(9);
        assert_eq!(picker.content_column(), 9);
        picker.actions.push("NewPane".to_owned());
        assert_eq!(picker.content_column(), 12);
    }
}
