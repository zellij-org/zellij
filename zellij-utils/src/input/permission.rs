use std::{
    collections::{hash_map::Iter, HashMap},
    fs,
};

use crate::{
    data::PermissionType, consts::ZELLIJ_PLUGIN_PERMISSIONS_CACHE,
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

    pub fn from_cache_or_default() -> Self {
        let default_permission = ZELLIJ_PLUGIN_PERMISSIONS_CACHE.to_path_buf();

        match fs::read_to_string(&default_permission) {
            Ok(s) => GrantedPermission::from_string(s).unwrap_or_default(),
            Err(_) => GrantedPermission::default(),
        }
    }
}
