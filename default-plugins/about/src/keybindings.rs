use std::cell::RefCell;
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
}

impl Feature {
    pub fn label(&self) -> &'static str {
        match self {
            Feature::NestedSessions => "nested sessions",
            Feature::PaneFocus => "pane focus",
            Feature::ScrollByCommand => "scrolling by command",
            Feature::Inbox => "the inbox and pane status",
            Feature::SessionHistory => "moving between sessions",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpectedBind {
    pub feature: Feature,
    pub mode: InputMode,
    pub key: KeyWithModifier,
    pub action: Action,
    pub description: &'static str,
    pub returns_to_base_mode: bool,
    pub in_base_mode: bool,
}

impl ExpectedBind {
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
        if self.returns_to_base_mode {
            vec![
                self.action.clone(),
                Action::SwitchToMode {
                    input_mode: base_mode,
                },
            ]
        } else {
            vec![self.action.clone()]
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

#[derive(Debug)]
pub struct KeybindingState {
    expected: Vec<ExpectedBind>,
    missing: Vec<usize>,
    conflicts: Vec<(usize, Vec<Action>)>,
    applied: Vec<ExpectedBind>,
    base_mode: InputMode,
    status: ApplyStatus,
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
        }
    }
}

impl KeybindingState {
    pub fn set_base_mode(&mut self, base_mode: InputMode) -> bool {
        let changed = self.base_mode != base_mode;
        self.base_mode = base_mode;
        changed
    }
    pub fn base_mode(&self) -> InputMode {
        self.base_mode
    }
    pub fn base_mode_name(&self) -> String {
        format!("{:?}", self.base_mode).to_uppercase()
    }
    pub fn update_from_keybinds(&mut self, keybinds: &KeybindsVec) -> bool {
        let base_mode = self.base_mode;
        let mut missing = vec![];
        let mut conflicts = vec![];
        for (index, expected_bind) in self.expected.iter().enumerate() {
            if action_is_bound(keybinds, &expected_bind.action) {
                continue;
            }
            missing.push(index);
            if let Some(currently_bound_actions) = actions_bound_to_key(
                keybinds,
                expected_bind.mode_in(base_mode),
                &expected_bind.key,
            )
            {
                if currently_bound_actions != expected_bind.actions(base_mode) {
                    conflicts.push((index, currently_bound_actions));
                }
            }
        }
        let changed = missing != self.missing || conflicts != self.conflicts;
        self.missing = missing;
        self.conflicts = conflicts;
        changed
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
    pub fn bind_for_action(&self, action: &Action) -> Option<&ExpectedBind> {
        self.expected.iter().find(|bind| &bind.action == action)
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
        ExpectedBind {
            feature: Feature::NestedSessions,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Char(']')),
            action: Action::FocusHostSession,
            description: "Focus the host (outer) session",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::NestedSessions,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Char('[')),
            action: Action::FocusGuestSession,
            description: "Focus the guest (inner) session",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::NestedSessions,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Char('f')),
            action: Action::ToggleHostFullscreen,
            description: "Toggle host session fullscreen",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::PaneFocus,
            mode: InputMode::Pane,
            key: KeyWithModifier::new(BareKey::Char(';')),
            action: Action::FocusLastPane,
            description: "Focus the last focused pane",
            returns_to_base_mode: false,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::ScrollByCommand,
            mode: InputMode::Scroll,
            key: KeyWithModifier::new(BareKey::Char('[')),
            action: Action::ScrollToPreviousPrompt,
            description: "Scroll to the previous command",
            returns_to_base_mode: false,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::ScrollByCommand,
            mode: InputMode::Scroll,
            key: KeyWithModifier::new(BareKey::Char(']')),
            action: Action::ScrollToNextPrompt,
            description: "Scroll to the next command",
            returns_to_base_mode: false,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::ScrollByCommand,
            mode: InputMode::Scroll,
            key: KeyWithModifier::new(BareKey::Char('m')),
            action: Action::SelectCommandAtScrollPosition,
            description: "Select the command at the scroll position",
            returns_to_base_mode: false,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::ScrollByCommand,
            mode: InputMode::Scroll,
            key: KeyWithModifier::new(BareKey::Char('c')),
            action: Action::CopyLastCommandOutput,
            description: "Copy the output of the last command",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Normal,
            key: KeyWithModifier::new(BareKey::F(9)),
            action: Action::ToggleSessionCard,
            description: "Open or close the inbox",
            returns_to_base_mode: false,
            in_base_mode: true,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Pane,
            key: KeyWithModifier::new(BareKey::Char('u')),
            action: Action::MutePane,
            description: "Mute the focused pane",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Pane,
            key: KeyWithModifier::new(BareKey::Char('g')),
            action: Action::IgnorePaneOutput,
            description: "Ignore the focused pane's output",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Pane,
            key: KeyWithModifier::new(BareKey::Char('v')),
            action: Action::WatchPane,
            description: "Watch the focused pane",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Tab,
            key: KeyWithModifier::new(BareKey::Char('u')),
            action: Action::MuteTab,
            description: "Mute the focused tab",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Char('u')),
            action: Action::MuteSession,
            description: "Mute this session",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::Inbox,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Char('n')),
            action: Action::GoToNextAttention,
            description: "Go to the next item that needs you",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::SessionHistory,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Tab),
            action: Action::ToggleSession,
            description: "Return to the previous session",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::SessionHistory,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Left),
            action: Action::GoToPreviousSession,
            description: "Go back in session history",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
        ExpectedBind {
            feature: Feature::SessionHistory,
            mode: InputMode::Session,
            key: KeyWithModifier::new(BareKey::Right),
            action: Action::GoToNextSession,
            description: "Go forward in session history",
            returns_to_base_mode: true,
            in_base_mode: false,
        },
    ]
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
