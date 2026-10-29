use std::collections::BTreeMap;
use std::sync::OnceLock;

use zellij_tile::prelude::*;

const DEFAULT_CONFIG: &str = include_str!("../../../zellij-utils/assets/config/default.kdl");

fn section_for(path: &[String]) -> Option<SettingSection> {
    let path: Vec<&str> = path.iter().map(|name| name.as_str()).collect();
    match path.as_slice() {
        [] => Some(SettingSection::TopLevel),
        ["ui", "pane_frames"] => Some(SettingSection::PaneFrames),
        ["web_client"] => Some(SettingSection::WebClient),
        ["terminal"] => Some(SettingSection::Terminal),
        ["terminal", "theme"] => Some(SettingSection::TerminalTheme),
        ["attention"] => Some(SettingSection::Attention),
        _ => None,
    }
}

fn first_word(text: &str) -> &str {
    text.split(|c: char| c.is_whitespace() || c == '{')
        .next()
        .unwrap_or("")
}

fn looks_like_a_value(line: &str) -> bool {
    let rest = line[first_word(line).len()..].trim();
    let token = rest.split_whitespace().next().unwrap_or("");
    rest.is_empty()
        || rest.starts_with(['"', '{', '-'])
        || rest.starts_with(|c: char| c.is_ascii_digit())
        || token == "true"
        || token == "false"
}

fn key_named(path: &[String], name: &str) -> Option<SettingKey> {
    let section = section_for(path)?;
    SettingKey::all()
        .into_iter()
        .find(|key| key.section() == section && key.kdl_name() == name)
}

fn is_heading(line: &str) -> bool {
    line.ends_with(':') && line.split_whitespace().count() <= 3
}

fn ends_a_sentence(line: &str) -> bool {
    line.ends_with(['.', '!', '?', ':', ';', ',', ')', '"', '`'])
}

fn starts_own_line(line: &str, previous: Option<&str>) -> bool {
    if previous.map(is_heading).unwrap_or(true) {
        return true;
    }
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with('(') {
        return true;
    }
    match line.split_once(':') {
        Some((head, _)) => {
            head.split_whitespace().count() <= 3
                && head
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                && head.chars().all(|c| c.is_alphabetic() || c == ' ')
        },
        None => false,
    }
}

fn is_restart_note(line: &str) -> bool {
    line.trim() == "(Requires restart)"
}

fn shown_elsewhere(line: &str) -> bool {
    let line = line.trim();
    is_restart_note(line) || line.starts_with("Default:")
}

fn is_flag_value(line: &str) -> bool {
    let value = line
        .trim()
        .trim_start_matches(['-', '*'])
        .trim()
        .to_lowercase();
    value.starts_with("true") || value.starts_with("false")
}

fn without_repeats(lines: &[String]) -> Vec<String> {
    let lines: Vec<String> = lines
        .iter()
        .filter(|line| !shown_elsewhere(line))
        .cloned()
        .collect();
    let mut kept = vec![];
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if is_heading(line) {
            let items: Vec<&String> = lines[index + 1..]
                .iter()
                .take_while(|item| {
                    let item = item.trim();
                    item.starts_with("- ") || item.starts_with("* ")
                })
                .collect();
            if !items.is_empty() && items.iter().all(|item| is_flag_value(item)) {
                index += 1 + items.len();
                continue;
            }
        }
        kept.push(lines[index].clone());
        index += 1;
    }
    kept
}

fn reflow(lines: &[String]) -> String {
    let lines = without_repeats(lines);
    let mut out: Vec<String> = vec![];
    let mut previous: Option<&str> = None;
    for line in &lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let shown = if line.starts_with("- ") || line.starts_with("* ") {
            format!("  {}", line)
        } else {
            line.to_owned()
        };
        match out.last_mut() {
            Some(last) if !starts_own_line(line, previous) => {
                let new_sentence = line
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false);
                if new_sentence && !ends_a_sentence(last) {
                    last.push('.');
                }
                last.push(' ');
                last.push_str(line);
            },
            _ => out.push(shown),
        }
        previous = Some(line);
    }
    while out.last().map(|last| is_heading(last)).unwrap_or(false) {
        out.pop();
    }
    if let Some(first) = out.first_mut() {
        let mut characters = first.chars();
        if let Some(initial) = characters.next() {
            *first = initial.to_uppercase().chain(characters).collect();
        }
    }
    out.join("\n")
}

fn finish(lines: &[String]) -> Option<String> {
    let mut paragraphs: Vec<String> = vec![];
    let mut current: Vec<String> = vec![];
    for line in lines {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(reflow(&current));
                current.clear();
            }
            paragraphs.retain(|paragraph| !paragraph.is_empty());
        } else {
            current.push(line.clone());
        }
    }
    if !current.is_empty() {
        paragraphs.push(reflow(&current));
    }
    paragraphs.retain(|paragraph| !paragraph.is_empty());
    if paragraphs.is_empty() {
        None
    } else {
        Some(paragraphs.join("\n\n"))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawHelp {
    pub own: Option<Vec<String>>,
    pub block: Option<Vec<String>>,
}

impl RawHelp {
    pub fn text(&self) -> Option<String> {
        self.own
            .as_ref()
            .and_then(|lines| finish(lines))
            .or_else(|| self.block.as_ref().and_then(|lines| finish(lines)))
    }
    #[cfg(test)]
    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.own.iter().chain(self.block.iter()).flatten()
    }
}

fn has_text(lines: &[String]) -> bool {
    lines.iter().any(|line| !line.trim().is_empty())
}

pub fn parse_raw_help(config: &str) -> BTreeMap<SettingKey, RawHelp> {
    let mut help = BTreeMap::new();
    let mut path: Vec<String> = vec![];
    let mut block_help: Vec<Option<Vec<String>>> = vec![];
    let mut pending: Vec<String> = vec![];
    let mut previous: Option<Vec<String>> = None;
    for raw in config.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            pending.clear();
            previous = None;
            continue;
        }
        let (inner, commented) = match trimmed.strip_prefix("//") {
            Some(rest) => (rest.trim(), true),
            None => (trimmed, false),
        };
        if inner == "}" {
            path.pop();
            block_help.pop();
            pending.clear();
            previous = None;
            continue;
        }
        if let Some(text) = inner.strip_prefix("//") {
            if commented {
                pending.push(text.trim().to_owned());
                previous = None;
            }
            continue;
        }
        if inner.is_empty() {
            if path.is_empty() {
                pending.push(String::new());
            } else {
                pending.clear();
                previous = None;
            }
            continue;
        }
        if inner.ends_with('{') {
            path.push(first_word(inner).to_owned());
            block_help.push(Some(pending.clone()).filter(|lines| has_text(lines)));
            pending.clear();
            previous = None;
            continue;
        }
        let key = key_named(&path, first_word(inner)).filter(|_| looks_like_a_value(inner));
        match key {
            Some(key) => {
                let own = Some(pending.clone())
                    .filter(|lines| has_text(lines))
                    .or_else(|| previous.clone());
                let block = block_help.iter().rev().flatten().next().cloned();
                help.entry(key).or_insert(RawHelp {
                    own: own.clone(),
                    block,
                });
                previous = own;
                pending.clear();
            },
            None if commented => {
                pending.push(inner.to_owned());
                previous = None;
            },
            None => {
                pending.clear();
                previous = None;
            },
        }
    }
    help
}

#[cfg(test)]
pub fn parse_help(config: &str) -> BTreeMap<SettingKey, String> {
    parse_raw_help(config)
        .into_iter()
        .filter_map(|(key, raw)| raw.text().map(|text| (key, text)))
        .collect()
}

fn raw_help_table() -> &'static BTreeMap<SettingKey, RawHelp> {
    static RAW: OnceLock<BTreeMap<SettingKey, RawHelp>> = OnceLock::new();
    RAW.get_or_init(|| parse_raw_help(DEFAULT_CONFIG))
}

pub fn setting_help(key: SettingKey) -> Option<&'static str> {
    static HELP: OnceLock<BTreeMap<SettingKey, String>> = OnceLock::new();
    HELP.get_or_init(|| {
        raw_help_table()
            .iter()
            .filter_map(|(key, raw)| raw.text().map(|text| (*key, text)))
            .collect()
    })
    .get(&key)
    .map(|text| text.as_str())
}

#[cfg(test)]
pub fn says_it_needs_a_restart(key: SettingKey) -> bool {
    raw_help_table()
        .get(&key)
        .map(|raw| raw.lines().any(|line| is_restart_note(line)))
        .unwrap_or(false)
}

#[cfg(test)]
pub fn stated_default(key: SettingKey) -> Option<String> {
    let raw = raw_help_table().get(&key)?;
    raw.own
        .iter()
        .flatten()
        .find_map(|line| line.trim().strip_prefix("Default:"))
        .map(|value| value.trim().to_owned())
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut lines = vec![];
    for source_line in text.lines() {
        let indent: String = source_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let mut line = String::new();
        for word in source_line.split_whitespace() {
            let candidate = if line.is_empty() {
                format!("{}{}", indent, word)
            } else {
                format!("{} {}", line, word)
            };
            if zellij_tile::graphemes::display_width(&candidate) > width && !line.is_empty() {
                lines.push(line);
                line = format!("{}{}", indent, word);
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_top_level_option_gets_the_comment_above_it() {
        let help = setting_help(SettingKey::OnForceClose).unwrap();
        assert!(help.contains("SIGTERM"), "{}", help);
        assert!(help.contains("detach (Default)"), "{}", help);
        assert!(!help.contains("on_force_close"), "{}", help);
    }

    #[test]
    fn options_inside_a_block_get_their_own_comment() {
        let frame = setting_help(SettingKey::AttentionFrameMarkers).unwrap();
        assert!(frame.contains("frame title"), "{}", frame);
        let tabs = setting_help(SettingKey::AttentionTabMarkers).unwrap();
        assert!(tabs.contains("tab bar"), "{}", tabs);
        assert!(!tabs.contains("frame title"), "{}", tabs);
        let font_size = setting_help(SettingKey::TerminalFontSize).unwrap();
        assert!(font_size.contains("points"), "{}", font_size);
    }

    #[test]
    fn options_listed_together_share_their_comment() {
        let top = setting_help(SettingKey::FrameBorderTop).unwrap();
        let right = setting_help(SettingKey::FrameBorderRight).unwrap();
        assert_eq!(top, right);
        assert_eq!(
            setting_help(SettingKey::ThemeDark),
            setting_help(SettingKey::ThemeLight)
        );
    }

    #[test]
    fn a_nested_comment_block_does_not_leak_into_the_next_top_level_option() {
        let parsed = parse_help(
            "// Outer block\n//\n// outer {\n//     // inner text\n//     thing 1\n// }\n\n// About pane frames\n//\n// pane_frames true\n",
        );
        assert_eq!(
            parsed.get(&SettingKey::PaneFrames).map(|s| s.as_str()),
            Some("About pane frames")
        );
    }

    #[test]
    fn options_without_a_comment_get_the_comment_of_their_block() {
        let parsed =
            parse_help("// About the web client\nweb_client {\n    font \"monospace\"\n}\n");
        assert_eq!(
            parsed.get(&SettingKey::WebClientFont).map(|s| s.as_str()),
            Some("About the web client")
        );
    }

    #[test]
    fn every_setting_has_help_from_the_config_file() {
        let missing: Vec<SettingKey> = crate::settings::all_row_settings()
            .iter()
            .copied()
            .filter(|key| setting_help(*key).is_none())
            .collect();
        assert!(missing.is_empty(), "{:?}", missing);
    }

    #[test]
    fn the_line_breaks_of_the_config_file_are_replaced_by_the_screen_width() {
        let help = setting_help(SettingKey::ScrollBufferSize).unwrap();
        assert!(
            help.contains("for each pane in the scroll back buffer. A logical line"),
            "{}",
            help
        );
        let help = setting_help(SettingKey::OnForceClose).unwrap();
        assert!(
            help.contains("\nOptions:\n  - detach (Default)\n  - quit"),
            "{}",
            help
        );
        assert!(help.contains("is closed\nOptions:"), "{}", help);
    }

    fn without_quotes(text: &str) -> String {
        text.replace(['"', '\\'], "").trim().to_lowercase()
    }

    #[test]
    fn what_the_settings_screen_already_shows_is_left_out() {
        let help = setting_help(SettingKey::OnForceClose).unwrap();
        assert!(!help.contains("Requires restart"), "{}", help);
        assert!(help.contains("detach (Default)"), "{}", help);
        let help = setting_help(SettingKey::ScrollBufferSize).unwrap();
        assert!(!help.contains("Default"), "{}", help);
        assert!(help.contains("Valid values: positive integers"), "{}", help);
        let help = setting_help(SettingKey::PaneFrames).unwrap();
        assert!(!help.contains("true"), "{}", help);
        assert!(!help.contains("Options"), "{}", help);
    }

    #[test]
    fn the_help_never_restates_a_restart_or_a_default_in_other_words() {
        for key in crate::settings::all_row_settings() {
            let help = setting_help(*key).unwrap().to_lowercase();
            assert!(
                !help.contains("restart"),
                "{}: write it as a line of its own, \"(Requires restart)\": {}",
                key,
                help
            );
            for phrase in ["default:", "default value", "default is", "defaults to "] {
                assert!(
                    !help
                        .lines()
                        .any(|line| line.contains(phrase) && !line.contains("defaults to the")),
                    "{}: write the default as a line of its own, \"Default: ...\": {}",
                    key,
                    help
                );
            }
        }
    }

    #[test]
    fn the_restart_note_of_the_config_file_matches_the_setting() {
        for key in crate::settings::all_row_settings() {
            assert_eq!(
                says_it_needs_a_restart(*key),
                key.requires_restart(),
                "{}: the config file and the setting disagree about \"(Requires restart)\"",
                key
            );
        }
    }

    #[test]
    fn the_default_of_the_config_file_matches_the_setting() {
        for key in crate::settings::all_row_settings() {
            let Some(stated) = stated_default(*key) else {
                continue;
            };
            let stated = without_quotes(&stated);
            let known = without_quotes(crate::settings::describe(*key).default);
            assert!(
                stated == known || stated.starts_with(&format!("{} (", known)),
                "{}: the config file says the default is {:?}, the settings screen says {:?}",
                key,
                stated,
                known
            );
        }
    }

    #[test]
    fn long_text_is_wrapped_at_word_boundaries() {
        assert_eq!(
            wrap("one two three four", 10),
            vec!["one two".to_owned(), "three four".to_owned()]
        );
        assert_eq!(wrap("  - a b", 20), vec!["  - a b".to_owned()]);
    }
}
