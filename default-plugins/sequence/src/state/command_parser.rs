use super::{ChainType, CommandEntry};

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParseState {
    Normal,
    InSingleQuote,
    InDoubleQuote,
    Escaped(EscapeContext),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum EscapeContext {
    Normal,
    DoubleQuote,
}

pub fn split_by_chain_operators(text: &str) -> Vec<(String, Option<ChainType>)> {
    let mut segments = Vec::new();
    let mut current_segment = String::new();
    let mut state = ParseState::Normal;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        match state {
            ParseState::Normal => {
                if ch == '\\' {
                    state = ParseState::Escaped(EscapeContext::Normal);
                    current_segment.push(ch);
                    i += 1;
                } else if ch == '\'' {
                    state = ParseState::InSingleQuote;
                    current_segment.push(ch);
                    i += 1;
                } else if ch == '"' {
                    state = ParseState::InDoubleQuote;
                    current_segment.push(ch);
                    i += 1;
                } else if ch == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
                    let segment_text = current_segment.trim().to_string();
                    if !segment_text.is_empty() {
                        segments.push((segment_text, Some(ChainType::And)));
                    }
                    current_segment.clear();
                    i += 2;
                } else if ch == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
                    let segment_text = current_segment.trim().to_string();
                    if !segment_text.is_empty() {
                        segments.push((segment_text, Some(ChainType::Or)));
                    }
                    current_segment.clear();
                    i += 2;
                } else if ch == ';' {
                    let segment_text = current_segment.trim().to_string();
                    if !segment_text.is_empty() {
                        segments.push((segment_text, Some(ChainType::Then)));
                    }
                    current_segment.clear();
                    i += 1;
                } else {
                    current_segment.push(ch);
                    i += 1;
                }
            },
            ParseState::InSingleQuote => {
                current_segment.push(ch);
                if ch == '\'' {
                    state = ParseState::Normal;
                }
                i += 1;
            },
            ParseState::InDoubleQuote => {
                if ch == '\\' {
                    state = ParseState::Escaped(EscapeContext::DoubleQuote);
                    current_segment.push(ch);
                    i += 1;
                } else if ch == '"' {
                    current_segment.push(ch);
                    state = ParseState::Normal;
                    i += 1;
                } else {
                    current_segment.push(ch);
                    i += 1;
                }
            },
            ParseState::Escaped(context) => {
                current_segment.push(ch);
                state = match context {
                    EscapeContext::Normal => ParseState::Normal,
                    EscapeContext::DoubleQuote => ParseState::InDoubleQuote,
                };
                i += 1;
            },
        }
    }

    let final_segment = current_segment.trim().to_string();
    if !final_segment.is_empty() {
        segments.push((final_segment, None));
    }

    segments
}

pub fn detect_chain_operator_at_end(text: &str) -> Option<(String, ChainType)> {
    let segments = split_by_chain_operators(text);

    if segments.is_empty() {
        return None;
    }

    if segments.len() == 1 {
        if let Some((text, Some(chain_type))) = segments.first() {
            return Some((text.clone(), *chain_type));
        }
        return None;
    }

    if let Some((first_text, Some(chain_type))) = segments.first() {
        return Some((first_text.clone(), *chain_type));
    }

    None
}

pub fn get_remaining_after_first_segment(text: &str) -> Option<String> {
    let segments = split_by_chain_operators(text);

    if segments.len() <= 1 {
        return None;
    }

    let mut result = String::new();
    for (i, (segment_text, chain_type_opt)) in segments.iter().enumerate().skip(1) {
        if i > 1 {
            if let Some(chain_type) = chain_type_opt {
                result.push_str(&format!(" {} ", chain_type.as_str()));
            }
        }
        result.push_str(segment_text);
        if let Some(chain_type) = chain_type_opt {
            result.push_str(&format!(" {} ", chain_type.as_str()));
        }
    }

    Some(result.trim().to_string())
}

pub fn detect_cd_command(text: &str) -> Option<String> {
    let trimmed = text.trim();

    if trimmed == "cd" {
        return Some("~".to_string());
    }

    if trimmed.starts_with("cd ") {
        let path = trimmed[3..].trim();
        return Some(path.to_string());
    }

    None
}

/// Serialize a sequence of commands to editor format.
/// CWD changes are emitted as `cd <path>` lines; chain type as trailing ` &&`, ` ||`, ` ;`.
pub fn serialize_sequence_to_editor(commands: &[CommandEntry]) -> String {
    let mut result = String::new();
    let mut prev_cwd: Option<std::path::PathBuf> = None;

    for cmd in commands {
        if cmd.get_text().trim().is_empty() {
            continue;
        }

        let cmd_cwd = cmd.get_cwd();
        if cmd_cwd != prev_cwd && cmd_cwd.is_some() {
            let path = cmd_cwd.as_ref().unwrap();
            result.push_str(&format!("cd {}\n", path.display()));
        }
        prev_cwd = cmd_cwd;

        let op_suffix = match cmd.get_chain_type() {
            ChainType::And => " &&",
            ChainType::Or => " ||",
            ChainType::Then => " ;",
            ChainType::None => "",
        };

        result.push_str(&format!("{}{}\n", cmd.get_text(), op_suffix));
    }

    result
}

/// Parse editor file contents back into a Vec<CommandEntry>.
/// `cd <path>` lines set the cwd for subsequent commands.
/// Blank lines and `#` comment lines are ignored.
pub fn load_from_editor_file(
    contents: &str,
    initial_cwd: Option<std::path::PathBuf>,
) -> Vec<CommandEntry> {
    use crate::path_formatting;
    let mut commands = Vec::new();
    let mut current_cwd = initial_cwd;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Split by chain operators first so that "cd /path &&" doesn't absorb the operator
        // into the path. Each resulting segment is then checked for cd independently.
        let segments = split_by_chain_operators(trimmed);
        for (text, chain_type_opt) in segments {
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            if let Some(path) = detect_cd_command(&text) {
                if let Some(new_cwd) = path_formatting::resolve_path(current_cwd.as_ref(), &path) {
                    current_cwd = Some(new_cwd);
                }
                // cd segments are not added as command entries; their chain type is discarded
            } else {
                let mut entry = CommandEntry::new(&text, current_cwd.clone());
                if let Some(chain_type) = chain_type_opt {
                    entry.set_chain_type(chain_type);
                }
                commands.push(entry);
            }
        }
    }

    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_command_no_operator() {
        let result = split_by_chain_operators("ls");
        assert_eq!(result, vec![("ls".to_string(), None)]);
    }

    #[test]
    fn test_two_commands_with_and() {
        let result = split_by_chain_operators("ls && pwd");
        assert_eq!(
            result,
            vec![
                ("ls".to_string(), Some(ChainType::And)),
                ("pwd".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_two_commands_with_or() {
        let result = split_by_chain_operators("cmd1 || cmd2");
        assert_eq!(
            result,
            vec![
                ("cmd1".to_string(), Some(ChainType::Or)),
                ("cmd2".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_two_commands_with_semicolon() {
        let result = split_by_chain_operators("echo hi ; ls");
        assert_eq!(
            result,
            vec![
                ("echo hi".to_string(), Some(ChainType::Then)),
                ("ls".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_three_commands_mixed() {
        let result = split_by_chain_operators("a && b || c");
        assert_eq!(
            result,
            vec![
                ("a".to_string(), Some(ChainType::And)),
                ("b".to_string(), Some(ChainType::Or)),
                ("c".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_operator_in_single_quotes() {
        let result = split_by_chain_operators("echo '&&' && ls");
        assert_eq!(
            result,
            vec![
                ("echo '&&'".to_string(), Some(ChainType::And)),
                ("ls".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_operator_in_double_quotes() {
        let result = split_by_chain_operators("echo \"||\" || pwd");
        assert_eq!(
            result,
            vec![
                ("echo \"||\"".to_string(), Some(ChainType::Or)),
                ("pwd".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_escaped_operator() {
        let result = split_by_chain_operators("echo \\&& test");
        assert_eq!(result, vec![("echo \\&& test".to_string(), None)]);
    }

    #[test]
    fn test_complex_quoting() {
        let result = split_by_chain_operators("cmd1 && echo \"a || b\" && cmd2");
        assert_eq!(
            result,
            vec![
                ("cmd1".to_string(), Some(ChainType::And)),
                ("echo \"a || b\"".to_string(), Some(ChainType::And)),
                ("cmd2".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_empty_segments_filtered() {
        let result = split_by_chain_operators("cmd1 && && cmd2");
        assert_eq!(
            result,
            vec![
                ("cmd1".to_string(), Some(ChainType::And)),
                ("cmd2".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_detect_operator_just_typed_and() {
        let result = detect_chain_operator_at_end("ls &&");
        assert_eq!(result, Some(("ls".to_string(), ChainType::And)));
    }

    #[test]
    fn test_detect_operator_just_typed_or() {
        let result = detect_chain_operator_at_end("cmd ||");
        assert_eq!(result, Some(("cmd".to_string(), ChainType::Or)));
    }

    #[test]
    fn test_detect_operator_just_typed_semicolon() {
        let result = detect_chain_operator_at_end("echo ;");
        assert_eq!(result, Some(("echo".to_string(), ChainType::Then)));
    }

    #[test]
    fn test_detect_operator_no_operator() {
        let result = detect_chain_operator_at_end("ls");
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_operator_in_quotes() {
        let result = detect_chain_operator_at_end("echo '&&'");
        assert_eq!(result, None);
    }

    #[test]
    fn test_whitespace_trimming() {
        let result = split_by_chain_operators("  cmd1  &&  cmd2  ");
        assert_eq!(
            result,
            vec![
                ("cmd1".to_string(), Some(ChainType::And)),
                ("cmd2".to_string(), None)
            ]
        );
    }

    #[test]
    fn test_trailing_operator_with_space() {
        let result = split_by_chain_operators("ls && ");
        assert_eq!(result, vec![("ls".to_string(), Some(ChainType::And))]);
    }

    #[test]
    fn test_cd_with_chain_operator_does_not_absorb_operator_into_path() {
        use super::super::CommandEntry;
        use std::path::PathBuf;

        let contents = "cd /some/path && cargo build";
        let result = load_from_editor_file(contents, None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].get_text(), "cargo build");
        assert_eq!(result[0].get_cwd(), Some(PathBuf::from("/some/path")));
    }
}
