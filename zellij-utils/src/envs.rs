/// Uniformly operates ZELLIJ* environment variables
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    env::{set_var, var},
};

use std::fmt;

pub const ZELLIJ_ENV_KEY: &str = "ZELLIJ";
pub fn get_zellij() -> Result<String> {
    Ok(var(ZELLIJ_ENV_KEY)?)
}
pub fn set_zellij(v: String) {
    set_var(ZELLIJ_ENV_KEY, v);
}

pub const SESSION_NAME_ENV_KEY: &str = "ZELLIJ_SESSION_NAME";

pub fn get_session_name() -> Result<String> {
    Ok(var(SESSION_NAME_ENV_KEY)?)
}

pub fn set_session_name(v: String) {
    set_var(SESSION_NAME_ENV_KEY, v);
}

pub const SOCKET_DIR_ENV_KEY: &str = "ZELLIJ_SOCKET_DIR";

pub const ENV_BEFORE_CONFIG_KEY: &str = "ZELLIJ_ENV_BEFORE_CONFIG";

pub fn take_env_values_before_config() -> HashMap<String, Option<String>> {
    let values = var(ENV_BEFORE_CONFIG_KEY)
        .ok()
        .and_then(|encoded| serde_json::from_str(&encoded).ok())
        .unwrap_or_default();
    std::env::remove_var(ENV_BEFORE_CONFIG_KEY);
    values
}
pub fn get_socket_dir() -> Result<String> {
    Ok(var(SOCKET_DIR_ENV_KEY)?)
}

/// Manage ENVIRONMENT VARIABLES from the configuration and the layout files
#[derive(Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentVariables {
    env: HashMap<String, String>,
}

impl fmt::Debug for EnvironmentVariables {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut stable_sorted = BTreeMap::new();
        for (env_var_name, env_var_value) in self.env.iter() {
            stable_sorted.insert(env_var_name, env_var_value);
        }
        write!(f, "{:#?}", stable_sorted)
    }
}

impl EnvironmentVariables {
    /// Merges two structs, keys from `other` supersede keys from `self`
    pub fn merge(&self, other: Self) -> Self {
        let mut env = self.clone();
        env.env.extend(other.env);
        env
    }
    pub fn from_data(data: HashMap<String, String>) -> Self {
        EnvironmentVariables { env: data }
    }
    /// Set all the ENVIRONMENT VARIABLES, that are configured
    /// in the configuration and layout files
    pub fn set_vars(&self) {
        if self.env.is_empty() {
            return;
        }
        let mut values_before_config: HashMap<String, Option<String>> =
            var(ENV_BEFORE_CONFIG_KEY)
                .ok()
                .and_then(|encoded| serde_json::from_str(&encoded).ok())
                .unwrap_or_default();
        for (k, v) in &self.env {
            values_before_config
                .entry(k.clone())
                .or_insert_with(|| var(k).ok());
            set_var(k, v);
        }
        if let Ok(encoded) = serde_json::to_string(&values_before_config) {
            set_var(ENV_BEFORE_CONFIG_KEY, encoded);
        }
    }
    pub fn inner(&self) -> &HashMap<String, String> {
        &self.env
    }
}

#[cfg(test)]
mod env_before_config_tests {
    use super::*;

    #[test]
    fn values_before_the_config_set_its_variables_are_handed_over_once() {
        let had_a_value = "ZELLIJ_ENV_BEFORE_CONFIG_TEST_HAD_A_VALUE";
        let had_none = "ZELLIJ_ENV_BEFORE_CONFIG_TEST_HAD_NONE";
        set_var(had_a_value, "from the shell");
        std::env::remove_var(had_none);
        let mut data = HashMap::new();
        data.insert(had_a_value.to_owned(), "from the config".to_owned());
        data.insert(had_none.to_owned(), "also from the config".to_owned());
        EnvironmentVariables::from_data(data.clone()).set_vars();
        EnvironmentVariables::from_data(data).set_vars();
        assert_eq!(var(had_a_value).unwrap(), "from the config");
        let before = take_env_values_before_config();
        assert_eq!(
            before.get(had_a_value),
            Some(&Some("from the shell".to_owned()))
        );
        assert_eq!(before.get(had_none), Some(&None));
        assert!(var(ENV_BEFORE_CONFIG_KEY).is_err());
        std::env::remove_var(had_a_value);
        std::env::remove_var(had_none);
    }
}
