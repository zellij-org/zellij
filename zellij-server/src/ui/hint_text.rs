use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;
use zellij_utils::data::StyledText;

pub enum HintLevel {
    Plain,
    Emphasis,
    Error,
    Success,
}

pub struct HintSegment {
    pub text: String,
    pub level: HintLevel,
}

impl HintSegment {
    fn plain(text: &str) -> Self {
        HintSegment {
            text: text.to_string(),
            level: HintLevel::Plain,
        }
    }
    fn emphasis(text: &str) -> Self {
        HintSegment {
            text: text.to_string(),
            level: HintLevel::Emphasis,
        }
    }
    fn error(text: &str) -> Self {
        HintSegment {
            text: text.to_string(),
            level: HintLevel::Error,
        }
    }
    fn success(text: &str) -> Self {
        HintSegment {
            text: text.to_string(),
            level: HintLevel::Success,
        }
    }
}

#[derive(Copy, Clone)]
pub enum HintExitStatus {
    Code(i32),
    Exited,
}

#[derive(Copy, Clone)]
pub enum HintTier {
    Full,
    Medium,
    Minimal,
}

const ALL_TIERS: [HintTier; 3] = [HintTier::Full, HintTier::Medium, HintTier::Minimal];

fn level_index(level: &HintLevel) -> Option<usize> {
    match level {
        HintLevel::Plain => None,
        HintLevel::Emphasis => Some(0),
        HintLevel::Error => Some(6),
        HintLevel::Success => Some(7),
    }
}

pub fn segments_to_styled_text(segments: &[HintSegment]) -> StyledText {
    let mut text = String::new();
    let mut indices: Vec<Vec<usize>> = vec![];
    let mut offset = 0;
    for segment in segments {
        let char_count = segment.text.chars().count();
        if let Some(level) = level_index(&segment.level) {
            if indices.len() <= level {
                indices.resize(level + 1, vec![]);
            }
            for position in offset..offset + char_count {
                indices[level].push(position);
            }
        }
        text.push_str(&segment.text);
        offset += char_count;
    }
    StyledText { text, indices }
}

pub fn rerun_segments(is_first_run: bool, tier: HintTier) -> Vec<HintSegment> {
    match tier {
        HintTier::Full => vec![
            HintSegment::plain(if is_first_run { " <" } else { "<" }),
            HintSegment::emphasis("ENTER"),
            HintSegment::plain(">"),
            HintSegment::plain(if is_first_run { " run, " } else { " re-run, " }),
            HintSegment::plain("<"),
            HintSegment::emphasis("ESC"),
            HintSegment::plain(">"),
            HintSegment::plain(" drop to shell, "),
            HintSegment::plain("<"),
            HintSegment::emphasis("Ctrl-c"),
            HintSegment::plain(">"),
            HintSegment::plain(" exit "),
        ],
        HintTier::Medium => vec![
            HintSegment::plain("<"),
            HintSegment::emphasis("ENTER"),
            HintSegment::plain(">"),
            HintSegment::plain(if is_first_run { " run " } else { " re-run " }),
            HintSegment::plain("<"),
            HintSegment::emphasis("ESC"),
            HintSegment::plain(">"),
            HintSegment::plain(" shell "),
            HintSegment::plain("<"),
            HintSegment::emphasis("Ctrl-c"),
            HintSegment::plain(">"),
            HintSegment::plain(" exit"),
        ],
        HintTier::Minimal => vec![
            HintSegment::plain("<"),
            HintSegment::emphasis("ENTER"),
            HintSegment::plain(">"),
            HintSegment::plain(if is_first_run { " run" } else { " re-run" }),
        ],
    }
}

pub fn exit_code_segments(exit_status: HintExitStatus) -> Vec<HintSegment> {
    match exit_status {
        HintExitStatus::Code(exit_code) => {
            let exit_code_text = format!("{}", exit_code);
            let exit_code_segment = if exit_code == 0 {
                HintSegment::success(&exit_code_text)
            } else {
                HintSegment::error(&exit_code_text)
            };
            vec![
                HintSegment::plain(" [ "),
                HintSegment::plain("EXIT CODE: "),
                exit_code_segment,
                HintSegment::plain(" ] "),
            ]
        },
        HintExitStatus::Exited => vec![
            HintSegment::plain(" [ "),
            HintSegment::error("EXITED"),
            HintSegment::plain(" ] "),
        ],
    }
}

pub fn hover_segments(tier: HintTier) -> Vec<HintSegment> {
    match tier {
        HintTier::Full => vec![
            HintSegment::plain(" "),
            HintSegment::emphasis("Alt"),
            HintSegment::plain(" <"),
            HintSegment::emphasis("Click"),
            HintSegment::plain(">"),
            HintSegment::plain(" - group,"),
            HintSegment::plain(" "),
            HintSegment::emphasis("Alt"),
            HintSegment::plain(" <"),
            HintSegment::emphasis("Right-Click"),
            HintSegment::plain(">"),
            HintSegment::plain(" - ungroup all "),
        ],
        HintTier::Medium => vec![
            HintSegment::emphasis("Alt"),
            HintSegment::plain(" <"),
            HintSegment::emphasis("Click"),
            HintSegment::plain("> group, "),
            HintSegment::emphasis("Alt"),
            HintSegment::plain(" <"),
            HintSegment::emphasis("Right-Click"),
            HintSegment::plain("> ungroup"),
        ],
        HintTier::Minimal => vec![
            HintSegment::emphasis("Alt"),
            HintSegment::plain(" <"),
            HintSegment::emphasis("Click"),
            HintSegment::plain("> group"),
        ],
    }
}

pub fn hover_hint_variants() -> BTreeMap<usize, StyledText> {
    let mut variants = BTreeMap::new();
    for tier in ALL_TIERS {
        let styled_text = segments_to_styled_text(&hover_segments(tier));
        variants.insert(styled_text.text.width(), styled_text);
    }
    variants
}

pub fn held_hint_variants(
    is_first_run: bool,
    exit_status: Option<HintExitStatus>,
) -> BTreeMap<usize, StyledText> {
    let mut variants = BTreeMap::new();
    for tier in ALL_TIERS {
        let mut segments = vec![];
        if let Some(exit_status) = exit_status {
            segments.extend(exit_code_segments(exit_status));
        }
        segments.extend(rerun_segments(is_first_run, tier));
        let styled_text = segments_to_styled_text(&segments);
        variants.insert(styled_text.text.width(), styled_text);
    }
    variants
}
