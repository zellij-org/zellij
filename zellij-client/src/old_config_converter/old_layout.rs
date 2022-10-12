// This is a converter from the old yaml layout to the new KDL layout.
//
// It is supposed to be mostly self containing - please refrain from adding to it, importing
// from it or changing it
use super::old_config::{config_yaml_to_config_kdl, OldConfigFromYaml, OldRunCommand};
use serde::{Deserialize, Serialize};
use std::vec::Vec;
use std::{fmt, path::PathBuf};
use url::Url;

fn pane_line(
    pane_name: Option<&String>,
    split_size: Option<OldSplitSize>,
    focus: Option<bool>,
    borderless: bool,
) -> String {
    let mut pane_line = format!("pane");
    if let Some(pane_name) = pane_name {
        // we use debug print here so that quotes and backslashes will be escaped
        pane_line.push_str(&format!(" name={:?}", pane_name));
    }
    if let Some(split_size) = split_size {
        pane_line.push_str(&format!(" size={}", split_size));
    }
    if let Some(focus) = focus {
        pane_line.push_str(&format!(" focus={}", focus));
    }
    if borderless {
        pane_line.push_str(" borderless=true");
    }
    pane_line
}

fn tab_line(
    pane_name: Option<&String>,
    split_size: Option<OldSplitSize>,
    focus: Option<bool>,
    borderless: bool,
) -> String {
    let mut pane_line = format!("tab");
    if let Some(pane_name) = pane_name {
        // we use debug print here so that quotes and backslashes will be escaped
        pane_line.push_str(&format!(" name={:?}", pane_name));
    }
    if let Some(split_size) = split_size {
        pane_line.push_str(&format!(" size={}", split_size));
    }
    if let Some(focus) = focus {
        pane_line.push_str(&format!(" focus={}", focus));
    }
    if borderless {
        pane_line.push_str(" borderless=true");
    }
    pane_line
}

fn pane_line_with_children(
    pane_name: Option<&String>,
    split_size: Option<OldSplitSize>,
    focus: Option<bool>,
    borderless: bool,
    split_direction: OldDirection,
) -> String {
    let mut pane_line = format!("pane");
    if let Some(pane_name) = pane_name {
        // we use debug print here so that quotes and backslashes will be escaped
        pane_line.push_str(&format!(" name={:?}", pane_name));
    }
    if let Some(split_size) = split_size {
        pane_line.push_str(&format!(" size={}", split_size));
    }
    if let Some(focus) = focus {
        pane_line.push_str(&format!(" focus={}", focus));
    }
    pane_line.push_str(&format!(" split_direction=\"{}\"", split_direction));
    if borderless {
        pane_line.push_str(" borderless=true");
    }
    pane_line
}

fn pane_command_line(
    pane_name: Option<&String>,
    split_size: Option<OldSplitSize>,
    focus: Option<bool>,
    borderless: bool,
    command: &PathBuf,
) -> String {
    let mut pane_line = format!("pane command={:?}", command);
    if let Some(pane_name) = pane_name {
        // we use debug print here so that quotes and backslashes will be escaped
        pane_line.push_str(&format!(" name={:?}", pane_name));
    }
    if let Some(split_size) = split_size {
        pane_line.push_str(&format!(" size={}", split_size));
    }
    if let Some(focus) = focus {
        pane_line.push_str(&format!(" focus={}", focus));
    }
    if borderless {
        pane_line.push_str(" borderless=true");
    }
    pane_line
}

fn tab_line_with_children(
    pane_name: Option<&String>,
    split_size: Option<OldSplitSize>,
    focus: Option<bool>,
    borderless: bool,
    split_direction: OldDirection,
) -> String {
    let mut pane_line = format!("tab");
    if let Some(pane_name) = pane_name {
        // we use debug print here so that quotes and backslashes will be escaped
        pane_line.push_str(&format!(" name={:?}", pane_name));
    }
    if let Some(split_size) = split_size {
        pane_line.push_str(&format!(" size={}", split_size));
    }
    if let Some(focus) = focus {
        pane_line.push_str(&format!(" focus={}", focus));
    }
    pane_line.push_str(&format!(" split_direction=\"{}\"", split_direction));
    if borderless {
        pane_line.push_str(" borderless=true");
    }
    pane_line
}

fn stringify_template(
    template: &OldLayoutTemplate,
    indentation: String,
    has_no_tabs: bool,
    is_base: bool,
) -> String {
    let mut stringified = if is_base {
        String::new()
    } else {
        String::from("\n")
    };
    if is_base && !template.parts.is_empty() && template.direction == OldDirection::Vertical {
        // we don't support specifying the split direction in the layout node
        // eg. layout split_direction="Vertical" { .. }  <== this is not supported!!
        // so we need to add a child wrapper with the split direction instead:
        // layout {
        //     pane split_direction="Vertical" { .. }
        // }
        let child_indentation = format!("{}    ", &indentation);
        stringified.push_str(&stringify_template(
            template,
            child_indentation,
            has_no_tabs,
            false,
        ));
    } else if !template.parts.is_empty() {
        if !is_base {
            stringified.push_str(&format!(
                "{}{} {{",
                indentation,
                pane_line_with_children(
                    template.pane_name.as_ref(),
                    template.split_size,
                    template.focus,
                    template.borderless,
                    template.direction
                )
            ));
        }
        for part in &template.parts {
            let child_indentation = format!("{}    ", &indentation);
            stringified.push_str(&stringify_template(
                &part,
                child_indentation,
                has_no_tabs,
                false,
            ));
        }
        if !is_base {
            stringified.push_str(&format!("\n{}}}", indentation));
        }
    } else if template.body && !has_no_tabs {
        stringified.push_str(&format!("{}children", indentation));
    } else {
        match template.run.as_ref() {
            Some(OldRunFromYaml::Plugin(plugin_from_yaml)) => {
                stringified.push_str(&format!(
                    "{}{} {{\n",
                    &indentation,
                    pane_line(
                        template.pane_name.as_ref(),
                        template.split_size,
                        template.focus,
                        template.borderless
                    )
                ));
                stringified.push_str(&format!(
                    "{}    plugin location=\"{}\"\n",
                    &indentation, plugin_from_yaml.location
                ));
                stringified.push_str(&format!("{}}}", &indentation));
            },
            Some(OldRunFromYaml::Command(command_from_yaml)) => {
                stringified.push_str(&format!(
                    "{}{}",
                    &indentation,
                    &pane_command_line(
                        template.pane_name.as_ref(),
                        template.split_size,
                        template.focus,
                        template.borderless,
                        &command_from_yaml.command
                    )
                ));
                if let Some(cwd) = command_from_yaml.cwd.as_ref() {
                    stringified.push_str(&format!(" cwd={:?}", cwd));
                }
                if !command_from_yaml.args.is_empty() {
                    stringified.push_str(" {\n");
                    stringified.push_str(&format!(
                        "{}    args {}\n",
                        &indentation,
                        command_from_yaml
                            .args
                            .iter()
                            // we use debug print here so that quotes and backslashes will be
                            // escaped
                            .map(|s| format!("{:?}", s))
                            .collect::<Vec<String>>()
                            .join(" ")
                    ));
                    stringified.push_str(&format!("{}}}", &indentation));
                }
            },
            None => {
                stringified.push_str(&format!(
                    "{}{}",
                    &indentation,
                    pane_line(
                        template.pane_name.as_ref(),
                        template.split_size,
                        template.focus,
                        template.borderless
                    )
                ));
            },
        };
    }
    stringified
}

fn stringify_tabs(tabs: Vec<OldTabLayout>) -> String {
    let mut stringified = String::new();
    for tab in tabs {
        let child_indentation = String::from("    ");
        if !tab.parts.is_empty() {
            stringified.push_str(&format!(
                "\n{}{} {{",
                child_indentation,
                tab_line_with_children(
                    tab.pane_name.as_ref(),
                    tab.split_size,
                    tab.focus,
                    tab.borderless,
                    tab.direction
                )
            ));
            let tab_template = OldLayoutTemplate::from(tab);
            stringified.push_str(&stringify_template(
                &tab_template,
                child_indentation.clone(),
                true,
                true,
            ));
            stringified.push_str(&format!("\n{}}}", child_indentation));
        } else {
            stringified.push_str(&format!(
                "\n{}{}",
                child_indentation,
                tab_line(
                    tab.pane_name.as_ref(),
                    tab.split_size,
                    tab.focus,
                    tab.borderless
                )
            ));
        }
    }
    stringified
}

pub fn layout_yaml_to_layout_kdl(raw_yaml_layout: &str) -> Result<String, String> {
    // returns the raw kdl config
    let layout_from_yaml: OldLayoutFromYamlIntermediate = serde_yaml::from_str(raw_yaml_layout)
        .map_err(|e| format!("Failed to parse yaml: {:?}", e))?;
    let mut kdl_layout = String::new();
    kdl_layout.push_str("layout {");
    let template = layout_from_yaml.template;
    let tabs = layout_from_yaml.tabs;
    let has_no_tabs = tabs.is_empty()
        || tabs.len() == 1 && tabs.get(0).map(|t| t.parts.is_empty()).unwrap_or(false);
    if has_no_tabs {
        let indentation = String::from("");
        kdl_layout.push_str(&stringify_template(
            &template,
            indentation,
            has_no_tabs,
            true,
        ));
    } else {
        kdl_layout.push_str("\n    default_tab_template {");
        let indentation = String::from("    ");
        kdl_layout.push_str(&stringify_template(
            &template,
            indentation,
            has_no_tabs,
            true,
        ));
        kdl_layout.push_str("\n    }");
        kdl_layout.push_str(&stringify_tabs(tabs));
    }
    kdl_layout.push_str("\n}");
    let layout_config = config_yaml_to_config_kdl(raw_yaml_layout, true)?;
    if let Some(session_name) = layout_from_yaml.session.name {
        // we use debug print here so that quotes and backslashes will be escaped
        kdl_layout.push_str(&format!("\nsession_name {:?}", session_name));
        if let Some(attach_to_session) = layout_from_yaml.session.attach {
            kdl_layout.push_str(&format!("\nattach_to_session {}", attach_to_session));
        }
    }
    if !layout_config.is_empty() {
        kdl_layout.push('\n');
    }
    kdl_layout.push_str(&layout_config);
    Ok(kdl_layout)
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub enum OldDirection {
    #[serde(alias = "horizontal")]
    Horizontal,
    #[serde(alias = "vertical")]
    Vertical,
}

impl fmt::Display for OldDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Horizontal => write!(f, "Horizontal"),
            Self::Vertical => write!(f, "Vertical"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum OldSplitSize {
    #[serde(alias = "percent")]
    Percent(u64), // 1 to 100
    #[serde(alias = "fixed")]
    Fixed(usize), // An absolute number of columns or rows
}

impl fmt::Display for OldSplitSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            Self::Percent(percent) => write!(f, "\"{}%\"", percent),
            Self::Fixed(fixed_size) => write!(f, "{}", fixed_size),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum OldRunFromYaml {
    #[serde(rename = "plugin")]
    Plugin(OldRunPluginFromYaml),
    #[serde(rename = "command")]
    Command(OldRunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct OldRunPluginFromYaml {
    #[serde(default)]
    pub _allow_exec_host_cmd: bool,
    pub location: Url,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct OldLayoutFromYamlIntermediate {
    #[serde(default)]
    pub template: OldLayoutTemplate,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub tabs: Vec<OldTabLayout>,
    #[serde(default)]
    pub session: OldSessionFromYaml,
    #[serde(flatten)]
    pub config: Option<OldConfigFromYaml>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(default)]
pub struct OldLayoutFromYaml {
    #[serde(default)]
    pub session: OldSessionFromYaml,
    #[serde(default)]
    pub template: OldLayoutTemplate,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub tabs: Vec<OldTabLayout>,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct OldSessionFromYaml {
    pub name: Option<String>,
    #[serde(default = "default_as_some_true")]
    pub attach: Option<bool>,
}

fn default_as_some_true() -> Option<bool> {
    Some(true)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct OldLayoutTemplate {
    pub direction: OldDirection,
    #[serde(default)]
    pub pane_name: Option<String>,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub parts: Vec<OldLayoutTemplate>,
    #[serde(default)]
    pub body: bool,
    pub split_size: Option<OldSplitSize>,
    pub focus: Option<bool>,
    pub run: Option<OldRunFromYaml>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct OldTabLayout {
    #[serde(default)]
    pub direction: OldDirection,
    pub pane_name: Option<String>,
    #[serde(default)]
    pub borderless: bool,
    #[serde(default)]
    pub parts: Vec<OldTabLayout>,
    pub split_size: Option<OldSplitSize>,
    #[serde(default)]
    pub name: String,
    pub focus: Option<bool>,
    pub run: Option<OldRunFromYaml>,
}

impl From<OldTabLayout> for OldLayoutTemplate {
    fn from(old_tab_layout: OldTabLayout) -> Self {
        OldLayoutTemplate {
            direction: old_tab_layout.direction,
            pane_name: old_tab_layout.pane_name.clone(),
            borderless: old_tab_layout.borderless,
            parts: old_tab_layout.parts.iter().map(|o| o.into()).collect(),
            split_size: old_tab_layout.split_size,
            focus: old_tab_layout.focus,
            run: old_tab_layout.run.clone(),
            body: false,
        }
    }
}

impl From<&OldTabLayout> for OldLayoutTemplate {
    fn from(old_tab_layout: &OldTabLayout) -> Self {
        OldLayoutTemplate {
            direction: old_tab_layout.direction,
            pane_name: old_tab_layout.pane_name.clone(),
            borderless: old_tab_layout.borderless,
            parts: old_tab_layout.parts.iter().map(|o| o.into()).collect(),
            split_size: old_tab_layout.split_size,
            focus: old_tab_layout.focus,
            run: old_tab_layout.run.clone(),
            body: false,
        }
    }
}

impl From<&mut OldTabLayout> for OldLayoutTemplate {
    fn from(old_tab_layout: &mut OldTabLayout) -> Self {
        OldLayoutTemplate {
            direction: old_tab_layout.direction,
            pane_name: old_tab_layout.pane_name.clone(),
            borderless: old_tab_layout.borderless,
            parts: old_tab_layout.parts.iter().map(|o| o.into()).collect(),
            split_size: old_tab_layout.split_size,
            focus: old_tab_layout.focus,
            run: old_tab_layout.run.clone(),
            body: false,
        }
    }
}

impl From<OldLayoutFromYamlIntermediate> for OldLayoutFromYaml {
    fn from(layout_from_yaml_intermediate: OldLayoutFromYamlIntermediate) -> Self {
        Self {
            template: layout_from_yaml_intermediate.template,
            borderless: layout_from_yaml_intermediate.borderless,
            tabs: layout_from_yaml_intermediate.tabs,
            session: layout_from_yaml_intermediate.session,
        }
    }
}

impl From<OldLayoutFromYaml> for OldLayoutFromYamlIntermediate {
    fn from(layout_from_yaml: OldLayoutFromYaml) -> Self {
        Self {
            template: layout_from_yaml.template,
            borderless: layout_from_yaml.borderless,
            tabs: layout_from_yaml.tabs,
            config: None,
            session: layout_from_yaml.session,
        }
    }
}

impl Default for OldLayoutFromYamlIntermediate {
    fn default() -> Self {
        OldLayoutFromYaml::default().into()
    }
}

impl Default for OldLayoutTemplate {
    fn default() -> Self {
        Self {
            direction: OldDirection::Horizontal,
            pane_name: None,
            body: false,
            borderless: false,
            parts: vec![OldLayoutTemplate {
                direction: OldDirection::Horizontal,
                pane_name: None,
                body: true,
                borderless: false,
                split_size: None,
                focus: None,
                run: None,
                parts: vec![],
            }],
            split_size: None,
            focus: None,
            run: None,
        }
    }
}

impl Default for OldDirection {
    fn default() -> Self {
        OldDirection::Horizontal
    }
}

// The unit test location.
#[path = "./unit/convert_layout_tests.rs"]
#[cfg(test)]
mod convert_layout_test;
