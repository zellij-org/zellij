/// Uniformly operates ZELLIJ* environment variables
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::input::config::ConfigError;
use kdl::KdlNode;
use crate::{kdl_children_nodes_or_error, kdl_name, kdl_first_entry_as_string, kdl_first_entry_as_i64};
use std::{
    collections::HashMap,
    env::{set_var, var},
};

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

pub fn set_initial_environment_vars() {
    set_var("COLORTERM", "24bit");
}

pub const SOCKET_DIR_ENV_KEY: &str = "ZELLIJ_SOCKET_DIR";
pub fn get_socket_dir() -> Result<String> {
    Ok(var(SOCKET_DIR_ENV_KEY)?)
}

/// Manage ENVIRONMENT VARIABLES from the configuration and the layout files
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentVariables {
    env: HashMap<String, String>,
}

impl EnvironmentVariables {
    /// Merges two structs, keys from `other` supersede keys from `self`
    pub fn merge(&self, other: Self) -> Self {
        let mut env = self.clone();
        env.env.extend(other.env);
        env
    }
    pub fn from_data(data: HashMap<String, String>) -> Self {
        EnvironmentVariables {
            env: data
        }
    }
    pub fn from_kdl(kdl_env_variables: &KdlNode) -> Result<Self, ConfigError> {
        let mut env: HashMap<String, String> = HashMap::new();
        for env_var in kdl_children_nodes_or_error!(kdl_env_variables, "empty env variable block") {
            let env_var_name = kdl_name!(env_var);
            let env_var_str_value = kdl_first_entry_as_string!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_int_value = kdl_first_entry_as_i64!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_value = env_var_str_value
                .or(env_var_int_value)
                .ok_or::<Box<dyn std::error::Error>>(format!("Failed to parse env var: {:?}", env_var_name).into())?;
            env.insert(env_var_name.into(), env_var_value);
        }
        Ok(EnvironmentVariables::from_data(env))
    }

    /// Set all the ENVIRONMENT VARIABLES, that are configured
    /// in the configuration and layout files
    pub fn set_vars(&self) {
        for (k, v) in &self.env {
            set_var(k, v);
        }
    }
}
