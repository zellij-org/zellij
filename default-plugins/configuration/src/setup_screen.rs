use std::collections::BTreeSet;

use zellij_tile::prelude::desktop_integration::{
    Flag, Item, ItemStatus, Outcome, Request, Response, Status, Upgrade, DESKTOP_ENTRY_ITEMS,
};
use zellij_tile::prelude::*;

use crate::keys_screen::KeysScreen;
use crate::ui_components::request_close;

const WIDTH: usize = 72;
const REPLACE_MARKER: &str = "Confirm to replace it.";
const BUTTON_GAP: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Choose,
    Keys,
    Applying,
    Result,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Line {
    Copy,
    Entry,
    Terminal,
}

impl Line {
    const ALL: [Line; 3] = [Line::Copy, Line::Entry, Line::Terminal];

    fn label(self) -> &'static str {
        match self {
            Line::Copy => "Copy Zellij to a Permanent Folder",
            Line::Entry => "Install Desktop Entry",
            Line::Terminal => "Make Zellij your Default Terminal",
        }
    }

    fn items(self) -> &'static [Item] {
        match self {
            Line::Copy => &[Item::CopyBinary],
            Line::Entry => &[Item::Launcher, Item::StartMenu, Item::ExplorerItem],
            Line::Terminal => &[Item::DefaultTerminal],
        }
    }

    fn ticked_by_default(self) -> bool {
        !matches!(self, Line::Terminal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    Line(Line),
    Keys,
    Info(Topic),
    Uninstall,
    Approve,
    Dismiss,
    Never,
    Replace,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Topic {
    Line(Line),
    Keys,
}

fn topic_of(row: Row) -> Option<Topic> {
    match row {
        Row::Line(line) => Some(Topic::Line(line)),
        Row::Keys => Some(Topic::Keys),
        _ => None,
    }
}

enum Block {
    Text(Text),
    Checkbox(Checkbox),
    Buttons(Vec<Button>),
}

struct Checkbox {
    ticked: bool,
    label: String,
    row: Row,
    disabled: bool,
    uninstall: bool,
}

struct Button {
    key: &'static str,
    label: String,
    row: Row,
}

impl Button {
    fn new(key: &'static str, label: impl Into<String>, row: Row) -> Self {
        Button {
            key,
            label: label.into(),
            row,
        }
    }

    fn text(&self) -> String {
        format!("{} {}", self.key, self.label)
    }

    fn width(&self) -> usize {
        zellij_tile::graphemes::display_width(&self.text())
    }
}

fn button_rows(buttons: &[Button], width: usize) -> Vec<Vec<usize>> {
    let mut rows: Vec<Vec<usize>> = vec![];
    let mut used = 0;
    for (index, button) in buttons.iter().enumerate() {
        let needed = button.width();
        match rows.last_mut() {
            Some(row) if used + BUTTON_GAP + needed <= width => {
                row.push(index);
                used += BUTTON_GAP + needed;
            },
            _ => {
                rows.push(vec![index]);
                used = needed;
            },
        }
    }
    rows
}

pub struct SetupScreen {
    step: Step,
    auto_shown: bool,
    status: Option<Status>,
    status_error: Option<String>,
    ticked: BTreeSet<Line>,
    set_up_keys: bool,
    focus: usize,
    show_focus: bool,
    focusable: Vec<Row>,
    hovered: Option<Row>,
    clickable: Vec<(usize, usize, usize, Row)>,
    keys: KeysScreen,
    keys_visited: bool,
    outcomes: Vec<Outcome>,
    waiting_for_integration: usize,
    waiting_for_config: bool,
    config_result: Option<Result<(), Option<String>>>,
    replace_offer: Option<String>,
    own_plugin_id: u32,
    uninstall_prompt: Option<u64>,
    uninstalling: bool,
    notice: Option<(String, bool)>,
    widened: bool,
}

impl SetupScreen {
    pub fn new(auto_shown: bool, own_plugin_id: u32) -> Self {
        let mut keys = KeysScreen::new(true).as_setup_step();
        keys.set_snapshot(&read_config());
        let screen = SetupScreen {
            step: Step::Choose,
            auto_shown,
            status: None,
            status_error: None,
            ticked: BTreeSet::new(),
            set_up_keys: false,
            focus: 0,
            show_focus: false,
            focusable: vec![],
            hovered: None,
            clickable: vec![],
            keys,
            keys_visited: false,
            outcomes: vec![],
            waiting_for_integration: 0,
            waiting_for_config: false,
            config_result: None,
            replace_offer: None,
            own_plugin_id,
            uninstall_prompt: None,
            uninstalling: false,
            notice: None,
            widened: false,
        };
        screen.retitle();
        request_desktop_integration(&Request::Status);
        screen
    }

    fn retitle(&self) {
        let title = match self.step {
            Step::Keys => "Welcome to Zellij: Keybindings",
            _ => "Welcome to Zellij",
        };
        rename_plugin_pane(self.own_plugin_id, title);
    }

    pub fn keys_mut(&mut self) -> &mut KeysScreen {
        &mut self.keys
    }

    fn offered(&self, line: Line) -> Vec<&ItemStatus> {
        let Some(status) = self.status.as_ref() else {
            return vec![];
        };
        line.items()
            .iter()
            .filter(|item| status.offered(**item))
            .filter_map(|item| status.item(*item))
            .collect()
    }

    fn line_statuses(&self, line: Line) -> Vec<&ItemStatus> {
        let Some(status) = self.status.as_ref() else {
            return vec![];
        };
        line.items()
            .iter()
            .filter_map(|item| status.item(*item))
            .filter(|item| item.available || item.installed)
            .collect()
    }

    fn line_done(&self, line: Line) -> bool {
        let statuses = self.line_statuses(line);
        !statuses.is_empty() && statuses.iter().all(|status| status.installed)
    }

    fn visible_lines(&self) -> Vec<Line> {
        Line::ALL
            .into_iter()
            .filter(|line| {
                if self.line_done(*line) {
                    !self.auto_shown
                } else {
                    !self.offered(*line).is_empty()
                }
            })
            .collect()
    }

    fn removable_entry_items(&self) -> Vec<Item> {
        let Some(status) = self.status.as_ref() else {
            return vec![];
        };
        DESKTOP_ENTRY_ITEMS
            .into_iter()
            .filter(|item| status.removable(*item))
            .collect()
    }

    fn can_uninstall(&self) -> bool {
        !self.auto_shown && self.line_done(Line::Entry) && !self.removable_entry_items().is_empty()
    }

    fn removed_by_uninstall(&self) -> Vec<String> {
        let Some(status) = self.status.as_ref() else {
            return vec![];
        };
        self.removable_entry_items()
            .into_iter()
            .filter_map(|item| status.item(item))
            .flat_map(|item| item.removes.clone())
            .collect()
    }

    fn upgrade(&self) -> Option<&Upgrade> {
        self.offered(Line::Copy)
            .into_iter()
            .find_map(|status| status.upgrade.as_ref())
    }

    fn line_label(&self, line: Line) -> String {
        match (line, self.upgrade()) {
            (Line::Copy, Some(upgrade)) => {
                format!("Update Zellij from {} to {}", upgrade.from, upgrade.to)
            },
            _ => line.label().to_owned(),
        }
    }

    fn ticked_by_default(&self, line: Line) -> bool {
        line.ticked_by_default()
    }

    fn copy_needed(&self) -> bool {
        self.visible_lines().contains(&Line::Copy) && !self.line_done(Line::Copy)
    }

    fn any_done(&self) -> bool {
        self.visible_lines()
            .into_iter()
            .any(|line| self.line_done(line))
    }

    pub fn desktop_integration(&mut self, payload: String) -> bool {
        match Response::from_json(&payload) {
            Response::Status(status) => {
                if self.status.is_none() {
                    let approved_before = status.state.has(Flag::SetupApproved);
                    self.status = Some(status);
                    for line in self.visible_lines() {
                        if !self.line_done(line) && !approved_before && self.ticked_by_default(line)
                        {
                            self.ticked.insert(line);
                        }
                    }
                    self.fix_dependencies();
                    self.focus = 0;
                } else {
                    self.status = Some(status);
                    self.fix_dependencies();
                }
            },
            Response::Applied(outcomes) if self.uninstalling => {
                self.uninstalling = false;
                let failed: Vec<&Outcome> = outcomes.iter().filter(|o| !o.succeeded).collect();
                self.notice = Some(match failed.first() {
                    None => ("✓ The desktop entry was uninstalled.".to_owned(), true),
                    Some(outcome) => (
                        format!(
                            "✗ The desktop entry could not be uninstalled: {}",
                            outcome.message
                        ),
                        false,
                    ),
                });
                self.ticked.remove(&Line::Entry);
                request_desktop_integration(&Request::Status);
            },
            Response::Applied(outcomes) => {
                self.waiting_for_integration = self.waiting_for_integration.saturating_sub(1);
                self.replace_offer = outcomes
                    .iter()
                    .find(|o| !o.succeeded && o.message.contains(REPLACE_MARKER))
                    .map(|o| o.message.clone());
                self.outcomes.extend(outcomes);
                self.maybe_finish_applying();
            },
            Response::Done(_) => {},
            Response::Error(error) => {
                if self.status.is_none() {
                    self.status_error = Some(error);
                } else if self.step == Step::Applying {
                    self.waiting_for_integration = self.waiting_for_integration.saturating_sub(1);
                    self.status_error = Some(error);
                    self.maybe_finish_applying();
                }
            },
        }
        true
    }

    pub fn config_written(&mut self) -> bool {
        if self.waiting_for_config {
            self.waiting_for_config = false;
            self.config_result = Some(Ok(()));
            self.maybe_finish_applying();
            return true;
        }
        false
    }

    pub fn config_write_failed(&mut self, path: Option<String>) -> bool {
        if self.waiting_for_config {
            self.waiting_for_config = false;
            self.config_result = Some(Err(path));
            self.maybe_finish_applying();
            return true;
        }
        false
    }

    fn maybe_finish_applying(&mut self) {
        if self.step != Step::Applying
            || self.waiting_for_integration > 0
            || self.waiting_for_config
        {
            return;
        }
        let config_failed = matches!(self.config_result, Some(Err(_)));
        if self.outcomes.is_empty() && self.status_error.is_none() && !config_failed {
            request_close();
            return;
        }
        self.step = Step::Result;
        self.focus = 0;
    }

    fn toggle(&mut self, line: Line) {
        if self.line_done(line) {
            return;
        }
        if self.ticked.contains(&line) {
            self.ticked.remove(&line);
        } else {
            self.ticked.insert(line);
            if line == Line::Terminal
                && self.visible_lines().contains(&Line::Entry)
                && !self.line_done(Line::Entry)
            {
                self.ticked.insert(Line::Entry);
            }
            if matches!(line, Line::Entry | Line::Terminal) && self.copy_needed() {
                self.ticked.insert(Line::Copy);
            }
        }
        self.fix_dependencies();
    }

    fn fix_dependencies(&mut self) {
        let visible = self.visible_lines();
        if self.copy_needed() && !self.ticked.contains(&Line::Copy) {
            self.ticked.remove(&Line::Entry);
            self.ticked.remove(&Line::Terminal);
        }
        if visible.contains(&Line::Entry)
            && !self.line_done(Line::Entry)
            && !self.ticked.contains(&Line::Entry)
        {
            self.ticked.remove(&Line::Terminal);
        }
        self.ticked.retain(|line| visible.contains(line));
    }

    fn to_install(&self) -> Vec<Item> {
        self.visible_lines()
            .into_iter()
            .filter(|line| self.ticked.contains(line) && !self.line_done(*line))
            .flat_map(|line| self.offered(line))
            .filter(|status| !status.installed)
            .map(|status| status.item)
            .collect()
    }

    fn ask_to_uninstall(&mut self, placement: PromptPlacement) {
        if !self.can_uninstall() {
            return;
        }
        self.hovered = None;
        let removes = self.removed_by_uninstall();
        let mut text = String::from("Uninstalling the desktop entry deletes:\n");
        for path in &removes {
            text.push('\n');
            text.push_str(path);
        }
        let mut message = Text::from(text);
        for path in &removes {
            message = message.color_substring(0, path);
        }
        let request = PromptRequest::confirm(message)
            .title("Uninstall Desktop Entry")
            .yes("Uninstall")
            .no("Cancel")
            .placement(placement);
        self.uninstall_prompt = Some(prompt(request));
    }

    pub fn prompt_result(&mut self, request_id: u64, result: PromptResult) -> bool {
        if self.uninstall_prompt != Some(request_id) {
            return false;
        }
        self.uninstall_prompt = None;
        if matches!(result, PromptResult::Confirmed(true)) {
            let remove = self.removable_entry_items();
            if !remove.is_empty() {
                self.uninstalling = true;
                request_desktop_integration(&Request::Apply {
                    install: vec![],
                    remove,
                    overwrite_binary: false,
                });
            }
        }
        true
    }

    fn approve(&mut self) {
        if self.set_up_keys {
            self.step = Step::Keys;
            self.keys_visited = true;
            self.keys.set_focused(true);
            self.retitle();
        } else {
            self.apply();
        }
    }

    fn apply(&mut self) {
        request_desktop_integration(&Request::SetFlag(Flag::SetupApproved));
        self.outcomes.clear();
        self.status_error = None;
        self.config_result = None;
        if self.keys_visited {
            self.waiting_for_config = true;
            overwrite_config_file();
        }
        let install = self.to_install();
        if !install.is_empty() {
            self.waiting_for_integration += 1;
            request_desktop_integration(&Request::Apply {
                install,
                remove: vec![],
                overwrite_binary: false,
            });
        }
        self.step = Step::Applying;
        self.retitle();
        self.maybe_finish_applying();
    }

    fn replace_binary(&mut self) {
        self.replace_offer = None;
        self.outcomes.retain(|o| o.item != Item::CopyBinary);
        self.waiting_for_integration += 1;
        let mut install = vec![Item::CopyBinary];
        if self.ticked.contains(&Line::Entry)
            && self
                .status
                .as_ref()
                .map(|s| s.offered(Item::Launcher))
                .unwrap_or(false)
        {
            install.push(Item::Launcher);
        }
        if self.ticked.contains(&Line::Terminal) && !self.line_done(Line::Terminal) {
            install.push(Item::DefaultTerminal);
        }
        self.outcomes
            .retain(|o| o.succeeded || !install.contains(&o.item));
        request_desktop_integration(&Request::Apply {
            install,
            remove: vec![],
            overwrite_binary: true,
        });
        self.step = Step::Applying;
    }

    fn dismiss(&mut self) {
        if self.auto_shown {
            request_desktop_integration(&Request::SetFlag(Flag::WizardDismissed));
        }
        request_close();
    }

    fn never_show_again(&mut self) {
        request_desktop_integration(&Request::SetFlag(Flag::SetupNeverShow));
        request_desktop_integration(&Request::SetFlag(Flag::WizardDismissed));
        request_close();
    }

    fn activate(&mut self, row: Row) {
        match row {
            Row::Line(line) => self.toggle(line),
            Row::Keys => self.set_up_keys = !self.set_up_keys,
            Row::Info(topic) => self.show_info(topic, PromptPlacement::Mouse),
            Row::Uninstall => self.ask_to_uninstall(PromptPlacement::Mouse),
            Row::Approve => self.approve(),
            Row::Dismiss => self.dismiss(),
            Row::Never => self.never_show_again(),
            Row::Replace => self.replace_binary(),
            Row::Close => request_close(),
        }
    }

    fn show_info(&mut self, topic: Topic, placement: PromptPlacement) {
        self.hovered = None;
        let mut message = Text::from(self.info_paragraphs(topic).join("\n\n"));
        for emphasized in self.info_emphasis(topic) {
            message = message.color_substring(0, emphasized);
        }
        prompt(PromptRequest::info(self.info_title(topic), message).placement(placement));
    }

    fn info_emphasis(&self, topic: Topic) -> Vec<String> {
        match topic {
            Topic::Line(Line::Copy) => match self.upgrade() {
                Some(upgrade) => vec![upgrade.path.display().to_string()],
                None => self
                    .offered(Line::Copy)
                    .first()
                    .and_then(|status| status.target.clone())
                    .map(|target| vec![target.display().to_string()])
                    .unwrap_or_default(),
            },
            Topic::Line(Line::Terminal) => self
                .offered(Line::Terminal)
                .first()
                .map(|status| status.state_text.clone())
                .filter(|text| !text.is_empty())
                .into_iter()
                .collect(),
            _ => vec![],
        }
    }

    fn info_title(&self, topic: Topic) -> String {
        match topic {
            Topic::Line(line) => self.line_label(line),
            Topic::Keys => "Keybindings".to_owned(),
        }
    }

    fn info_paragraphs(&self, topic: Topic) -> Vec<String> {
        let state_text = |line: Line| {
            self.offered(line)
                .first()
                .map(|status| status.state_text.clone())
                .filter(|text| !text.is_empty())
        };
        let mut paragraphs = vec![];
        if let Topic::Line(line) = topic {
            if self.line_done(line) {
                paragraphs.push(match line {
                    Line::Copy => {
                        "Zellij is already in a permanent folder, so it can be run from anywhere."
                            .to_owned()
                    },
                    Line::Entry => "Zellij is already in your app menu.".to_owned(),
                    Line::Terminal => "Zellij is already your default terminal.".to_owned(),
                });
                if let Some(text) = self
                    .line_statuses(line)
                    .first()
                    .map(|status| status.state_text.clone())
                    .filter(|text| !text.is_empty())
                {
                    paragraphs.push(text);
                }
                return paragraphs;
            }
        }
        match topic {
            Topic::Line(Line::Copy) => match self.upgrade() {
                Some(upgrade) => {
                    paragraphs.push(format!(
                        "Replaces the older Zellij at {} with this version. The old file is overwritten. Sessions that are already running keep using the old version until they end.",
                        upgrade.path.display()
                    ));
                },
                None => {
                    let target = self
                        .offered(Line::Copy)
                        .first()
                        .and_then(|status| status.target.clone())
                        .map(|target| target.display().to_string())
                        .unwrap_or_else(|| "a permanent folder".to_owned());
                    paragraphs.push(format!(
                        "This copy of Zellij isn't in your PATH. Checking this will copy it to {}.",
                        target
                    ));
                    paragraphs.push("This is so that it can be run from anywhere, and so that the desktop entry and the default terminal run this copy instead of the file you started.".to_owned());
                },
            },
            Topic::Line(Line::Entry) => {
                let windows = self
                    .offered(Line::Entry)
                    .iter()
                    .any(|status| status.item == Item::StartMenu);
                if windows {
                    paragraphs.push("Adds Zellij to the Start menu, and an \"Open Zellij here\" item when you right-click a folder in Explorer.".to_owned());
                } else {
                    paragraphs.push("Adds Zellij to your app menu, so it opens in its own window like any other app.".to_owned());
                }
            },
            Topic::Line(Line::Terminal) => {
                paragraphs.push("Apps that open a terminal, like a file manager's \"Open terminal here\", will open Zellij instead.".to_owned());
                paragraphs.extend(state_text(Line::Terminal));
            },
            Topic::Keys => {
                paragraphs.push("Zellij's default keys use Ctrl shortcuts, which editors and other programs might also use. Ticking this lets you select a keybinding preset that avoids this.".to_owned());
            },
        }
        paragraphs
    }

    fn move_focus(&mut self, forward: bool) {
        if self.focusable.is_empty() {
            return;
        }
        self.hovered = None;
        if !self.show_focus {
            self.show_focus = true;
            return;
        }
        let count = self.focusable.len();
        self.focus = if forward {
            (self.focus + 1) % count
        } else {
            (self.focus + count - 1) % count
        };
    }

    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.step == Step::Keys {
            self.keys.handle_key(key);
            self.after_keys_input();
            return true;
        }
        if self.step == Step::Applying {
            return false;
        }
        if key.is_key_with_ctrl_modifier(BareKey::Char('c')) {
            match self.step {
                Step::Choose if self.auto_shown => self.never_show_again(),
                Step::Choose => self.dismiss(),
                _ => request_close(),
            }
            return true;
        }
        if key.bare_key == BareKey::Tab && key.has_modifiers(&[KeyModifier::Shift]) {
            self.move_focus(false);
            return true;
        }
        if !key.has_no_modifiers() {
            return false;
        }
        match key.bare_key {
            BareKey::Down | BareKey::Tab => self.move_focus(true),
            BareKey::Up => self.move_focus(false),
            BareKey::Char(' ') if self.step == Step::Choose => {
                self.hovered = None;
                self.show_focus = true;
                if let Some(row) = self.focusable.get(self.focus).copied() {
                    self.activate(row);
                }
            },
            BareKey::Char('?') if self.step == Step::Choose => {
                self.show_focus = true;
                if let Some(topic) = self.focusable.get(self.focus).copied().and_then(topic_of) {
                    let own_pane = PaneId::Plugin(self.own_plugin_id);
                    self.show_info(topic, PromptPlacement::Pane(own_pane));
                }
            },
            BareKey::Delete if self.step == Step::Choose && self.can_uninstall() => {
                let own_pane = PaneId::Plugin(self.own_plugin_id);
                self.ask_to_uninstall(PromptPlacement::Pane(own_pane));
            },
            BareKey::Char('r') if self.step == Step::Result && self.replace_offer.is_some() => {
                self.replace_binary();
            },
            BareKey::Enter => match self.step {
                Step::Choose => self.approve(),
                _ => request_close(),
            },
            BareKey::Esc => match self.step {
                Step::Choose => self.dismiss(),
                _ => request_close(),
            },
            _ => return false,
        }
        true
    }

    fn after_keys_input(&mut self) -> bool {
        if self.keys.take_needs_refresh() {
            self.keys.set_snapshot(&read_config());
        }
        let mut changed = false;
        if self.keys.take_wants_back() {
            self.step = Step::Choose;
            self.retitle();
            changed = true;
        }
        if self.keys.take_finished() {
            self.apply();
            changed = true;
        }
        changed
    }

    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if self.step == Step::Keys {
            let handled = self.keys.handle_mouse(mouse);
            return self.after_keys_input() || handled;
        }
        if let Mouse::Hover(line, column) = mouse {
            let hovered = self.row_at(line.max(0) as usize, column);
            if hovered != self.hovered {
                self.hovered = hovered;
                return true;
            }
            return false;
        }
        if let Mouse::LeftClick(line, column) = mouse {
            let line = line.max(0) as usize;
            let hit = self
                .clickable
                .iter()
                .find(|(y, start, end, _)| *y == line && column >= *start && column < *end)
                .map(|(_, _, _, row)| *row);
            if let Some(row) = hit {
                if let Some(position) = self.focusable.iter().position(|r| *r == row) {
                    self.focus = position;
                }
                self.show_focus = false;
                self.activate(row);
                return true;
            }
        }
        false
    }

    fn row_at(&self, line: usize, column: usize) -> Option<Row> {
        self.clickable
            .iter()
            .find(|(y, start, end, _)| *y == line && column >= *start && column < *end)
            .map(|(_, _, _, row)| *row)
    }

    pub fn handle_timer(&mut self) -> bool {
        if self.step == Step::Keys {
            return self.keys.handle_timer();
        }
        false
    }

    pub fn render(&mut self, rows: usize, cols: usize) {
        if self.step == Step::Keys {
            let width = 72.min(cols);
            let height = 18.min(rows);
            let x = cols.saturating_sub(width) / 2;
            let y = rows.saturating_sub(height) / 2;
            self.keys.render(x, y, width, height);
            self.keys.render_overlays(rows, cols);
            return;
        }
        if self.step == Step::Result && !self.widened && self.result_width() + 2 > cols {
            self.widened = true;
            if let Some(coordinates) = FloatingPaneCoordinates::new(
                Some("5%".to_owned()),
                None,
                Some("90%".to_owned()),
                None,
                None,
                None,
            ) {
                change_floating_panes_coordinates(vec![(
                    PaneId::Plugin(self.own_plugin_id),
                    coordinates,
                )]);
            }
        }
        let width = if self.step == Step::Result {
            self.result_width()
                .max(WIDTH)
                .min(cols.saturating_sub(2))
                .max(20)
        } else {
            WIDTH.min(cols.saturating_sub(2)).max(20)
        };
        let mut blocks = match self.step {
            Step::Result => self.result_blocks(width),
            Step::Applying => vec![Block::Text(
                Text::from("Applying your choices…").color_all(2),
            )],
            _ => self.choose_blocks(width),
        };
        let footer = match blocks.last() {
            Some(Block::Buttons(_)) => blocks.pop(),
            _ => None,
        };
        self.focusable = blocks
            .iter()
            .filter_map(|block| match block {
                Block::Checkbox(checkbox) => Some(checkbox.row),
                _ => None,
            })
            .collect();
        if self.focus >= self.focusable.len() {
            self.focus = self.focusable.len().saturating_sub(1);
        }
        let focused_row = self.focusable.get(self.focus).copied();
        let shown_focus = if self.show_focus { focused_row } else { None };
        let help_rows = 2;
        let available = rows.saturating_sub(help_rows).max(1);
        let button_lines = match &footer {
            Some(Block::Buttons(buttons)) => button_rows(buttons, width).len(),
            _ => 0,
        };
        let footer_rows = if footer.is_some() {
            (button_lines + 1).min(available)
        } else {
            0
        };
        let body_height = available.saturating_sub(footer_rows).max(1);
        let focused_line = blocks.iter().position(|block| match block {
            Block::Checkbox(checkbox) => Some(checkbox.row) == focused_row,
            _ => false,
        });
        let offset = focused_line
            .map(|line| (line + 2).saturating_sub(body_height))
            .unwrap_or(0)
            .min(blocks.len().saturating_sub(body_height));
        let x = cols.saturating_sub(width) / 2;
        let content_rows = blocks.len().min(body_height) + footer_rows;
        let top = available.saturating_sub(content_rows) / 2;
        self.clickable.clear();
        let mut y = top;
        for block in blocks.into_iter().skip(offset).take(body_height) {
            self.draw_block(block, shown_focus, x, y, width);
            y += 1;
        }
        if let Some(footer) = footer {
            if footer_rows > button_lines {
                y += 1;
            }
            self.draw_block(footer, shown_focus, x, y, width);
        }
        let help = match self.step {
            Step::Choose if self.can_uninstall() => {
                "<↓↑> move · <Space> tick · <?> more · <Del> uninstall"
            },
            Step::Choose => "<↓↑> move · <Space> tick · <?> more",
            _ => "",
        };
        print_text_with_coordinates(
            Text::from(truncate(help, cols)).dim_all(),
            0,
            rows.saturating_sub(1),
            None,
            None,
        );
    }

    fn draw_block(
        &mut self,
        block: Block,
        focused_row: Option<Row>,
        x: usize,
        y: usize,
        width: usize,
    ) {
        match block {
            Block::Text(text) => print_text_with_coordinates(text, x, y, Some(width), None),
            Block::Checkbox(checkbox) => {
                let mark = if checkbox.ticked { "[x]" } else { "[ ]" };
                let mark = if checkbox.disabled {
                    Text::from(mark).dim_all()
                } else if Some(checkbox.row) == self.hovered {
                    Text::from(mark).color_indices(3, vec![0, 2])
                } else {
                    Text::from(mark)
                };
                let label_width = width.saturating_sub(8);
                let label_text = truncate(&checkbox.label, label_width);
                let label_length = zellij_tile::graphemes::display_width(&label_text);
                let label = Text::from(label_text);
                let label = if checkbox.disabled {
                    label.dim_all()
                } else {
                    label.color_range(0, ..)
                };
                let label = if Some(checkbox.row) == focused_row {
                    label.selected()
                } else {
                    label
                };
                let info_x = x + 4 + label_length + 1;
                if let Some(topic) = topic_of(checkbox.row) {
                    let info_row = Row::Info(topic);
                    let info = if Some(info_row) == self.hovered {
                        Text::from("[?]").color_indices(3, vec![0, 1, 2])
                    } else {
                        Text::from("[?]").color_indices(3, vec![1])
                    };
                    self.clickable.push((y, info_x, info_x + 3, info_row));
                    print_text_with_coordinates(info, info_x, y, Some(3), None);
                }
                if checkbox.uninstall {
                    let uninstall_x = info_x + 4;
                    let label = "[Uninstall]";
                    let uninstall = if Some(Row::Uninstall) == self.hovered {
                        Text::from(label).color_all(3)
                    } else {
                        Text::from(label).color_range(3, 1..label.len() - 1)
                    };
                    self.clickable.push((
                        y,
                        uninstall_x,
                        uninstall_x + label.len(),
                        Row::Uninstall,
                    ));
                    print_text_with_coordinates(uninstall, uninstall_x, y, Some(label.len()), None);
                }
                self.clickable
                    .push((y, x, x + 4 + label_length, checkbox.row));
                print_text_with_coordinates(mark, x, y, Some(3), None);
                print_text_with_coordinates(label, x + 4, y, Some(label_length.max(1)), None);
            },
            Block::Buttons(buttons) => {
                for (line, indices) in button_rows(&buttons, width).into_iter().enumerate() {
                    let mut column = x;
                    for index in indices {
                        let button = &buttons[index];
                        let text = button.text();
                        let text_width = button.width();
                        let key_end = button.key.chars().count();
                        let mut styled = Text::from(text).color_range(3, ..key_end);
                        if Some(button.row) == self.hovered {
                            styled = styled.color_range(3, key_end..);
                        }
                        self.clickable
                            .push((y + line, column, column + text_width, button.row));
                        print_text_with_coordinates(styled, column, y + line, None, None);
                        column += text_width + BUTTON_GAP;
                    }
                }
            },
        }
    }

    fn choose_blocks(&self, width: usize) -> Vec<Block> {
        let mut blocks = vec![
            Block::Text(Text::from("Welcome to Zellij!").color_all(2)),
            Block::Text(Text::from("")),
        ];
        for line in emphasized_wrap(INTRODUCTION, &INTRODUCTION_COMMANDS, width) {
            blocks.push(Block::Text(line));
        }
        blocks.push(Block::Text(Text::from("")));
        match (&self.status, &self.status_error) {
            (None, None) => {
                blocks.push(Block::Text(Text::from("Checking this computer…").dim_all()));
                blocks.push(Block::Text(Text::from("")));
            },
            (None, Some(error)) => {
                for line in wrap(&format!("Desktop setup is not available: {}", error), width) {
                    blocks.push(Block::Text(Text::from(line).dim_all()));
                }
                blocks.push(Block::Text(Text::from("")));
            },
            (Some(_), _) => {
                let lines = self.visible_lines();
                for line in &lines {
                    let done = self.line_done(*line);
                    blocks.push(Block::Checkbox(Checkbox {
                        ticked: done || self.ticked.contains(line),
                        label: self.line_label(*line),
                        row: Row::Line(*line),
                        disabled: done,
                        uninstall: *line == Line::Entry && self.can_uninstall(),
                    }));
                    if let (Line::Copy, Some(upgrade)) = (line, self.upgrade()) {
                        let at = format!("    at {}", upgrade.path.display());
                        blocks.push(Block::Text(Text::from(truncate(&at, width)).dim_all()));
                    }
                }
            },
        }
        blocks.push(Block::Checkbox(Checkbox {
            ticked: self.set_up_keys,
            label: "Set up keybindings to avoid collisions with editors.".to_owned(),
            row: Row::Keys,
            disabled: false,
            uninstall: false,
        }));
        blocks.push(Block::Text(Text::from("")));
        if let Some((notice, succeeded)) = &self.notice {
            let color = if *succeeded { 2 } else { 3 };
            for line in wrap(notice, width) {
                blocks.push(Block::Text(Text::from(line).color_range(color, ..1)));
            }
        }
        if self.any_done() {
            blocks.push(Block::Text(
                Text::from("Grayed out items are already set up.").dim_all(),
            ));
        }
        blocks.push(Block::Text(Text::from("You can always change this later.")));
        let approve = if self.set_up_keys {
            "Approve and Continue"
        } else {
            "Approve and Finish"
        };
        let mut buttons = vec![
            Button::new("<Enter>", approve, Row::Approve),
            Button::new("<Esc>", "Dismiss", Row::Dismiss),
        ];
        if self.auto_shown {
            buttons.push(Button::new("<Ctrl c>", "Don't Show Again", Row::Never));
        }
        blocks.push(Block::Buttons(buttons));
        blocks
    }

    fn result_width(&self) -> usize {
        self.outcomes
            .iter()
            .flat_map(|outcome| outcome_sections(&outcome.message))
            .flat_map(|section| match section {
                OutcomeSection::Files(_, files) => files,
                OutcomeSection::Note(_) => vec![],
            })
            .map(|file| zellij_tile::graphemes::display_width(&file) + 6)
            .max()
            .unwrap_or(0)
    }

    fn result_blocks(&self, width: usize) -> Vec<Block> {
        let failed = self.outcomes.iter().any(|o| !o.succeeded)
            || self.status_error.is_some()
            || matches!(self.config_result, Some(Err(_)));
        let heading = if failed {
            "Some changes did not work"
        } else {
            "All set"
        };
        let mut blocks = vec![
            Block::Text(Text::from(heading).color_all(2)),
            Block::Text(Text::from("")),
        ];
        for (index, outcome) in self.outcomes.iter().enumerate() {
            if index > 0 {
                blocks.push(Block::Text(Text::from("")));
            }
            let (mark, color) = if outcome.succeeded {
                ("✓", 2)
            } else {
                ("✗", 3)
            };
            let verb = if outcome.install { "" } else { "Removed: " };
            blocks.push(Block::Text(
                Text::from(truncate(
                    &format!("{} {}{}", mark, verb, outcome.item.label()),
                    width,
                ))
                .color_range(color, ..1),
            ));
            for section in outcome_sections(&outcome.message) {
                match section {
                    OutcomeSection::Files(heading, files) => {
                        blocks.push(Block::Text(Text::from(format!("  {}", heading)).dim_all()));
                        for file in files {
                            let line = truncate_end(&format!("    - {}", file), width);
                            let length = line.chars().count();
                            blocks.push(Block::Text(
                                Text::from(line).dim_all().color_range(0, 6.min(length)..),
                            ));
                        }
                    },
                    OutcomeSection::Note(note) => {
                        for wrapped in wrap(&note, width.saturating_sub(2)) {
                            blocks
                                .push(Block::Text(Text::from(format!("  {}", wrapped)).dim_all()));
                        }
                    },
                }
            }
        }
        if let Some(error) = &self.status_error {
            for wrapped in wrap(&format!("✗ {}", error), width) {
                blocks.push(Block::Text(Text::from(wrapped).color_range(3, ..1)));
            }
        }
        match &self.config_result {
            Some(Ok(())) => blocks.push(Block::Text(
                Text::from("✓ Keybindings saved").color_range(2, ..1),
            )),
            Some(Err(path)) => {
                let file = path
                    .clone()
                    .unwrap_or_else(|| "your config file".to_owned());
                for wrapped in wrap(
                    &format!("✗ Keybindings could not be saved to {}", file),
                    width,
                ) {
                    blocks.push(Block::Text(Text::from(wrapped).color_range(3, ..1)));
                }
            },
            None => {},
        }
        blocks.push(Block::Text(Text::from("")));
        if let Some(offer) = &self.replace_offer {
            for wrapped in wrap(offer, width) {
                blocks.push(Block::Text(Text::from(wrapped).color_all(3)));
            }
        }
        let mut buttons = vec![];
        if self.replace_offer.is_some() {
            buttons.push(Button::new("<r>", "Replace it", Row::Replace));
        }
        buttons.push(Button::new("<Enter>", "Close", Row::Close));
        blocks.push(Block::Buttons(buttons));
        blocks
    }
}

const INTRODUCTION: &str = "Zellij is both a terminal and multiplexer. It can run on its own with zellij terminal or inside another terminal with zellij. It can also integrate with your desktop and become your default terminal.";
const INTRODUCTION_COMMANDS: [&str; 2] = ["zellij terminal", "zellij."];

fn emphasized_ranges(text: &str, commands: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = vec![];
    for command in commands {
        let command = command.trim_end_matches('.');
        let mut search_from = 0;
        while let Some(found) = text[search_from..].find(command) {
            let byte_start = search_from + found;
            let start = text[..byte_start].chars().count();
            let end = start + command.chars().count();
            if !ranges.iter().any(|(s, e)| start < *e && end > *s) {
                ranges.push((start, end));
            }
            search_from = byte_start + command.len();
        }
    }
    ranges
}

fn emphasized_wrap(text: &str, commands: &[&str], width: usize) -> Vec<Text> {
    let mut ranges = emphasized_ranges(text, &commands[..1]);
    for (start, end) in emphasized_ranges(text, &commands[1..]) {
        let after = text.chars().nth(end);
        if after == Some('.') && !ranges.iter().any(|(s, e)| start < *e && end > *s) {
            ranges.push((start, end));
        }
    }
    let mut lines = vec![];
    let mut position = 0;
    for line in wrap(text, width) {
        let length = line.chars().count();
        let line_start = position;
        let line_end = position + length;
        let mut styled = Text::from(line);
        for (start, end) in &ranges {
            let from = (*start).max(line_start);
            let to = (*end).min(line_end);
            if from < to {
                styled = styled.color_range(3, from - line_start..to - line_start);
            }
        }
        lines.push(styled);
        position = line_end + 1;
    }
    lines
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OutcomeSection {
    Files(&'static str, Vec<String>),
    Note(String),
}

fn outcome_sections(message: &str) -> Vec<OutcomeSection> {
    let mut sections: Vec<OutcomeSection> = vec![];
    for line in message.lines() {
        let file = [("Wrote ", "Wrote"), ("Removed ", "Removed")]
            .into_iter()
            .find_map(|(prefix, heading)| line.strip_prefix(prefix).map(|file| (heading, file)));
        match file {
            Some((heading, file)) => {
                let existing = sections.iter_mut().find_map(|section| match section {
                    OutcomeSection::Files(h, files) if *h == heading => Some(files),
                    _ => None,
                });
                match existing {
                    Some(files) => files.push(file.to_owned()),
                    None => sections.push(OutcomeSection::Files(heading, vec![file.to_owned()])),
                }
            },
            None if !line.trim().is_empty() => sections.push(OutcomeSection::Note(line.to_owned())),
            None => {},
        }
    }
    sections
}

fn truncate_end(text: &str, width: usize) -> String {
    if zellij_tile::graphemes::display_width(text) <= width {
        return text.to_owned();
    }
    let mut cut = truncate(text, width.saturating_sub(1));
    cut.push('…');
    cut
}

fn truncate(text: &str, width: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = zellij_tile::graphemes::display_width(&character.to_string());
        if used + character_width > width {
            break;
        }
        used += character_width;
        result.push(character);
    }
    result
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut lines = vec![];
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{} {}", current, word)
        };
        if zellij_tile::graphemes::display_width(&candidate) > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_owned();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &Text) -> String {
        StyledText::from(text.clone()).text
    }

    #[test]
    fn the_introduction_wraps_and_keeps_both_commands_emphasized() {
        let lines = emphasized_wrap(INTRODUCTION, &INTRODUCTION_COMMANDS, 40);
        assert!(lines.len() > 1);
        let mut emphasized = String::new();
        for line in &lines {
            let styled = StyledText::from(line.clone());
            assert!(zellij_tile::graphemes::display_width(&styled.text) <= 40);
            let indices = styled.indices.get(3).cloned().unwrap_or_default();
            for (index, character) in styled.text.chars().enumerate() {
                if indices.contains(&index) {
                    emphasized.push(character);
                }
            }
            emphasized.push('|');
        }
        let emphasized: String = emphasized.replace('|', " ");
        let words: Vec<&str> = emphasized.split_whitespace().collect();
        assert_eq!(words, vec!["zellij", "terminal", "zellij"]);
        assert!(plain(&lines[0]).starts_with("Zellij is both"));
    }

    #[test]
    fn buttons_move_to_the_next_row_only_when_they_do_not_fit() {
        let buttons = vec![
            Button::new("<Enter>", "Approve and Finish", Row::Approve),
            Button::new("<Esc>", "Dismiss", Row::Dismiss),
            Button::new("<Ctrl c>", "Don't Show Again", Row::Never),
        ];
        assert_eq!(button_rows(&buttons, WIDTH), vec![vec![0, 1, 2]]);
        assert_eq!(button_rows(&buttons, 45), vec![vec![0, 1], vec![2]]);
        assert_eq!(button_rows(&buttons, 30), vec![vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn written_files_are_grouped_under_one_heading_and_notes_kept() {
        let sections = outcome_sections(
            "Wrote /a/app.desktop\nWrote /a/icon.png\nIt runs /bin/zellij\nRemoved /a/old.desktop",
        );
        assert_eq!(
            sections,
            vec![
                OutcomeSection::Files(
                    "Wrote",
                    vec!["/a/app.desktop".to_owned(), "/a/icon.png".to_owned()]
                ),
                OutcomeSection::Note("It runs /bin/zellij".to_owned()),
                OutcomeSection::Files("Removed", vec!["/a/old.desktop".to_owned()]),
            ]
        );
    }

    #[test]
    fn a_path_too_long_for_the_pane_is_cut_at_the_end() {
        assert_eq!(truncate_end("    - /short", 20), "    - /short");
        let cut = truncate_end("    - /a/very/long/path/to/a/file.png", 20);
        assert_eq!(zellij_tile::graphemes::display_width(&cut), 20);
        assert!(cut.ends_with('…') && cut.starts_with("    - /a/very"));
    }
}
