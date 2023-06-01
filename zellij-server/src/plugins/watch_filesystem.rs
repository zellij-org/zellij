use super::PluginInstruction;
use std::path::PathBuf;

use crate::thread_bus::ThreadSenders;
use std::path::Path;

use zellij_utils::{
    data::Event,
    errors::prelude::*,
};

use zellij_utils::notify::{self, Watcher, RecommendedWatcher, RecursiveMode, EventKind};
pub fn watch_filesystem(senders: ThreadSenders, zellij_cwd: &Path) -> Result<RecommendedWatcher> {
    let path_prefix_in_plugins = PathBuf::from("/host");
    let current_dir = PathBuf::from(zellij_cwd);
    let mut watcher = notify::recommended_watcher({
        move |res: notify::Result<notify::Event>| {
            match res {
               Ok(event) => {
                   let paths: Vec<PathBuf> = event.paths.iter().map(|p| {
                       let stripped_prefix_path = p.strip_prefix(&current_dir).unwrap_or_else(|_| p);
                       path_prefix_in_plugins.join(stripped_prefix_path)
                   }).collect();
                   match event.kind {
                       EventKind::Access(_) => {
                           let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(None, None, Event::FileSystemRead(paths))]));
                       }
                       EventKind::Create(_) => {
                           let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(None, None, Event::FileSystemCreate(paths))]));
                       }
                       EventKind::Modify(_) => {
                           let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(None, None, Event::FileSystemUpdate(paths))]));
                       }
                       EventKind::Remove(_) => {
                           let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(None, None, Event::FileSystemDelete(paths))]));
                       }
                       _ => {}
                   }
               }
               Err(e) => log::error!("watch error: {:?}", e),
            }
        }
    })?;

    watcher.watch(zellij_cwd, RecursiveMode::Recursive)?;
    Ok(watcher)
}
