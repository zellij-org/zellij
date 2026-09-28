//! Helper functions for querying cargo metadata
use anyhow::Context;
use serde_json::Value;
use xshell::{cmd, Shell};

/// Get cargo metadata for the workspace
pub fn get_cargo_metadata(sh: &Shell) -> anyhow::Result<Value> {
    let cargo = crate::cargo().context("Failed to find cargo executable")?;
    let metadata_json = cmd!(sh, "{cargo} metadata --format-version 1 --no-deps")
        .read()
        .context("Failed to run cargo metadata")?;

    serde_json::from_str(&metadata_json).context("Failed to parse cargo metadata JSON")
}

pub const WEB_FEATURE: &str = "web_server_capability";
pub const WINDOW_FEATURE: &str = "window";

/// Get the appropriate features string for a crate when --no-web is enabled
/// Returns Some(features_string) if the crate has web_server_capability and should use --no-default-features
/// Returns None if the crate doesn't have web_server_capability and should use normal build
pub fn get_no_web_features(sh: &Shell, crate_name: &str) -> anyhow::Result<Option<String>> {
    get_features_without(sh, crate_name, &[WEB_FEATURE])
}

pub fn get_features_without(
    sh: &Shell,
    crate_name: &str,
    excluded: &[&str],
) -> anyhow::Result<Option<String>> {
    features_without(&get_cargo_metadata(sh)?, crate_name, excluded)
}

fn features_without(
    metadata: &Value,
    crate_name: &str,
    excluded: &[&str],
) -> anyhow::Result<Option<String>> {
    let packages = metadata["packages"]
        .as_array()
        .context("Expected packages array in metadata")?;

    let mut main_default_features = Vec::new();
    for package in packages {
        let name = package["name"]
            .as_str()
            .context("Expected package name as string")?;

        if name == "zellij" {
            let features = package["features"]
                .as_object()
                .context("Expected features object")?;

            if let Some(default_features) = features.get("default").and_then(|v| v.as_array()) {
                for feature_value in default_features {
                    if let Some(feature_name) = feature_value.as_str() {
                        main_default_features.push(feature_name.to_string());
                    }
                }
            }
            break;
        }
    }

    for package in packages {
        let name = package["name"]
            .as_str()
            .context("Expected package name as string")?;

        // Handle the root crate case
        let matches_crate = if crate_name == "." {
            name == "zellij"
        } else {
            name == crate_name
        };

        if matches_crate {
            let features = package["features"]
                .as_object()
                .context("Expected features object")?;
            let crate_features: Vec<String> = features.keys().cloned().collect();

            return Ok(applicable_features(
                &main_default_features,
                &crate_features,
                excluded,
            ));
        }
    }

    Ok(None)
}

fn applicable_features(
    main_default_features: &[String],
    crate_features: &[String],
    excluded: &[&str],
) -> Option<String> {
    if !excluded
        .iter()
        .any(|feature| crate_features.iter().any(|owned| owned == feature))
    {
        return None;
    }

    let kept: Vec<&str> = main_default_features
        .iter()
        .map(String::as_str)
        .filter(|feature| !excluded.contains(feature))
        .filter(|feature| crate_features.iter().any(|owned| owned == feature))
        .collect();

    Some(kept.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(features: &[&str]) -> Vec<String> {
        features.iter().map(|f| f.to_string()).collect()
    }

    fn zellij_defaults() -> Vec<String> {
        owned(&[
            "plugins_from_target",
            "vendored_curl",
            WEB_FEATURE,
            WINDOW_FEATURE,
        ])
    }

    #[test]
    fn a_crate_without_the_excluded_feature_is_left_alone() {
        assert_eq!(
            applicable_features(
                &zellij_defaults(),
                &owned(&["plugins_from_target"]),
                &[WINDOW_FEATURE]
            ),
            None
        );
    }

    #[test]
    fn the_excluded_feature_is_dropped_and_the_rest_of_the_defaults_kept() {
        assert_eq!(
            applicable_features(&zellij_defaults(), &zellij_defaults(), &[WINDOW_FEATURE]),
            Some(format!("plugins_from_target vendored_curl {}", WEB_FEATURE))
        );
    }

    #[test]
    fn excluding_both_leaves_only_the_features_neither_names() {
        assert_eq!(
            applicable_features(
                &zellij_defaults(),
                &zellij_defaults(),
                &[WEB_FEATURE, WINDOW_FEATURE]
            ),
            Some("plugins_from_target vendored_curl".to_owned())
        );
    }

    #[test]
    fn a_default_the_crate_does_not_have_is_not_offered_to_it() {
        assert_eq!(
            applicable_features(
                &zellij_defaults(),
                &owned(&[WEB_FEATURE, "unstable"]),
                &[WEB_FEATURE]
            ),
            Some(String::new())
        );
    }

    fn metadata() -> Value {
        serde_json::json!({
            "packages": [
                {
                    "name": "zellij",
                    "features": {
                        "default": ["plugins_from_target", "vendored_curl", WEB_FEATURE, WINDOW_FEATURE],
                        "plugins_from_target": [],
                        "vendored_curl": [],
                        "unstable": [],
                        WEB_FEATURE: [],
                        WINDOW_FEATURE: [],
                    }
                },
                {
                    "name": "zellij-utils",
                    "features": { "plugins_from_target": [], "vendored_curl": [], WEB_FEATURE: [] }
                },
                {
                    "name": "zellij-window",
                    "features": {}
                }
            ]
        })
    }

    #[test]
    fn the_root_crate_is_addressed_by_its_directory() {
        assert_eq!(
            features_without(&metadata(), ".", &[WINDOW_FEATURE]).unwrap(),
            Some(format!("plugins_from_target vendored_curl {}", WEB_FEATURE))
        );
    }

    #[test]
    fn a_crate_carrying_the_excluded_feature_keeps_only_the_defaults_it_has() {
        assert_eq!(
            features_without(&metadata(), "zellij-utils", &[WEB_FEATURE]).unwrap(),
            Some("plugins_from_target vendored_curl".to_owned())
        );
    }

    #[test]
    fn a_featureless_crate_is_reported_as_unaffected() {
        assert_eq!(
            features_without(&metadata(), "zellij-window", &[WINDOW_FEATURE]).unwrap(),
            None
        );
    }

    #[test]
    fn a_crate_that_is_not_in_the_workspace_is_unaffected_rather_than_an_error() {
        assert_eq!(
            features_without(&metadata(), "no-such-crate", &[WEB_FEATURE]).unwrap(),
            None
        );
    }

    #[test]
    fn the_web_exclusion_behaves_as_it_did_before_the_generalization() {
        assert_eq!(
            applicable_features(
                &zellij_defaults(),
                &owned(&["plugins_from_target", "vendored_curl", WEB_FEATURE]),
                &[WEB_FEATURE]
            ),
            Some("plugins_from_target vendored_curl".to_owned())
        );
    }
}
