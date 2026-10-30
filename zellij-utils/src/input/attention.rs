use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use super::config::ConfigError;
use crate::attention::AttentionStatus;

pub const DEFAULT_DONE_THRESHOLD_SECS: u64 = 10;
pub const DEFAULT_IDLE_ACTIVITY_SECS: u64 = 10;
pub const DEFAULT_IDLE_SILENCE_SECS: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum IdleRuleScope {
    #[default]
    All,
    Watched,
}

impl IdleRuleScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            IdleRuleScope::All => "all",
            IdleRuleScope::Watched => "watched",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "all" => Some(IdleRuleScope::All),
            "watched" => Some(IdleRuleScope::Watched),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum StartupCard {
    #[default]
    Attention,
    Always,
    Never,
}

impl StartupCard {
    pub fn as_str(&self) -> &'static str {
        match self {
            StartupCard::Attention => "attention",
            StartupCard::Always => "always",
            StartupCard::Never => "never",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "attention" => Some(StartupCard::Attention),
            "always" => Some(StartupCard::Always),
            "never" => Some(StartupCard::Never),
            _ => None,
        }
    }
}

pub const POPUP_STATUSES: [AttentionStatus; 3] = [
    AttentionStatus::NeedsYou,
    AttentionStatus::Failed,
    AttentionStatus::Done,
];

pub const BADGE_STATUSES: [AttentionStatus; 6] = [
    AttentionStatus::NeedsYou,
    AttentionStatus::Failed,
    AttentionStatus::Done,
    AttentionStatus::Working,
    AttentionStatus::Quiet,
    AttentionStatus::NewOutput,
];

pub const DEFAULT_BADGE_STATUSES: [AttentionStatus; 4] = [
    AttentionStatus::NeedsYou,
    AttentionStatus::Failed,
    AttentionStatus::Done,
    AttentionStatus::Working,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct StatusSet {
    bits: u8,
}

impl StatusSet {
    fn bit(status: AttentionStatus) -> u8 {
        1 << (status as u8)
    }
    pub fn of(statuses: &[AttentionStatus]) -> Self {
        statuses
            .iter()
            .fold(StatusSet::default(), |set, status| set.with(*status, true))
    }
    pub fn with(self, status: AttentionStatus, on: bool) -> Self {
        let bits = if on {
            self.bits | Self::bit(status)
        } else {
            self.bits & !Self::bit(status)
        };
        StatusSet { bits }
    }
    pub fn includes(&self, status: AttentionStatus) -> bool {
        self.bits & Self::bit(status) != 0
    }
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatusMap<T> {
    pub needs_you: Option<T>,
    pub failed: Option<T>,
    pub done: Option<T>,
    pub working: Option<T>,
    pub quiet: Option<T>,
    pub new_output: Option<T>,
}

impl<T> Default for StatusMap<T> {
    fn default() -> Self {
        StatusMap {
            needs_you: None,
            failed: None,
            done: None,
            working: None,
            quiet: None,
            new_output: None,
        }
    }
}

impl<T: Copy> StatusMap<T> {
    pub fn get(&self, status: AttentionStatus) -> Option<T> {
        match status {
            AttentionStatus::NeedsYou => self.needs_you,
            AttentionStatus::Failed => self.failed,
            AttentionStatus::Done => self.done,
            AttentionStatus::Working => self.working,
            AttentionStatus::Quiet => self.quiet,
            AttentionStatus::NewOutput => self.new_output,
            AttentionStatus::Idle => None,
        }
    }
    pub fn set(&mut self, status: AttentionStatus, value: Option<T>) {
        let slot = match status {
            AttentionStatus::NeedsYou => &mut self.needs_you,
            AttentionStatus::Failed => &mut self.failed,
            AttentionStatus::Done => &mut self.done,
            AttentionStatus::Working => &mut self.working,
            AttentionStatus::Quiet => &mut self.quiet,
            AttentionStatus::NewOutput => &mut self.new_output,
            AttentionStatus::Idle => return,
        };
        *slot = value;
    }
    pub fn merge(&self, other: &StatusMap<T>) -> Self {
        let mut merged = *self;
        for status in BADGE_STATUSES {
            if let Some(value) = other.get(status) {
                merged.set(status, Some(value));
            }
        }
        merged
    }
    pub fn is_empty(&self) -> bool {
        BADGE_STATUSES.iter().all(|status| self.get(*status).is_none())
    }
}

pub fn colour_text(colour: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", colour[0], colour[1], colour[2])
}

pub fn parse_colour(text: &str) -> Option<[u8; 3]> {
    let hex = text.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let hex = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.to_owned(),
        _ => return None,
    };
    let part = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    Some([part(0..2)?, part(2..4)?, part(4..6)?])
}

fn status_children<T: Copy>(
    node: &KdlNode,
    allowed: &[AttentionStatus],
    read: impl Fn(&KdlNode) -> Result<T, ConfigError>,
) -> Result<StatusMap<T>, ConfigError> {
    let mut map = StatusMap::default();
    let Some(children) = node.children() else {
        return Ok(map);
    };
    for child in children.nodes() {
        let name = child.name().value();
        let status = AttentionStatus::from_str(name)
            .filter(|status| allowed.contains(status))
            .ok_or_else(|| {
                let names: Vec<&str> = allowed.iter().map(|s| s.as_str()).collect();
                kdl_error(
                    format!(
                        "{} takes only {}, not \"{}\"",
                        node.name().value(),
                        names.join(", "),
                        name
                    ),
                    child,
                )
            })?;
        map.set(status, Some(read(child)?));
    }
    Ok(map)
}

fn colour_value(node: &KdlNode) -> Result<[u8; 3], ConfigError> {
    first_value(node)
        .and_then(|v| v.as_string())
        .and_then(parse_colour)
        .ok_or_else(|| {
            kdl_error(
                format!(
                    "{} must be a colour like \"#ff8c00\"",
                    node.name().value()
                ),
                node,
            )
        })
}

fn status_block<T: Copy>(
    name: &str,
    map: &StatusMap<T>,
    value: impl Fn(T) -> KdlValue,
) -> Option<KdlNode> {
    if map.is_empty() {
        return None;
    }
    let mut node = KdlNode::new(name);
    let mut children = KdlDocument::new();
    for status in BADGE_STATUSES {
        if let Some(v) = map.get(status) {
            let mut child = KdlNode::new(status.as_str());
            child.push(value(v));
            children.nodes_mut().push(child);
        }
    }
    node.set_children(children);
    Some(node)
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct AttentionConfig {
    pub frame_markers: Option<bool>,
    pub host_status: Option<bool>,
    pub tab_markers: Option<bool>,
    pub idle_rule: Option<IdleRuleScope>,
    pub done_threshold_secs: Option<u64>,
    pub idle_activity_secs: Option<u64>,
    pub idle_silence_secs: Option<u64>,
    pub startup_card: Option<StartupCard>,
    pub tray_icon: Option<bool>,
    pub desktop_notifications: StatusMap<bool>,
    pub tray_badge: StatusMap<bool>,
    pub tray_badge_colors: StatusMap<[u8; 3]>,
    pub notification_details: Option<bool>,
}

fn kdl_error(message: String, node: &KdlNode) -> ConfigError {
    ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
}

fn first_value(node: &KdlNode) -> Option<&KdlValue> {
    node.entries()
        .iter()
        .find(|e| e.name().is_none())
        .map(|e| e.value())
}

fn bool_value(node: &KdlNode) -> Result<bool, ConfigError> {
    first_value(node).and_then(|v| v.as_bool()).ok_or_else(|| {
        kdl_error(
            format!("{} must be true or false", node.name().value()),
            node,
        )
    })
}

fn secs_value(node: &KdlNode) -> Result<u64, ConfigError> {
    match first_value(node).and_then(|v| v.as_i64()) {
        Some(v) if v >= 0 => Ok(v as u64),
        _ => Err(kdl_error(
            format!("{} must be a whole number of seconds", node.name().value()),
            node,
        )),
    }
}

impl AttentionConfig {
    pub fn frame_markers(&self) -> bool {
        self.frame_markers.unwrap_or(true)
    }
    pub fn host_status(&self) -> bool {
        self.host_status.unwrap_or(true)
    }
    pub fn tab_markers(&self) -> bool {
        self.tab_markers.unwrap_or(true)
    }
    pub fn idle_rule(&self) -> IdleRuleScope {
        self.idle_rule.unwrap_or_default()
    }
    pub fn done_threshold_secs(&self) -> u64 {
        self.done_threshold_secs
            .unwrap_or(DEFAULT_DONE_THRESHOLD_SECS)
    }
    pub fn idle_activity_secs(&self) -> u64 {
        self.idle_activity_secs
            .unwrap_or(DEFAULT_IDLE_ACTIVITY_SECS)
    }
    pub fn idle_silence_secs(&self) -> u64 {
        self.idle_silence_secs.unwrap_or(DEFAULT_IDLE_SILENCE_SECS)
    }
    pub fn startup_card(&self) -> StartupCard {
        self.startup_card.unwrap_or_default()
    }
    pub fn tray_icon(&self) -> bool {
        self.tray_icon.unwrap_or(true)
    }
    pub fn desktop_notifications(&self) -> StatusSet {
        POPUP_STATUSES
            .iter()
            .fold(StatusSet::default(), |set, status| {
                set.with(
                    *status,
                    self.desktop_notifications.get(*status).unwrap_or(true),
                )
            })
    }
    pub fn tray_badge(&self) -> StatusSet {
        BADGE_STATUSES
            .iter()
            .fold(StatusSet::default(), |set, status| {
                let default = DEFAULT_BADGE_STATUSES.contains(status);
                set.with(*status, self.tray_badge.get(*status).unwrap_or(default))
            })
    }
    pub fn tray_badge_color(&self, status: AttentionStatus) -> Option<[u8; 3]> {
        self.tray_badge_colors.get(status)
    }
    pub fn notification_details(&self) -> bool {
        self.notification_details.unwrap_or(true)
    }

    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut config = AttentionConfig::default();
        let Some(children) = kdl.children() else {
            return Ok(config);
        };
        for node in children.nodes() {
            match node.name().value() {
                "frame_markers" => config.frame_markers = Some(bool_value(node)?),
                "host_status" => config.host_status = Some(bool_value(node)?),
                "tab_markers" => config.tab_markers = Some(bool_value(node)?),
                "idle_rule" => {
                    let value = first_value(node).and_then(|v| v.as_string());
                    config.idle_rule =
                        Some(value.and_then(IdleRuleScope::from_str).ok_or_else(|| {
                            kdl_error("idle_rule must be \"all\" or \"watched\"".to_owned(), node)
                        })?);
                },
                "done_threshold_secs" => config.done_threshold_secs = Some(secs_value(node)?),
                "idle_activity_secs" => config.idle_activity_secs = Some(secs_value(node)?),
                "idle_silence_secs" => config.idle_silence_secs = Some(secs_value(node)?),
                "startup_card" => {
                    let value = first_value(node).and_then(|v| v.as_string());
                    config.startup_card =
                        Some(value.and_then(StartupCard::from_str).ok_or_else(|| {
                            kdl_error(
                                "startup_card must be \"attention\", \"always\" or \"never\""
                                    .to_owned(),
                                node,
                            )
                        })?);
                },
                "tray_icon" => config.tray_icon = Some(bool_value(node)?),
                "notification_details" => config.notification_details = Some(bool_value(node)?),
                "desktop_notifications" => {
                    config.desktop_notifications =
                        status_children(node, &POPUP_STATUSES, bool_value)?;
                },
                "tray_badge" => {
                    config.tray_badge = status_children(node, &BADGE_STATUSES, bool_value)?;
                },
                "tray_badge_colors" => {
                    config.tray_badge_colors =
                        status_children(node, &BADGE_STATUSES, colour_value)?;
                },
                other => {
                    return Err(kdl_error(
                        format!("Unknown attention setting \"{}\"", other),
                        node,
                    ))
                },
            }
        }
        Ok(config)
    }

    pub fn to_kdl(&self) -> Option<KdlNode> {
        if self == &AttentionConfig::default() {
            return None;
        }
        let mut node = KdlNode::new("attention");
        let mut children = KdlDocument::new();
        let mut push = |name: &str, value: KdlValue| {
            let mut child = KdlNode::new(name);
            child.push(value);
            children.nodes_mut().push(child);
        };
        if let Some(v) = self.frame_markers {
            push("frame_markers", KdlValue::Bool(v));
        }
        if let Some(v) = self.host_status {
            push("host_status", KdlValue::Bool(v));
        }
        if let Some(v) = self.tab_markers {
            push("tab_markers", KdlValue::Bool(v));
        }
        if let Some(v) = self.idle_rule {
            push("idle_rule", KdlValue::String(v.as_str().to_owned()));
        }
        if let Some(v) = self.done_threshold_secs {
            push("done_threshold_secs", KdlValue::Base10(v as i64));
        }
        if let Some(v) = self.idle_activity_secs {
            push("idle_activity_secs", KdlValue::Base10(v as i64));
        }
        if let Some(v) = self.idle_silence_secs {
            push("idle_silence_secs", KdlValue::Base10(v as i64));
        }
        if let Some(v) = self.startup_card {
            push("startup_card", KdlValue::String(v.as_str().to_owned()));
        }
        if let Some(v) = self.tray_icon {
            push("tray_icon", KdlValue::Bool(v));
        }
        if let Some(v) = self.notification_details {
            push("notification_details", KdlValue::Bool(v));
        }
        let blocks = [
            status_block(
                "desktop_notifications",
                &self.desktop_notifications,
                KdlValue::Bool,
            ),
            status_block("tray_badge", &self.tray_badge, KdlValue::Bool),
            status_block("tray_badge_colors", &self.tray_badge_colors, |colour| {
                KdlValue::String(colour_text(colour))
            }),
        ];
        children.nodes_mut().extend(blocks.into_iter().flatten());
        node.set_children(children);
        Some(node)
    }

    pub fn merge(&self, other: AttentionConfig) -> Self {
        AttentionConfig {
            frame_markers: other.frame_markers.or(self.frame_markers),
            host_status: other.host_status.or(self.host_status),
            tab_markers: other.tab_markers.or(self.tab_markers),
            idle_rule: other.idle_rule.or(self.idle_rule),
            done_threshold_secs: other.done_threshold_secs.or(self.done_threshold_secs),
            idle_activity_secs: other.idle_activity_secs.or(self.idle_activity_secs),
            idle_silence_secs: other.idle_silence_secs.or(self.idle_silence_secs),
            startup_card: other.startup_card.or(self.startup_card),
            tray_icon: other.tray_icon.or(self.tray_icon),
            desktop_notifications: self
                .desktop_notifications
                .merge(&other.desktop_notifications),
            tray_badge: self.tray_badge.merge(&other.tray_badge),
            tray_badge_colors: self.tray_badge_colors.merge(&other.tray_badge_colors),
            notification_details: other.notification_details.or(self.notification_details),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<AttentionConfig, ConfigError> {
        let document: KdlDocument = text.parse().unwrap();
        AttentionConfig::from_kdl(document.get("attention").unwrap())
    }

    #[test]
    fn defaults_apply_when_empty() {
        let config = parse("attention {\n}").unwrap();
        assert!(config.frame_markers());
        assert!(config.host_status());
        assert!(config.tab_markers());
        assert_eq!(config.idle_rule(), IdleRuleScope::All);
        assert_eq!(config.done_threshold_secs(), 10);
        assert_eq!(config.idle_activity_secs(), 10);
        assert_eq!(config.idle_silence_secs(), 3);
        assert_eq!(config.startup_card(), StartupCard::Attention);
        assert!(config.tray_icon());
        assert_eq!(config.desktop_notifications(), StatusSet::of(&POPUP_STATUSES));
        assert_eq!(config.tray_badge(), StatusSet::of(&DEFAULT_BADGE_STATUSES));
        assert_eq!(config.tray_badge_color(AttentionStatus::Failed), None);
        assert!(config.notification_details());
        assert_eq!(config.to_kdl(), None);
    }

    #[test]
    fn full_block_round_trips() {
        let config = parse(
            r##"
            attention {
                frame_markers false
                host_status false
                tab_markers false
                idle_rule "watched"
                done_threshold_secs 5
                idle_activity_secs 20
                idle_silence_secs 4
                startup_card "never"
                tray_icon false
                desktop_notifications {
                    needs_you true
                    done false
                }
                tray_badge {
                    working false
                    quiet true
                }
                tray_badge_colors {
                    failed "#112233"
                }
                notification_details false
            }
            "##,
        )
        .unwrap();
        assert!(!config.frame_markers());
        assert!(!config.host_status());
        assert!(!config.tab_markers());
        assert_eq!(config.idle_rule(), IdleRuleScope::Watched);
        assert_eq!(config.done_threshold_secs(), 5);
        assert_eq!(config.startup_card(), StartupCard::Never);
        assert!(!config.tray_icon());
        assert_eq!(
            config.desktop_notifications(),
            StatusSet::of(&[AttentionStatus::NeedsYou, AttentionStatus::Failed])
        );
        assert_eq!(
            config.tray_badge(),
            StatusSet::of(&[
                AttentionStatus::NeedsYou,
                AttentionStatus::Failed,
                AttentionStatus::Done,
                AttentionStatus::Quiet
            ])
        );
        assert_eq!(
            config.tray_badge_color(AttentionStatus::Failed),
            Some([0x11, 0x22, 0x33])
        );
        assert!(!config.notification_details());
        let reparsed = AttentionConfig::from_kdl(&config.to_kdl().unwrap()).unwrap();
        assert_eq!(config, reparsed);
    }

    #[test]
    fn bad_values_are_rejected() {
        assert!(parse("attention {\n idle_rule \"some\"\n}").is_err());
        assert!(parse("attention {\n frame_markers 3\n}").is_err());
        assert!(parse("attention {\n idle_silence_secs -1\n}").is_err());
        assert!(parse("attention {\n colour true\n}").is_err());
        assert!(parse("attention {\n startup_card \"sometimes\"\n}").is_err());
        assert!(parse("attention {\n desktop_notifications {\n working true\n }\n}").is_err());
        assert!(parse("attention {\n desktop_notifications {\n done \"no\"\n }\n}").is_err());
        assert!(parse("attention {\n tray_badge {\n idle true\n }\n}").is_err());
        assert!(parse("attention {\n tray_badge_colors {\n done \"red\"\n }\n}").is_err());
        assert!(parse("attention {\n tray_icon \"no\"\n}").is_err());
    }

    #[test]
    fn a_status_set_holds_each_status_on_its_own() {
        let set = StatusSet::of(&[AttentionStatus::NeedsYou, AttentionStatus::Done]);
        assert!(set.includes(AttentionStatus::NeedsYou));
        assert!(set.includes(AttentionStatus::Done));
        assert!(!set.includes(AttentionStatus::Failed));
        let set = set.with(AttentionStatus::NeedsYou, false);
        assert!(!set.includes(AttentionStatus::NeedsYou));
        assert!(set.with(AttentionStatus::Done, false).is_empty());
        assert_eq!(parse_colour("#FF8c00"), Some([255, 140, 0]));
        assert_eq!(parse_colour("ff8c00"), None);
        assert_eq!(parse_colour("#ff8c0"), None);
        assert_eq!(parse_colour("#f80"), Some([255, 136, 0]));
        assert_eq!(colour_text([255, 140, 0]), "#ff8c00");
    }

    #[test]
    fn merge_prefers_the_other_config() {
        let base = parse("attention {\n host_status false\n idle_silence_secs 9\n}").unwrap();
        let other = parse("attention {\n idle_silence_secs 2\n}").unwrap();
        let merged = base.merge(other);
        assert_eq!(merged.host_status, Some(false));
        assert_eq!(merged.idle_silence_secs, Some(2));
        let base = parse("attention {\n tray_badge {\n done false\n working false\n }\n}").unwrap();
        let other = parse("attention {\n tray_badge {\n done true\n }\n}").unwrap();
        let merged = base.merge(other);
        assert_eq!(merged.tray_badge.done, Some(true));
        assert_eq!(merged.tray_badge.working, Some(false));
    }
}
