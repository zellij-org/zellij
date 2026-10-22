use kdl::{KdlDocument, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use super::config::ConfigError;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DesktopNotifications {
    NeedsYou,
    NeedsYouFailed,
    #[default]
    NeedsYouFailedDone,
    None,
}

impl DesktopNotifications {
    pub fn as_str(&self) -> &'static str {
        match self {
            DesktopNotifications::NeedsYou => "needs_you",
            DesktopNotifications::NeedsYouFailed => "needs_you_failed",
            DesktopNotifications::NeedsYouFailedDone => "needs_you_failed_done",
            DesktopNotifications::None => "none",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "needs_you" => Some(DesktopNotifications::NeedsYou),
            "needs_you_failed" => Some(DesktopNotifications::NeedsYouFailed),
            "needs_you_failed_done" => Some(DesktopNotifications::NeedsYouFailedDone),
            "none" => Some(DesktopNotifications::None),
            _ => None,
        }
    }
    pub fn includes(&self, status: crate::attention::AttentionStatus) -> bool {
        use crate::attention::AttentionStatus;
        match status {
            AttentionStatus::NeedsYou => *self != DesktopNotifications::None,
            AttentionStatus::Failed => matches!(
                self,
                DesktopNotifications::NeedsYouFailed | DesktopNotifications::NeedsYouFailedDone
            ),
            AttentionStatus::Done => *self == DesktopNotifications::NeedsYouFailedDone,
            _ => false,
        }
    }
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
    pub desktop_notifications: Option<DesktopNotifications>,
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
    pub fn desktop_notifications(&self) -> DesktopNotifications {
        self.desktop_notifications.unwrap_or_default()
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
                    let value = first_value(node).and_then(|v| v.as_string());
                    config.desktop_notifications = Some(
                        value
                            .and_then(DesktopNotifications::from_str)
                            .ok_or_else(|| {
                                kdl_error(
                                    "desktop_notifications must be \"needs_you\", \"needs_you_failed\", \"needs_you_failed_done\" or \"none\""
                                        .to_owned(),
                                    node,
                                )
                            })?,
                    );
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
        if let Some(v) = self.desktop_notifications {
            push(
                "desktop_notifications",
                KdlValue::String(v.as_str().to_owned()),
            );
        }
        if let Some(v) = self.notification_details {
            push("notification_details", KdlValue::Bool(v));
        }
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
            desktop_notifications: other.desktop_notifications.or(self.desktop_notifications),
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
        assert_eq!(
            config.desktop_notifications(),
            DesktopNotifications::NeedsYouFailedDone
        );
        assert!(config.notification_details());
        assert_eq!(config.to_kdl(), None);
    }

    #[test]
    fn full_block_round_trips() {
        let config = parse(
            r#"
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
                desktop_notifications "needs_you_failed_done"
                notification_details false
            }
            "#,
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
            DesktopNotifications::NeedsYouFailedDone
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
        assert!(parse("attention {\n desktop_notifications \"done\"\n}").is_err());
        assert!(parse("attention {\n tray_icon \"no\"\n}").is_err());
    }

    #[test]
    fn desktop_notification_choices_cover_the_counted_statuses() {
        use crate::attention::AttentionStatus;
        let default = DesktopNotifications::default();
        assert!(default.includes(AttentionStatus::NeedsYou));
        assert!(default.includes(AttentionStatus::Failed));
        assert!(default.includes(AttentionStatus::Done));
        assert!(!DesktopNotifications::NeedsYouFailed.includes(AttentionStatus::Done));
        assert!(!default.includes(AttentionStatus::Quiet));
        assert!(!default.includes(AttentionStatus::NewOutput));
        assert!(!default.includes(AttentionStatus::Working));
        assert!(DesktopNotifications::NeedsYou.includes(AttentionStatus::NeedsYou));
        assert!(!DesktopNotifications::NeedsYou.includes(AttentionStatus::Failed));
        assert!(DesktopNotifications::NeedsYouFailedDone.includes(AttentionStatus::Done));
        assert!(!DesktopNotifications::None.includes(AttentionStatus::NeedsYou));
    }

    #[test]
    fn merge_prefers_the_other_config() {
        let base = parse("attention {\n host_status false\n idle_silence_secs 9\n}").unwrap();
        let other = parse("attention {\n idle_silence_secs 2\n}").unwrap();
        let merged = base.merge(other);
        assert_eq!(merged.host_status, Some(false));
        assert_eq!(merged.idle_silence_secs, Some(2));
    }
}
