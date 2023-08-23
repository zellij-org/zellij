use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use crate::{consts::ZELLIJ_PLUGIN_PERMISSIONS_CACHE, data::PermissionType};

pub type GrantedPermission = HashMap<String, Vec<PermissionType>>;

#[derive(Default, Debug)]
pub struct PermissionCache {
    path: PathBuf,
    granted: GrantedPermission,
}

impl PermissionCache {
    pub fn cache(&mut self, plugin_name: String, permissions: Vec<PermissionType>) {
        self.granted.insert(plugin_name, permissions);
    }

    pub fn get_permissions(&self, plugin_name: String) -> Option<&Vec<PermissionType>> {
        self.granted.get(&plugin_name)
    }

    pub fn check_permissions(
        &self,
        plugin_name: String,
        permissions_to_check: &Vec<PermissionType>,
    ) -> bool {
        if let Some(target) = self.granted.get(&plugin_name) {
            let mut all_granted = true;
            for permission in permissions_to_check {
                if !target.contains(permission) {
                    all_granted = false;
                }
            }
            return all_granted;
        }

        false
    }

    pub fn from_path_or_default(cache_path: Option<PathBuf>) -> Self {
        let cache_path = cache_path.unwrap_or(ZELLIJ_PLUGIN_PERMISSIONS_CACHE.to_path_buf());

        let granted = match fs::read_to_string(cache_path.clone()) {
            Ok(raw_string) => PermissionCache::from_string(raw_string).unwrap_or_default(),
            Err(e) => {
                log::error!("Failed to read permission cache file: {}", e);
                GrantedPermission::default()
            },
        };

        PermissionCache {
            path: cache_path,
            granted,
        }
    }

    pub fn write_to_file(&self) -> std::io::Result<()> {
        let mut f = File::create(&self.path)?;
        write!(f, "{}", PermissionCache::to_string(&self.granted))?;
        Ok(())
    }
}
