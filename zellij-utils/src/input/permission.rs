use std::{
    collections::{hash_map::Iter, HashMap},
    fs,
};

use crate::{
    consts::{ZELLIJ_CACHE_DIR, ZELLIJ_PLUGIN_PERMISSIONS_FILE},
    data::PermissionType,
    input::config::ConfigError,
};

#[derive(Default, Debug)]
pub struct GrantedPermission(HashMap<String, Vec<PermissionType>>);

impl GrantedPermission {
    pub fn insert(&mut self, k: String, v: Vec<PermissionType>) {
        self.0.insert(k, v);
    }

    pub fn get(&self, k: &String) -> Option<&Vec<PermissionType>> {
        self.0.get(k)
    }

    pub fn iter(&self) -> Iter<String, Vec<PermissionType>> {
        self.0.iter()
    }

    pub fn from_default() -> Result<Self, ConfigError> {
        let default_permission = ZELLIJ_CACHE_DIR.join(ZELLIJ_PLUGIN_PERMISSIONS_FILE);

        let raw_string = fs::read_to_string(&default_permission)
            .map_err(|e| ConfigError::IoPath(e, default_permission.into()))?;

        GrantedPermission::from_string(raw_string)
    }
}
