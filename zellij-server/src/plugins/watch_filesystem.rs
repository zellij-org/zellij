use super::PluginInstruction;
use std::path::PathBuf;
use std::collections::HashMap;

use crate::thread_bus::ThreadSenders;
use std::path::Path;
use std::time::Duration;

use zellij_utils::notify_debouncer_full::{
    new_debouncer,
    notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher},
    DebounceEventResult, Debouncer, FileIdMap,
};
use zellij_utils::{data::Event, errors::prelude::Result};

const DEBOUNCE_DURATION_MS: u64 = 400;

pub fn watch_filesystem(
    senders: ThreadSenders,
    zellij_cwd: &Path,
    files_in: &HashMap<PathBuf, bool>,
) -> Result<Debouncer<RecommendedWatcher, FileIdMap>> {
    let path_prefix_in_plugins = PathBuf::from("/host");
    let current_dir = PathBuf::from(zellij_cwd);
    let current_dir_clone = current_dir.clone();
    let path_prefix_in_plugins_clone = path_prefix_in_plugins.clone();
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
                    .map(|e| {
                        e.paths
                            .iter()
                            .map(|p| {
                                let stripped_prefix_path =
                                    p.strip_prefix(&current_dir).unwrap_or_else(|_| p);
                                path_prefix_in_plugins.join(stripped_prefix_path)
                            })
                            .collect()
                    })
                    .collect();
                let read_paths: Vec<PathBuf> = read_events
                    .drain(..)
                    .map(|e| {
                        e.paths
                            .iter()
                            .map(|p| {
                                let stripped_prefix_path =
                                    p.strip_prefix(&current_dir).unwrap_or_else(|_| p);
                                path_prefix_in_plugins.join(stripped_prefix_path)
                            })
                            .collect()
                    })
                    .collect();
                let update_paths: Vec<PathBuf> = update_events
                    .drain(..)
                    .map(|e| {
                        e.paths
                            .iter()
                            .map(|p| {
                                let stripped_prefix_path =
                                    p.strip_prefix(&current_dir).unwrap_or_else(|_| p);
                                path_prefix_in_plugins.join(stripped_prefix_path)
                            })
                            .collect()
                    })
                    .collect();
                let delete_paths: Vec<PathBuf> = delete_events
                    .drain(..)
                    .map(|e| {
                        e.paths
                            .iter()
                            .map(|p| {
                                let stripped_prefix_path =
                                    p.strip_prefix(&current_dir).unwrap_or_else(|_| p);
                                path_prefix_in_plugins.join(stripped_prefix_path)
                            })
                            .collect()
                    })
                    .collect();
                // TODO: at some point we might want to add FileMetadata to these, but right now
                // the API is a bit unstable, so let's not rock the boat too much by adding another
                // expensive syscall
                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![
                    (
                        None,
                        None,
                        Event::FileSystemRead(read_paths.into_iter().map(|p| (p, None)).collect()),
                    ),
                    (
                        None,
                        None,
                        Event::FileSystemCreate(
                            create_paths.into_iter().map(|p| (p, None)).collect(),
                        ),
                    ),
                    (
                        None,
                        None,
                        Event::FileSystemUpdate(
                            update_paths.into_iter().map(|p| (p, None)).collect(),
                        ),
                    ),
                    (
                        None,
                        None,
                        Event::FileSystemDelete(
                            delete_paths.into_iter().map(|p| (p, None)).collect(),
                        ),
                    ),
                ]));
            },
            Err(errors) => errors
                .iter()
                .for_each(|error| log::error!("watch error: {error:?}")),
        },
    )?;

    let default_files = HashMap::from([(PathBuf::from(zellij_cwd), true)]);
    let files = match files_in.is_empty()
    {
        true => &default_files,
        false => files_in
    };

    for (key, &value) in files
    {
        let path_buf = current_dir_clone.join(key.strip_prefix(&path_prefix_in_plugins_clone).unwrap_or_else(|_| key));
        if path_buf.exists()
        {
            let path = Path::new(path_buf.to_str().unwrap());
            if value
            {
                // If true, then we want to recurse
                debouncer
                    .watcher()
                    .watch(&path, RecursiveMode::Recursive)?;
            }
            else
            {
                // If false, then we do not want to recurse
                debouncer
                    .watcher()
                    .watch(&path, RecursiveMode::NonRecursive)?;
            }
        }
        else
        {
            let path_str = path_buf.to_str().unwrap();
            log::error!("Cannot watch file {path_str} since it does not exist.")
        }
    }
    Ok(debouncer)
}
