use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Feature {
    NestedSessions,
    PaneFocus,
    ScrollByCommand,
    Inbox,
    SessionHistory,
    FloatingPanes,
    StackOrientation,
    Bars,
    SwapLayouts,
    SessionRename,
}

impl Feature {
    pub fn label(&self) -> &'static str {
        match self {
            Feature::NestedSessions => "nested sessions",
            Feature::PaneFocus => "pane focus",
            Feature::ScrollByCommand => "scrolling by command",
            Feature::Inbox => "the inbox and pane status",
            Feature::SessionHistory => "moving between sessions",
            Feature::FloatingPanes => "floating panes",
            Feature::StackOrientation => "stack orientation",
            Feature::Bars => "hiding the bars",
            Feature::SwapLayouts => "swap layouts",
            Feature::SessionRename => "renaming sessions",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Detection {
    ActionBoundAnywhere(Action),
    KeyBoundToActions,
}

#[derive(Debug, Clone)]
pub struct ExpectedBind {
    pub feature: Feature,
    pub mode: InputMode,
    pub key: KeyWithModifier,
    pub bound_actions: Vec<Action>,
    pub detection: Detection,
    pub description: &'static str,
    pub returns_to_base_mode: bool,
    pub in_base_mode: bool,
}

impl ExpectedBind {
    fn new(
        feature: Feature,
        mode: InputMode,
        key: KeyWithModifier,
        action: Action,
        description: &'static str,
        returns_to_base_mode: bool,
    ) -> Self {
        ExpectedBind {
            feature,
            mode,
            key,
            bound_actions: vec![action.clone()],
            detection: Detection::ActionBoundAnywhere(action),
            description,
            returns_to_base_mode,
            in_base_mode: false,
        }
    }
    fn with_bound_actions(mut self, bound_actions: Vec<Action>) -> Self {
        self.bound_actions = bound_actions;
        self
    }
    fn detected_by_key(mut self) -> Self {
        self.detection = Detection::KeyBoundToActions;
        self
    }
    pub fn key_text(&self) -> String {
        format!("<{}>", self.key)
    }
    pub fn mode_name(&self) -> String {
        if self.in_base_mode {
            return "ANY".to_owned();
        }
        format!("{:?}", self.mode).to_uppercase()
    }
    pub fn mode_in(&self, base_mode: InputMode) -> InputMode {
        if self.in_base_mode {
            base_mode
        } else {
            self.mode
        }
    }
    pub fn actions(&self, base_mode: InputMode) -> Vec<Action> {
        let mut actions = self.bound_actions.clone();
        if self.returns_to_base_mode {
            actions.push(Action::SwitchToMode {
                input_mode: base_mode,
            });
        }
        actions
    }
    fn is_bound(&self, keybinds: &KeybindsVec, base_mode: InputMode) -> bool {
        match &self.detection {
            Detection::ActionBoundAnywhere(action) => action_is_bound(keybinds, action),
            Detection::KeyBoundToActions => {
                actions_bound_to_key(keybinds, self.mode_in(base_mode), &self.key)
                    == Some(self.actions(base_mode))
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApplyStatus {
    NotApplied,
    Applying,
    Applied,
    Failed(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shortcut {
    pub keys: String,
    pub action: Action,
}

#[derive(Debug)]
pub struct KeybindingState {
    expected: Vec<ExpectedBind>,
    missing: Vec<usize>,
    conflicts: Vec<(usize, Vec<Action>)>,
    applied: Vec<ExpectedBind>,
    base_mode: InputMode,
    status: ApplyStatus,
    inbox: Option<Shortcut>,
    session_manager: Option<Shortcut>,
}

impl Default for KeybindingState {
    fn default() -> Self {
        KeybindingState {
            expected: expected_binds(),
            missing: vec![],
            conflicts: vec![],
            applied: vec![],
            base_mode: InputMode::Normal,
            status: ApplyStatus::NotApplied,
            inbox: None,
            session_manager: None,
        }
    }
}

impl KeybindingState {
    pub fn set_base_mode(&mut self, base_mode: InputMode) -> bool {
        let changed = self.base_mode != base_mode;
        self.base_mode = base_mode;
        changed
    }
    pub fn update_from_keybinds(&mut self, keybinds: &KeybindsVec) -> bool {
        let base_mode = self.base_mode;
        let mut missing = vec![];
        let mut conflicts = vec![];
        for (index, expected_bind) in self.expected.iter().enumerate() {
            if expected_bind.is_bound(keybinds, base_mode) {
                continue;
            }
            missing.push(index);
            if let Some(currently_bound_actions) = actions_bound_to_key(
                keybinds,
                expected_bind.mode_in(base_mode),
                &expected_bind.key,
            ) {
                if currently_bound_actions != expected_bind.actions(base_mode) {
                    conflicts.push((index, currently_bound_actions));
                }
            }
        }
        let inbox = find_shortcut(keybinds, base_mode, |action| {
            matches!(action, Action::ToggleSessionCard)
        });
        let session_manager = find_shortcut(keybinds, base_mode, |action| {
            action.launches_plugin("session-manager")
                || action.launches_plugin("zellij:session-manager")
        });
        let changed = missing != self.missing
            || conflicts != self.conflicts
            || inbox != self.inbox
            || session_manager != self.session_manager;
        self.missing = missing;
        self.conflicts = conflicts;
        self.inbox = inbox;
        self.session_manager = session_manager;
        changed
    }
    pub fn inbox_keys(&self) -> Option<String> {
        self.inbox.as_ref().map(|shortcut| shortcut.keys.clone())
    }
    pub fn session_manager_keys(&self) -> Option<String> {
        self.session_manager
            .as_ref()
            .map(|shortcut| shortcut.keys.clone())
    }
    pub fn open_inbox(&self) {
        let action = self
            .inbox
            .as_ref()
            .map(|shortcut| shortcut.action.clone())
            .unwrap_or(Action::ToggleSessionCard);
        run_action(action, BTreeMap::new());
    }
    pub fn open_session_manager(&self) {
        match &self.session_manager {
            Some(shortcut) => run_action(shortcut.action.clone(), BTreeMap::new()),
            None => {
                open_plugin_pane_floating(
                    "zellij:session-manager",
                    BTreeMap::new(),
                    None,
                    BTreeMap::new(),
                );
            },
        }
    }
    pub fn has_missing_binds(&self) -> bool {
        !self.missing.is_empty()
    }
    pub fn has_missing_binds_for(&self, feature: Feature) -> bool {
        self.missing
            .iter()
            .filter_map(|index| self.expected.get(*index))
            .any(|bind| bind.feature == feature)
    }
    pub fn features_to_add(&self) -> Vec<Feature> {
        let mut features: Vec<Feature> = vec![];
        for bind in self.binds_to_add() {
            if !features.contains(&bind.feature) {
                features.push(bind.feature);
            }
        }
        features
    }
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
    pub fn binds_to_add(&self) -> Vec<&ExpectedBind> {
        if !self.applied.is_empty() {
            return self.applied.iter().collect();
        }
        self.missing
            .iter()
            .filter_map(|index| self.expected.get(*index))
            .collect()
    }
    pub fn binds_for_config(&self) -> Vec<(InputMode, KeyWithModifier, Vec<Action>)> {
        self.binds_to_add()
            .iter()
            .map(|bind| {
                (
                    bind.mode_in(self.base_mode),
                    bind.key.clone(),
                    bind.actions(self.base_mode),
                )
            })
            .collect()
    }
    pub fn conflicting_binds(&self) -> Vec<(&ExpectedBind, String)> {
        self.conflicts
            .iter()
            .filter_map(|(index, currently_bound_actions)| {
                self.expected
                    .get(*index)
                    .map(|bind| (bind, describe_actions(currently_bound_actions)))
            })
            .collect()
    }
    pub fn status(&self) -> &ApplyStatus {
        &self.status
    }
    pub fn set_status(&mut self, status: ApplyStatus) {
        self.status = status;
    }
    fn apply(&mut self) {
        let mut keys_to_unbind = vec![];
        let mut keys_to_rebind = vec![];
        for index in &self.missing {
            let Some(expected_bind) = self.expected.get(*index) else {
                continue;
            };
            let mode = expected_bind.mode_in(self.base_mode);
            if self.conflicts.iter().any(|(i, _)| i == index) {
                keys_to_unbind.push((mode, expected_bind.key.clone()));
            }
            keys_to_rebind.push((
                mode,
                expected_bind.key.clone(),
                expected_bind.actions(self.base_mode),
            ));
        }
        if keys_to_rebind.is_empty() {
            return;
        }
        self.applied = self
            .missing
            .iter()
            .filter_map(|index| self.expected.get(*index))
            .cloned()
            .collect();
        self.status = ApplyStatus::Applying;
        let write_config_to_disk = true;
        rebind_keys(keys_to_unbind, keys_to_rebind, write_config_to_disk);
    }
}

pub fn apply_missing_binds(keybinding_state: &Rc<RefCell<KeybindingState>>) {
    keybinding_state.borrow_mut().apply();
}

pub fn expected_binds() -> Vec<ExpectedBind> {
    vec![
        ExpectedBind::new(
            Feature::NestedSessions,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char(']')),
            Action::FocusHostSession,
            "host session",
            true,
        ),
        ExpectedBind::new(
            Feature::NestedSessions,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('[')),
            Action::FocusGuestSession,
            "guest session",
            true,
        ),
        ExpectedBind::new(
            Feature::NestedSessions,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('f')),
            Action::ToggleHostFullscreen,
            "host fullscreen",
            true,
        ),
        ExpectedBind::new(
            Feature::PaneFocus,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char(';')),
            Action::FocusLastPane,
            "last focused pane",
            false,
        ),
        ExpectedBind::new(
            Feature::ScrollByCommand,
            InputMode::Scroll,
            KeyWithModifier::new(BareKey::Char('[')),
            Action::ScrollToPreviousPrompt,
            "previous command",
            false,
        ),
        ExpectedBind::new(
            Feature::ScrollByCommand,
            InputMode::Scroll,
            KeyWithModifier::new(BareKey::Char(']')),
            Action::ScrollToNextPrompt,
            "next command",
            false,
        ),
        ExpectedBind::new(
            Feature::ScrollByCommand,
            InputMode::Scroll,
            KeyWithModifier::new(BareKey::Char('m')),
            Action::SelectCommandAtScrollPosition,
            "select command",
            false,
        ),
        ExpectedBind::new(
            Feature::ScrollByCommand,
            InputMode::Scroll,
            KeyWithModifier::new(BareKey::Char('c')),
            Action::CopyLastCommandOutput,
            "copy last output",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('i')),
            Action::ToggleSessionCard,
            "inbox",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('u')),
            Action::MuteSession,
            "mute session",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('n')),
            Action::GoToNextAttention,
            "next item that needs you",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('u')),
            Action::MutePane,
            "mute",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('g')),
            Action::IgnorePaneOutput,
            "ignore output",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('v')),
            Action::WatchPane,
            "watch",
            true,
        ),
        ExpectedBind::new(
            Feature::Inbox,
            InputMode::Tab,
            KeyWithModifier::new(BareKey::Char('u')),
            Action::MuteTab,
            "mute tab",
            true,
        ),
        ExpectedBind::new(
            Feature::SessionHistory,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Tab),
            Action::ToggleSession,
            "previous session",
            true,
        ),
        ExpectedBind::new(
            Feature::SessionHistory,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Left),
            Action::GoToPreviousSession,
            "back",
            true,
        ),
        ExpectedBind::new(
            Feature::SessionHistory,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Right),
            Action::GoToNextSession,
            "forward",
            true,
        ),
        ExpectedBind::new(
            Feature::FloatingPanes,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('a')),
            Action::NewFloatingPane {
                command: None,
                pane_name: None,
                coordinates: None,
                near_current_pane: false,
                no_focus: false,
                tab_id: None,
            },
            "new floating pane",
            true,
        ),
        ExpectedBind::new(
            Feature::StackOrientation,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('s')).with_shift_modifier(),
            Action::ToggleStackListOrientation,
            "toggle stack orientation",
            true,
        ),
        ExpectedBind::new(
            Feature::Bars,
            InputMode::Pane,
            KeyWithModifier::new(BareKey::Char('z')).with_shift_modifier(),
            Action::ToggleBars { bars: vec![] },
            "hide or show the bars",
            true,
        ),
        ExpectedBind::new(
            Feature::SwapLayouts,
            InputMode::Tab,
            KeyWithModifier::new(BareKey::Char(' ')),
            Action::NextSwapLayout,
            "next",
            false,
        )
        .detected_by_key(),
        ExpectedBind::new(
            Feature::SwapLayouts,
            InputMode::Tab,
            KeyWithModifier::new(BareKey::Char(' ')).with_shift_modifier(),
            Action::PreviousSwapLayout,
            "previous",
            false,
        )
        .detected_by_key(),
        ExpectedBind::new(
            Feature::SessionRename,
            InputMode::Session,
            KeyWithModifier::new(BareKey::Char('r')),
            Action::SwitchToMode {
                input_mode: InputMode::RenameSession,
            },
            "rename session",
            false,
        )
        .with_bound_actions(vec![
            Action::SwitchToMode {
                input_mode: InputMode::RenameSession,
            },
            Action::SessionNameInput { input: vec![0] },
        ]),
        ExpectedBind::new(
            Feature::SessionRename,
            InputMode::RenameSession,
            KeyWithModifier::new(BareKey::Esc),
            Action::UndoRenameSession,
            "undo",
            false,
        )
        .with_bound_actions(vec![
            Action::UndoRenameSession,
            Action::SwitchToMode {
                input_mode: InputMode::Session,
            },
        ]),
        ExpectedBind::new(
            Feature::SessionRename,
            InputMode::RenameSession,
            KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier(),
            Action::SwitchToMode {
                input_mode: InputMode::Normal,
            },
            "done",
            true,
        )
        .with_bound_actions(vec![])
        .detected_by_key(),
    ]
}

const MAX_MODE_HOPS: usize = 3;

pub fn find_shortcut(
    keybinds: &KeybindsVec,
    base_mode: InputMode,
    is_target: impl Fn(&Action) -> bool,
) -> Option<Shortcut> {
    let mut visited = vec![base_mode];
    let mut frontier: Vec<(InputMode, Vec<KeyWithModifier>)> = vec![(base_mode, vec![])];
    for _ in 0..=MAX_MODE_HOPS {
        let mut next_frontier = vec![];
        for (mode, path) in &frontier {
            let Some((_, mode_binds)) = keybinds.iter().find(|(m, _)| m == mode) else {
                continue;
            };
            for (key, actions) in mode_binds {
                if let Some(action) = actions.iter().find(|action| is_target(action)) {
                    let mut keys = path.clone();
                    keys.push(key.clone());
                    return Some(Shortcut {
                        keys: keys
                            .iter()
                            .map(|key| key.to_string())
                            .collect::<Vec<_>>()
                            .join(" + "),
                        action: action.clone(),
                    });
                }
            }
            for (key, actions) in mode_binds {
                for action in actions {
                    if let Action::SwitchToMode { input_mode } = action {
                        if !visited.contains(input_mode) {
                            visited.push(*input_mode);
                            let mut keys = path.clone();
                            keys.push(key.clone());
                            next_frontier.push((*input_mode, keys));
                        }
                    }
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }
    None
}

fn action_is_bound(keybinds: &KeybindsVec, action: &Action) -> bool {
    keybinds.iter().any(|(_mode, mode_binds)| {
        mode_binds
            .iter()
            .any(|(_key, actions)| actions.iter().any(|a| a == action))
    })
}

fn actions_bound_to_key(
    keybinds: &KeybindsVec,
    mode: InputMode,
    key: &KeyWithModifier,
) -> Option<Vec<Action>> {
    keybinds
        .iter()
        .find(|(bind_mode, _)| bind_mode == &mode)
        .and_then(|(_mode, mode_binds)| {
            mode_binds
                .iter()
                .find(|(bind_key, _)| bind_key == key)
                .map(|(_key, actions)| actions.clone())
        })
}

fn describe_actions(actions: &Vec<Action>) -> String {
    let significant_actions: Vec<&Action> = actions
        .iter()
        .filter(|action| !matches!(action, Action::SwitchToMode { .. }))
        .collect();
    let actions_to_describe = if significant_actions.is_empty() {
        actions.iter().collect()
    } else {
        significant_actions
    };
    actions_to_describe
        .iter()
        .map(|action| action.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
