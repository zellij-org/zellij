use std::collections::{hash_map::Iter, HashMap};

use crate::data::PermissionType;

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
}
