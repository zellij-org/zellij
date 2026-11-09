use super::{file_system_update_within_host_pipe, PluginId, PluginInstruction};
use std::path::PathBuf;

use crate::thread_bus::ThreadSenders;
use std::path::Path;
use std::time::Duration;

use notify_debouncer_full::{
    new_debouncer,
    notify::{EventKind, RecommendedWatcher, RecursiveMode},
    DebounceEventResult, Debouncer, RecommendedCache,
};
use zellij_utils::{data::Event, errors::prelude::Result};

const DEBOUNCE_DURATION_MS: u64 = 400;

/// Translate watched host paths into the `/host/...` namespace plugins see, dropping anything
/// outside the plugin's host root. A watcher must never widen a plugin's filesystem view.
pub(crate) fn plugin_host_paths(paths: &[PathBuf], host_root: &Path) -> Vec<PathBuf> {
    let path_prefix_in_plugins = PathBuf::from("/host");
    paths
        .iter()
        .filter_map(|path| {
            path.strip_prefix(host_root)
                .ok()
                .map(|relative| path_prefix_in_plugins.join(relative))
        })
        .collect()
}

pub fn watch_filesystem(
    senders: ThreadSenders,
    plugin_id: PluginId,
    host_root: &Path,
) -> Result<Debouncer<RecommendedWatcher, RecommendedCache>> {
    let current_dir = PathBuf::from(host_root);
    let mut debouncer = new_debouncer(
        Duration::from_millis(DEBOUNCE_DURATION_MS),
        None,
        move |result: DebounceEventResult| match result {
            Ok(events) => {
                let mut create_events = vec![];
                let mut read_events = vec![];
                let mut update_events = vec![];
                let mut delete_events = vec![];
                for event in events {
                    match event.kind {
                        EventKind::Access(_) => read_events.push(event),
                        EventKind::Create(_) => create_events.push(event),
                        EventKind::Modify(_) => update_events.push(event),
                        EventKind::Remove(_) => delete_events.push(event),
                        _ => {},
                    }
                }
                let create_paths: Vec<PathBuf> = create_events
                    .drain(..)
                    .flat_map(|e| plugin_host_paths(&e.paths, &current_dir))
                    .collect();
                let read_paths: Vec<PathBuf> = read_events
                    .drain(..)
                    .flat_map(|e| plugin_host_paths(&e.paths, &current_dir))
                    .collect();
                let update_paths: Vec<PathBuf> = update_events
                    .drain(..)
                    .flat_map(|e| plugin_host_paths(&e.paths, &current_dir))
                    .collect();
                let delete_paths: Vec<PathBuf> = delete_events
                    .drain(..)
                    .flat_map(|e| plugin_host_paths(&e.paths, &current_dir))
                    .collect();
                // TODO: at some point we might want to add FileMetadata to these, but right now
                // the API is a bit unstable, so let's not rock the boat too much by adding another
                // expensive syscall
                let mut updates = vec![];
                if !read_paths.is_empty() {
                    updates.push((
                        Some(plugin_id),
                        None,
                        Event::FileSystemRead(read_paths.into_iter().map(|p| (p, None)).collect()),
                    ));
                }
                if !create_paths.is_empty() {
                    updates.push((
                        Some(plugin_id),
                        None,
                        Event::FileSystemCreate(
                            create_paths.into_iter().map(|p| (p, None)).collect(),
                        ),
                    ));
                }
                if !update_paths.is_empty() {
                    let entries = file_system_update_within_host_pipe(
                        update_paths.into_iter().map(|p| (p, None)).collect(),
                    );
                    updates.push((
                        Some(plugin_id),
                        None,
                        Event::FileSystemUpdate(entries),
                    ));
                }
                if !delete_paths.is_empty() {
                    updates.push((
                        Some(plugin_id),
                        None,
                        Event::FileSystemDelete(
                            delete_paths.into_iter().map(|p| (p, None)).collect(),
                        ),
                    ));
                }
                if !updates.is_empty() {
                    let _ = senders.send_to_plugin(PluginInstruction::Update(updates));
                }
            },
            Err(errors) => errors
                .iter()
                .for_each(|error| log::error!("watch error: {error:?}")),
        },
    )?;

    debouncer.watch(host_root, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watched_paths_are_scoped_to_the_plugin_host_root() {
        let root = PathBuf::from("/tmp/plugin-host");
        let paths = vec![
            root.join("inside.txt"),
            root.join("nested/also-inside.txt"),
            PathBuf::from("/tmp/outside.txt"),
            PathBuf::from("/etc/passwd"),
        ];
        assert_eq!(
            plugin_host_paths(&paths, &root),
            vec![
                PathBuf::from("/host/inside.txt"),
                PathBuf::from("/host/nested/also-inside.txt"),
            ],
            "paths outside the plugin host root must not reach the plugin"
        );
    }
}
