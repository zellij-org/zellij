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

/// Get the appropriate features string for a crate when --no-web is enabled
/// Returns Some(features_string) if the crate has web_server_capability and should use --no-default-features
/// Returns None if the crate doesn't have web_server_capability and should use normal build
pub fn get_no_web_features(sh: &Shell, crate_name: &str) -> anyhow::Result<Option<String>> {
    let metadata = get_cargo_metadata(sh)?;

    let packages = metadata["packages"]
        .as_array()
        .context("Expected packages array in metadata")?;

    // First, find the main zellij crate to get the default features
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
                        if feature_name != "web_server_capability" {
                            main_default_features.push(feature_name);
                        }
                    }
                }
            }
            break;
        }
    }

    // Now check if the target crate has web_server_capability and filter features
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

            // Check if this crate has web_server_capability feature
            if !features.contains_key("web_server_capability") {
                return Ok(None);
            }

            // This crate has web_server_capability, so we need to use --no-default-features
            // Only include features that this crate actually has
            let mut applicable_features = Vec::new();
            for feature_name in &main_default_features {
                if features.contains_key(*feature_name) {
                    applicable_features.push(*feature_name);
                }
            }

            // Return the feature string (even if empty) to indicate we should use --no-default-features
            return Ok(Some(applicable_features.join(" ")));
        }
    }

    Ok(None)
}
