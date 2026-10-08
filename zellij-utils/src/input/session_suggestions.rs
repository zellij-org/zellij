use std::path::PathBuf;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use serde::{Deserialize, Serialize};

use super::config::ConfigError;

pub const FACT_DIRECTORY: &str = "directory";
pub const FACT_GIT_REPO: &str = "git_repo";
pub const FACT_GIT_BRANCH: &str = "git_branch";
pub const FACT_GIT_WORKTREE: &str = "git_worktree";
pub const FACT_HOSTNAME: &str = "hostname";
pub const BUILT_IN_FACTS: [&str; 5] = [
    FACT_DIRECTORY,
    FACT_GIT_REPO,
    FACT_GIT_BRANCH,
    FACT_GIT_WORKTREE,
    FACT_HOSTNAME,
];
pub const DEFAULT_SCRIPT_TIMEOUT_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum FactTiming {
    Start,
    #[default]
    Live,
}

impl FactTiming {
    fn as_str(&self) -> &'static str {
        match self {
            FactTiming::Start => "start",
            FactTiming::Live => "live",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustomFact {
    pub name: String,
    pub command: String,
    pub column: Option<String>,
    pub at: FactTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MatchRule {
    pub facts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ThenOrder {
    #[default]
    Recent,
    Alphabetical,
    None,
}

impl ThenOrder {
    fn as_str(&self) -> &'static str {
        match self {
            ThenOrder::Recent => "recent",
            ThenOrder::Alphabetical => "alphabetical",
            ThenOrder::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RankingScript {
    pub path: PathBuf,
    pub timeout_ms: u64,
    pub hide_unlisted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SessionSuggestionsConfig {
    pub facts: Vec<CustomFact>,
    pub rules: Option<Vec<MatchRule>>,
    pub then: Option<ThenOrder>,
    pub script: Option<RankingScript>,
    pub record: Option<PathBuf>,
}

pub fn default_rules() -> Vec<MatchRule> {
    vec![
        MatchRule {
            facts: vec![FACT_DIRECTORY.to_owned(), FACT_GIT_BRANCH.to_owned()],
        },
        MatchRule {
            facts: vec![FACT_GIT_REPO.to_owned(), FACT_GIT_BRANCH.to_owned()],
        },
        MatchRule {
            facts: vec![FACT_DIRECTORY.to_owned()],
        },
        MatchRule {
            facts: vec![FACT_GIT_REPO.to_owned()],
        },
    ]
}

fn kdl_error(message: String, node: &KdlNode) -> ConfigError {
    ConfigError::new_kdl_error(message, node.span().offset(), node.span().len())
}

fn string_property<'a>(node: &'a KdlNode, name: &str) -> Option<&'a str> {
    node.get(name).and_then(|e| e.value().as_string())
}

fn positional_strings(node: &KdlNode) -> Vec<String> {
    node.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .filter_map(|e| e.value().as_string().map(|s| s.to_owned()))
        .collect()
}

impl SessionSuggestionsConfig {
    pub fn effective_rules(&self) -> Vec<MatchRule> {
        self.rules.clone().unwrap_or_else(default_rules)
    }

    pub fn effective_then(&self) -> ThenOrder {
        self.then.unwrap_or_default()
    }

    pub fn is_known_fact(&self, name: &str) -> bool {
        BUILT_IN_FACTS.contains(&name) || self.facts.iter().any(|f| f.name == name)
    }

    pub fn rules_use_fact(&self, name: &str) -> bool {
        self.effective_rules()
            .iter()
            .any(|r| r.facts.iter().any(|f| f == name))
    }

    pub fn columns(&self) -> Vec<(String, String)> {
        let mut columns = vec![];
        for built_in in BUILT_IN_FACTS {
            if self.rules_use_fact(built_in) {
                columns.push((built_in.to_owned(), built_in.to_owned()));
            }
        }
        for fact in &self.facts {
            if let Some(column) = &fact.column {
                columns.push((fact.name.clone(), column.clone()));
            }
        }
        columns
    }

    pub fn from_kdl(kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut config = SessionSuggestionsConfig::default();
        let Some(children) = kdl.children() else {
            return Ok(config);
        };
        let mut rules = vec![];
        for node in children.nodes() {
            match node.name().value() {
                "fact" => {
                    let name = positional_strings(node).into_iter().next().ok_or_else(|| {
                        kdl_error("fact must have a name".to_owned(), node)
                    })?;
                    if BUILT_IN_FACTS.contains(&name.as_str()) {
                        return Err(kdl_error(
                            format!("\"{}\" is a built-in fact and cannot be redefined", name),
                            node,
                        ));
                    }
                    let command = string_property(node, "command")
                        .ok_or_else(|| kdl_error("fact must have a command=".to_owned(), node))?
                        .to_owned();
                    let column = string_property(node, "column").map(|c| c.to_owned());
                    let at = match string_property(node, "at") {
                        None | Some("live") => FactTiming::Live,
                        Some("start") => FactTiming::Start,
                        Some(other) => {
                            return Err(kdl_error(
                                format!("at must be \"start\" or \"live\", found \"{}\"", other),
                                node,
                            ))
                        },
                    };
                    config.facts.push(CustomFact {
                        name,
                        command,
                        column,
                        at,
                    });
                },
                "match" => {
                    let facts = positional_strings(node);
                    if facts.is_empty() {
                        return Err(kdl_error(
                            "match must list at least one fact".to_owned(),
                            node,
                        ));
                    }
                    rules.push((MatchRule { facts }, node.clone()));
                },
                "then" => {
                    let value = positional_strings(node).into_iter().next();
                    config.then = Some(match value.as_deref() {
                        Some("recent") => ThenOrder::Recent,
                        Some("alphabetical") => ThenOrder::Alphabetical,
                        Some("none") => ThenOrder::None,
                        _ => {
                            return Err(kdl_error(
                                "then must be \"recent\", \"alphabetical\" or \"none\"".to_owned(),
                                node,
                            ))
                        },
                    });
                },
                "script" => {
                    let path = positional_strings(node).into_iter().next().ok_or_else(|| {
                        kdl_error("script must have a path".to_owned(), node)
                    })?;
                    let timeout_ms = match node.get("timeout_ms").map(|e| e.value()) {
                        None => DEFAULT_SCRIPT_TIMEOUT_MS,
                        Some(value) => match value.as_i64() {
                            Some(v) if v > 0 => v as u64,
                            _ => {
                                return Err(kdl_error(
                                    "timeout_ms must be a positive number".to_owned(),
                                    node,
                                ))
                            },
                        },
                    };
                    let hide_unlisted = match node.get("hide_unlisted").map(|e| e.value()) {
                        None => false,
                        Some(value) => value.as_bool().ok_or_else(|| {
                            kdl_error("hide_unlisted must be true or false".to_owned(), node)
                        })?,
                    };
                    config.script = Some(RankingScript {
                        path: PathBuf::from(path),
                        timeout_ms,
                        hide_unlisted,
                    });
                },
                "record" => {
                    let path = positional_strings(node).into_iter().next().ok_or_else(|| {
                        kdl_error("record must have a path".to_owned(), node)
                    })?;
                    config.record = Some(PathBuf::from(path));
                },
                other => {
                    return Err(kdl_error(
                        format!("Unknown session_suggestions setting \"{}\"", other),
                        node,
                    ))
                },
            }
        }
        for (rule, node) in &rules {
            for fact in &rule.facts {
                if !config.is_known_fact(fact) {
                    return Err(kdl_error(format!("Unknown fact \"{}\"", fact), node));
                }
            }
        }
        if !rules.is_empty() {
            config.rules = Some(rules.into_iter().map(|(r, _)| r).collect());
        }
        Ok(config)
    }

    pub fn to_kdl(&self) -> Option<KdlNode> {
        if self == &SessionSuggestionsConfig::default() {
            return None;
        }
        let mut node = KdlNode::new("session_suggestions");
        let mut children = KdlDocument::new();
        for fact in &self.facts {
            let mut fact_node = KdlNode::new("fact");
            fact_node.push(KdlValue::String(fact.name.clone()));
            fact_node.push(KdlEntry::new_prop("command", fact.command.clone()));
            if let Some(column) = &fact.column {
                fact_node.push(KdlEntry::new_prop("column", column.clone()));
            }
            fact_node.push(KdlEntry::new_prop("at", fact.at.as_str()));
            children.nodes_mut().push(fact_node);
        }
        if let Some(rules) = &self.rules {
            for rule in rules {
                let mut match_node = KdlNode::new("match");
                for fact in &rule.facts {
                    match_node.push(KdlValue::String(fact.clone()));
                }
                children.nodes_mut().push(match_node);
            }
        }
        if let Some(then) = &self.then {
            let mut then_node = KdlNode::new("then");
            then_node.push(KdlValue::String(then.as_str().to_owned()));
            children.nodes_mut().push(then_node);
        }
        if let Some(script) = &self.script {
            let mut script_node = KdlNode::new("script");
            script_node.push(KdlValue::String(script.path.display().to_string()));
            script_node.push(KdlEntry::new_prop("timeout_ms", script.timeout_ms as i64));
            script_node.push(KdlEntry::new_prop("hide_unlisted", script.hide_unlisted));
            children.nodes_mut().push(script_node);
        }
        if let Some(record) = &self.record {
            let mut record_node = KdlNode::new("record");
            record_node.push(KdlValue::String(record.display().to_string()));
            children.nodes_mut().push(record_node);
        }
        node.set_children(children);
        Some(node)
    }

    pub fn merge(&self, other: SessionSuggestionsConfig) -> Self {
        let mut merged = self.clone();
        if !other.facts.is_empty() {
            merged.facts = other.facts;
        }
        merged.rules = other.rules.or(merged.rules);
        merged.then = other.then.or(merged.then);
        merged.script = other.script.or(merged.script);
        merged.record = other.record.or(merged.record);
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<SessionSuggestionsConfig, ConfigError> {
        let document: KdlDocument = text.parse().unwrap();
        SessionSuggestionsConfig::from_kdl(document.get("session_suggestions").unwrap())
    }

    #[test]
    fn full_block_is_parsed() {
        let config = parse(
            r#"
            session_suggestions {
                fact "project" command="echo $PROJECT" column="project" at="start"
                match "directory" "git_branch"
                match "project"
                then "alphabetical"
                script "~/rank.sh" timeout_ms=500 hide_unlisted=true
                record "~/record.sh"
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            config.facts,
            vec![CustomFact {
                name: "project".to_owned(),
                command: "echo $PROJECT".to_owned(),
                column: Some("project".to_owned()),
                at: FactTiming::Start,
            }]
        );
        assert_eq!(
            config.rules,
            Some(vec![
                MatchRule {
                    facts: vec!["directory".to_owned(), "git_branch".to_owned()]
                },
                MatchRule {
                    facts: vec!["project".to_owned()]
                },
            ])
        );
        assert_eq!(config.then, Some(ThenOrder::Alphabetical));
        assert_eq!(
            config.script,
            Some(RankingScript {
                path: PathBuf::from("~/rank.sh"),
                timeout_ms: 500,
                hide_unlisted: true,
            })
        );
        assert_eq!(config.record, Some(PathBuf::from("~/record.sh")));
    }

    #[test]
    fn empty_block_uses_the_default_rules() {
        let config = parse("session_suggestions {\n}").unwrap();
        assert_eq!(config.effective_rules(), default_rules());
        assert_eq!(config.effective_then(), ThenOrder::Recent);
    }

    #[test]
    fn the_default_block_is_identical_to_the_built_in_default() {
        let config = parse(
            r#"
            session_suggestions {
                match "directory" "git_branch"
                match "git_repo" "git_branch"
                match "directory"
                match "git_repo"
                then "recent"
            }
            "#,
        )
        .unwrap();
        assert_eq!(config.effective_rules(), default_rules());
        assert_eq!(config.effective_then(), ThenOrder::Recent);
    }

    #[test]
    fn unknown_facts_are_rejected() {
        assert!(parse("session_suggestions {\n match \"colour\"\n}").is_err());
    }

    #[test]
    fn bad_values_are_rejected() {
        assert!(parse("session_suggestions {\n then \"random\"\n}").is_err());
        assert!(
            parse("session_suggestions {\n fact \"a\" command=\"x\" at=\"never\"\n}").is_err()
        );
        assert!(parse("session_suggestions {\n script \"x\" timeout_ms=0\n}").is_err());
        assert!(parse("session_suggestions {\n fact \"directory\" command=\"x\"\n}").is_err());
    }

    #[test]
    fn script_defaults_are_applied() {
        let config = parse("session_suggestions {\n script \"rank.sh\"\n}").unwrap();
        assert_eq!(
            config.script,
            Some(RankingScript {
                path: PathBuf::from("rank.sh"),
                timeout_ms: DEFAULT_SCRIPT_TIMEOUT_MS,
                hide_unlisted: false,
            })
        );
    }

    #[test]
    fn to_kdl_round_trips() {
        let config = parse(
            r#"
            session_suggestions {
                fact "project" command="echo $PROJECT" column="project" at="live"
                match "git_repo"
                then "none"
                script "rank.sh" timeout_ms=300 hide_unlisted=false
                record "record.sh"
            }
            "#,
        )
        .unwrap();
        let node = config.to_kdl().unwrap();
        let reparsed = SessionSuggestionsConfig::from_kdl(&node).unwrap();
        assert_eq!(config, reparsed);
        assert_eq!(SessionSuggestionsConfig::default().to_kdl(), None);
    }

    #[test]
    fn columns_follow_the_rules_and_custom_columns() {
        let config = parse(
            r#"
            session_suggestions {
                fact "project" command="x" column="proj"
                fact "hidden" command="y"
                match "directory" "hidden"
            }
            "#,
        )
        .unwrap();
        assert_eq!(
            config.columns(),
            vec![
                ("directory".to_owned(), "directory".to_owned()),
                ("project".to_owned(), "proj".to_owned()),
            ]
        );
    }

    #[test]
    fn merge_prefers_the_other_config() {
        let base = parse("session_suggestions {\n then \"none\"\n match \"git_repo\"\n}").unwrap();
        let other = parse("session_suggestions {\n then \"alphabetical\"\n}").unwrap();
        let merged = base.merge(other);
        assert_eq!(merged.then, Some(ThenOrder::Alphabetical));
        assert_eq!(
            merged.rules,
            Some(vec![MatchRule {
                facts: vec!["git_repo".to_owned()]
            }])
        );
    }
}
