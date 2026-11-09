use std::{
    collections::HashMap,
    fs,
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
        let contents = PermissionCache::to_string(&self.granted);
        if let Ok(existing) = fs::read_to_string(&self.path) {
            if existing == contents {
                return Ok(());
            }
        }
        write_atomically(&self.path, contents.as_bytes())
    }
}

fn write_atomically(path: &PathBuf, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "permission cache path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".zellij-permissions-")
        .tempfile_in(parent)?;
    temporary.write_all(contents)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::PermissionType;
    use std::collections::HashMap;

    #[test]
    fn write_to_file_replaces_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        fs::write(&path, "old").unwrap();

        let mut granted = HashMap::new();
        granted.insert(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        );
        let cache = PermissionCache {
            path: path.clone(),
            granted,
        };
        cache.write_to_file().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("example"));
        assert!(!written.contains("old"));
        let leftover_temps: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".zellij-permissions-")
            })
            .collect();
        assert!(
            leftover_temps.is_empty(),
            "temporary permission files must not remain: {:?}",
            leftover_temps
        );
    }

    #[test]
    fn write_to_file_skips_unchanged_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let mut granted = HashMap::new();
        granted.insert(
            "example".to_owned(),
            vec![PermissionType::ReadApplicationState],
        );
        let cache = PermissionCache {
            path: path.clone(),
            granted,
        };
        cache.write_to_file().unwrap();
        let first_modified = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.write_to_file().unwrap();
        let second_modified = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(first_modified, second_modified);
    }
}
