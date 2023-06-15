use super::PluginInstruction;
use std::path::PathBuf;

use crate::thread_bus::ThreadSenders;
use std::path::Path;
use std::time::Duration;

use zellij_utils::notify_debouncer_full::{
    new_debouncer,
    notify::{EventKind, INotifyWatcher, RecursiveMode, Watcher},
    DebounceEventResult, Debouncer, FileIdMap,
};
use zellij_utils::{data::Event, errors::prelude::Result};

const DEBOUNCE_DURATION_MS: u64 = 500;

pub fn watch_filesystem(
    senders: ThreadSenders,
    zellij_cwd: &Path,
) -> Result<Debouncer<INotifyWatcher, FileIdMap>> {
    let path_prefix_in_plugins = PathBuf::from("/host");
    let current_dir = PathBuf::from(zellij_cwd);
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
                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![
                    (None, None, Event::FileSystemRead(read_paths)),
                    (None, None, Event::FileSystemCreate(create_paths)),
                    (None, None, Event::FileSystemUpdate(update_paths)),
                    (None, None, Event::FileSystemDelete(delete_paths)),
                ]));
            },
            Err(errors) => errors
                .iter()
                .for_each(|error| log::error!("watch error: {error:?}")),
        },
    )?;

    debouncer
        .watcher()
        .watch(zellij_cwd, RecursiveMode::Recursive)?;
    Ok(debouncer)
}
