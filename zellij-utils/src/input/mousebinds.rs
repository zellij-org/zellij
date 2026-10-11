use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::actions::Action;
use super::mouse::MouseEvent;
use crate::data::{InputMode, KeyModifier, MouseButton, MouseTarget, MouseTrigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PromptDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResizeScrollDirection {
    Increase,
    Decrease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseBehaviour {
    Click,
    Select,
    FocusPane,
    MovePane,
    ResizePane,
    ToggleFullscreen,
    GroupToggle,
    Ungroup,
    ContextMenu,
    Scroll(usize),
    ScrollColumns(usize),
    ScrollToPrompt(PromptDirection),
    ResizeScroll(ResizeScrollDirection),
    PassToApp,
    Ignore,
}

pub const MOUSE_BEHAVIOUR_NAMES: [&str; 15] = [
    "Click",
    "Select",
    "FocusPane",
    "MovePane",
    "ResizePane",
    "ToggleFullscreen",
    "GroupToggle",
    "Ungroup",
    "ContextMenu",
    "Scroll",
    "ScrollColumns",
    "ScrollToPrompt",
    "ResizeScroll",
    "PassToApp",
    "Ignore",
];

impl MouseBehaviour {
    pub fn name(&self) -> &'static str {
        match self {
            MouseBehaviour::Click => "Click",
            MouseBehaviour::Select => "Select",
            MouseBehaviour::FocusPane => "FocusPane",
            MouseBehaviour::MovePane => "MovePane",
            MouseBehaviour::ResizePane => "ResizePane",
            MouseBehaviour::ToggleFullscreen => "ToggleFullscreen",
            MouseBehaviour::GroupToggle => "GroupToggle",
            MouseBehaviour::Ungroup => "Ungroup",
            MouseBehaviour::ContextMenu => "ContextMenu",
            MouseBehaviour::Scroll(_) => "Scroll",
            MouseBehaviour::ScrollColumns(_) => "ScrollColumns",
            MouseBehaviour::ScrollToPrompt(_) => "ScrollToPrompt",
            MouseBehaviour::ResizeScroll(_) => "ResizeScroll",
            MouseBehaviour::PassToApp => "PassToApp",
            MouseBehaviour::Ignore => "Ignore",
        }
    }
    pub fn is_behaviour_name(name: &str) -> bool {
        MOUSE_BEHAVIOUR_NAMES.contains(&name)
    }
    pub fn from_name_and_arguments(
        name: &str,
        arguments: &[String],
    ) -> Result<MouseBehaviour, String> {
        let no_arguments = |behaviour: MouseBehaviour| {
            if arguments.is_empty() {
                Ok(behaviour)
            } else {
                Err(format!("{} does not take arguments", name))
            }
        };
        let count = |default: usize| -> Result<usize, String> {
            match arguments {
                [] => Ok(default),
                [value] => value
                    .parse::<usize>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or_else(|| format!("{} needs a positive whole number", name)),
                _ => Err(format!("{} takes one number", name)),
            }
        };
        match name {
            "Click" => no_arguments(MouseBehaviour::Click),
            "Select" => no_arguments(MouseBehaviour::Select),
            "FocusPane" => no_arguments(MouseBehaviour::FocusPane),
            "MovePane" => no_arguments(MouseBehaviour::MovePane),
            "ResizePane" => no_arguments(MouseBehaviour::ResizePane),
            "ToggleFullscreen" => no_arguments(MouseBehaviour::ToggleFullscreen),
            "GroupToggle" => no_arguments(MouseBehaviour::GroupToggle),
            "Ungroup" => no_arguments(MouseBehaviour::Ungroup),
            "ContextMenu" => no_arguments(MouseBehaviour::ContextMenu),
            "PassToApp" => no_arguments(MouseBehaviour::PassToApp),
            "Ignore" => no_arguments(MouseBehaviour::Ignore),
            "Scroll" => Ok(MouseBehaviour::Scroll(count(3)?)),
            "ScrollColumns" => Ok(MouseBehaviour::ScrollColumns(count(4)?)),
            "ScrollToPrompt" => match arguments {
                [direction] if direction.eq_ignore_ascii_case("previous") => {
                    Ok(MouseBehaviour::ScrollToPrompt(PromptDirection::Previous))
                },
                [direction] if direction.eq_ignore_ascii_case("next") => {
                    Ok(MouseBehaviour::ScrollToPrompt(PromptDirection::Next))
                },
                _ => Err("ScrollToPrompt needs \"previous\" or \"next\"".to_owned()),
            },
            "ResizeScroll" => match arguments {
                [direction] if direction.eq_ignore_ascii_case("increase") => Ok(
                    MouseBehaviour::ResizeScroll(ResizeScrollDirection::Increase),
                ),
                [direction] if direction.eq_ignore_ascii_case("decrease") => Ok(
                    MouseBehaviour::ResizeScroll(ResizeScrollDirection::Decrease),
                ),
                _ => Err("ResizeScroll needs \"increase\" or \"decrease\"".to_owned()),
            },
            _ => Err(format!("Unknown mouse behaviour '{}'", name)),
        }
    }
    pub fn arguments(&self) -> Vec<String> {
        match self {
            MouseBehaviour::Scroll(lines) => vec![lines.to_string()],
            MouseBehaviour::ScrollColumns(columns) => vec![columns.to_string()],
            MouseBehaviour::ScrollToPrompt(PromptDirection::Previous) => {
                vec!["previous".to_owned()]
            },
            MouseBehaviour::ScrollToPrompt(PromptDirection::Next) => vec!["next".to_owned()],
            MouseBehaviour::ResizeScroll(ResizeScrollDirection::Increase) => {
                vec!["increase".to_owned()]
            },
            MouseBehaviour::ResizeScroll(ResizeScrollDirection::Decrease) => {
                vec!["decrease".to_owned()]
            },
            _ => vec![],
        }
    }
    pub fn needs_wheel(&self) -> bool {
        matches!(
            self,
            MouseBehaviour::Scroll(_) | MouseBehaviour::ScrollColumns(_)
        )
    }
    pub fn default_app_first(&self) -> bool {
        matches!(
            self,
            MouseBehaviour::Click
                | MouseBehaviour::Select
                | MouseBehaviour::FocusPane
                | MouseBehaviour::ContextMenu
                | MouseBehaviour::Scroll(_)
                | MouseBehaviour::ScrollColumns(_)
                | MouseBehaviour::ScrollToPrompt(_)
        )
    }
    pub fn kdl_text(&self) -> String {
        let arguments: Vec<String> = self
            .arguments()
            .into_iter()
            .map(|argument| match argument.parse::<usize>() {
                Ok(_) => argument,
                Err(_) => format!("\"{}\"", argument),
            })
            .collect();
        if arguments.is_empty() {
            self.name().to_owned()
        } else {
            format!("{} {}", self.name(), arguments.join(" "))
        }
    }
}

impl fmt::Display for MouseBehaviour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.kdl_text())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MouseBindingKind {
    Behaviour(MouseBehaviour),
    Actions(Vec<Action>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MouseBinding {
    pub kind: MouseBindingKind,
    pub app_first: Option<bool>,
}

impl MouseBinding {
    pub fn behaviour(behaviour: MouseBehaviour) -> Self {
        MouseBinding {
            kind: MouseBindingKind::Behaviour(behaviour),
            app_first: None,
        }
    }
    pub fn actions(actions: Vec<Action>) -> Self {
        MouseBinding {
            kind: MouseBindingKind::Actions(actions),
            app_first: None,
        }
    }
    pub fn with_app_first(mut self, app_first: Option<bool>) -> Self {
        self.app_first = app_first;
        self
    }
    pub fn default_app_first(&self) -> bool {
        match &self.kind {
            MouseBindingKind::Behaviour(behaviour) => behaviour.default_app_first(),
            MouseBindingKind::Actions(_) => false,
        }
    }
    pub fn app_first(&self) -> bool {
        self.app_first.unwrap_or_else(|| self.default_app_first())
    }
    pub fn as_behaviour(&self) -> Option<MouseBehaviour> {
        match &self.kind {
            MouseBindingKind::Behaviour(behaviour) => Some(*behaviour),
            MouseBindingKind::Actions(_) => None,
        }
    }
    pub fn action_texts(&self) -> Vec<String> {
        match &self.kind {
            MouseBindingKind::Behaviour(behaviour) => vec![behaviour.kdl_text()],
            MouseBindingKind::Actions(actions) => super::config_blocks::action_texts(actions),
        }
    }
}

pub fn mouse_button_of_event(event: &MouseEvent) -> Option<MouseButton> {
    if event.wheel_up {
        Some(MouseButton::ScrollUp)
    } else if event.wheel_down {
        Some(MouseButton::ScrollDown)
    } else if event.wheel_left {
        Some(MouseButton::ScrollLeft)
    } else if event.wheel_right {
        Some(MouseButton::ScrollRight)
    } else if event.left {
        Some(MouseButton::Left)
    } else if event.right {
        Some(MouseButton::Right)
    } else if event.middle {
        Some(MouseButton::Middle)
    } else {
        None
    }
}

pub fn mouse_modifiers_of_event(event: &MouseEvent) -> BTreeSet<KeyModifier> {
    let mut modifiers = BTreeSet::new();
    if event.ctrl {
        modifiers.insert(KeyModifier::Ctrl);
    }
    if event.alt {
        modifiers.insert(KeyModifier::Alt);
    }
    if event.shift {
        modifiers.insert(KeyModifier::Shift);
    }
    modifiers
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mousebinds(pub HashMap<InputMode, HashMap<MouseTrigger, MouseBinding>>);

impl fmt::Debug for Mousebinds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let stable_sorted: BTreeMap<&InputMode, BTreeMap<&MouseTrigger, &MouseBinding>> = self
            .0
            .iter()
            .map(|(mode, bindings)| (mode, bindings.iter().collect()))
            .collect();
        write!(f, "{:#?}", stable_sorted)
    }
}

impl Mousebinds {
    pub fn get(&self, mode: &InputMode, trigger: &MouseTrigger) -> Option<&MouseBinding> {
        self.0.get(mode).and_then(|bindings| bindings.get(trigger))
    }
    pub fn mode_bindings(&self, mode: &InputMode) -> BTreeMap<MouseTrigger, MouseBinding> {
        self.0
            .get(mode)
            .map(|bindings| {
                bindings
                    .iter()
                    .map(|(trigger, binding)| (trigger.clone(), binding.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn resolve(
        &self,
        mode: InputMode,
        button: MouseButton,
        modifiers: &BTreeSet<KeyModifier>,
        click_count: u8,
        on_frame: bool,
    ) -> Option<(MouseTrigger, &MouseBinding)> {
        let bindings = self.0.get(&mode)?;
        let place = if on_frame {
            MouseTarget::Frame
        } else {
            MouseTarget::Content
        };
        let mut modifier_sets = vec![modifiers.clone()];
        if modifiers.contains(&KeyModifier::Shift) {
            let mut without_shift = modifiers.clone();
            without_shift.remove(&KeyModifier::Shift);
            modifier_sets.push(without_shift);
        }
        let click_count = if button.is_wheel() {
            1
        } else {
            click_count.clamp(1, 3)
        };
        for modifiers in modifier_sets {
            for count in (1..=click_count).rev() {
                for target in [place, MouseTarget::Any] {
                    let trigger = MouseTrigger {
                        button,
                        modifiers: modifiers.clone(),
                        click_count: count,
                        target,
                    };
                    if let Some(binding) = bindings.get(&trigger) {
                        return Some((trigger, binding));
                    }
                }
            }
        }
        None
    }
    pub fn merge(&mut self, other: Mousebinds) {
        for (mode, bindings) in other.0 {
            self.0.entry(mode).or_default().extend(bindings);
        }
    }
}

#[cfg(test)]
#[path = "./unit/mousebinds_test.rs"]
mod mousebinds_test;
