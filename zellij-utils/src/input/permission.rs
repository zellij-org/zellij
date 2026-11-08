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
    /// Records the plugin's permissions, and returns whether that changed what is cached.
    pub fn cache(&mut self, plugin_name: String, permissions: Vec<PermissionType>) -> bool {
        let unchanged = self.granted.get(&plugin_name).is_some_and(|cached| {
            cached.len() == permissions.len() && permissions.iter().all(|p| cached.contains(p))
        });
        self.granted.insert(plugin_name, permissions);
        !unchanged
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

    /// Replaces the file in one step, so a plugin loading meanwhile never reads it half written.
    pub fn write_to_file(&self) -> std::io::Result<()> {
        let file_name = self
            .path
            .file_name()
            .ok_or_else(|| std::io::Error::other("the permission cache path has no file name"))?
            .to_string_lossy();
        let temporary =
            self.path
                .with_file_name(format!(".{}.{}.tmp", file_name, std::process::id()));
        {
            let mut f = File::create(&temporary)?;
            write!(f, "{}", PermissionCache::to_string(&self.granted))?;
            f.sync_all()?;
        }
        fs::rename(&temporary, &self.path).inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_at(path: &std::path::Path) -> PermissionCache {
        PermissionCache::from_path_or_default(Some(path.to_path_buf()))
    }

    #[test]
    fn caching_the_permissions_already_cached_reports_no_change() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = cache_at(&dir.path().join("permissions.kdl"));
        let permissions = vec![
            PermissionType::ReadApplicationState,
            PermissionType::RunCommands,
        ];
        assert!(cache.cache("plugin".to_owned(), permissions.clone()));
        assert!(!cache.cache("plugin".to_owned(), permissions.into_iter().rev().collect()));
        assert!(cache.cache(
            "plugin".to_owned(),
            vec![PermissionType::ReadApplicationState]
        ));
        assert!(cache.cache(
            "other plugin".to_owned(),
            vec![PermissionType::ReadApplicationState]
        ));
    }

    #[test]
    fn writing_the_cache_replaces_the_file_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        fs::write(&path, "stale").unwrap();
        let mut cache = cache_at(&path);
        cache.cache("plugin".to_owned(), vec![PermissionType::RunCommands]);
        cache.write_to_file().unwrap();

        let read_back = cache_at(&path);
        assert!(
            read_back.check_permissions("plugin".to_owned(), &vec![PermissionType::RunCommands])
        );
        let files: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(
            files.len(),
            1,
            "only the cache itself is left in the folder"
        );
    }

    #[test]
    fn a_reader_never_sees_the_cache_empty_while_it_is_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.kdl");
        let mut cache = cache_at(&path);
        cache.cache("plugin".to_owned(), vec![PermissionType::RunCommands]);
        cache.write_to_file().unwrap();

        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            let mut cache = cache_at(&writer_path);
            for _ in 0..2000 {
                cache.cache("plugin".to_owned(), vec![PermissionType::RunCommands]);
                cache.write_to_file().unwrap();
            }
        });
        let mut reads = 0;
        while !writer.is_finished() {
            assert!(
                cache_at(&path)
                    .check_permissions("plugin".to_owned(), &vec![PermissionType::RunCommands]),
                "a read during a rewrite lost the cached grant"
            );
            reads += 1;
        }
        writer.join().unwrap();
        assert!(reads > 0);
    }
}
