use crate::state::positioning;
use std::path::PathBuf;
use zellij_tile::prelude::*;

pub struct Layout {
    pub needs_reposition: bool,
    pub cached_cursor_position: Option<(usize, usize)>,
    pub spinner_frame: usize,
    pub spinner_timer_scheduled: bool,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            needs_reposition: false,
            cached_cursor_position: None,
            spinner_frame: 0,
            spinner_timer_scheduled: false,
        }
    }

    pub fn reposition_plugin_based_on_sequence_state(
        &mut self,
        is_running: bool,
        first_command_pane_id: Option<PaneId>,
        pane_manifest: &PaneManifest,
        plugin_id: Option<u32>,
        total_viewport_columns: Option<usize>,
        total_viewport_rows: Option<usize>,
        total_commands: usize,
        _selected_index: Option<usize>,
        _running_index: usize,
        _cwd: &Option<PathBuf>,
    ) {
        let Some(plugin_id) = plugin_id else {
            return;
        };
        let (Some(total_cols), Some(total_rows)) = (total_viewport_columns, total_viewport_rows)
        else {
            return;
        };

        if !self.needs_reposition || !is_running {
            return;
        }

        let Some(first_pane_id) = first_command_pane_id else {
            return;
        };

        for (_tab_index, panes) in &pane_manifest.panes {
            for pane in panes {
                if !pane.is_plugin && PaneId::Terminal(pane.id) == first_pane_id {
                    positioning::reposition_plugin_for_sequence(
                        plugin_id,
                        total_cols,
                        total_rows,
                        total_commands,
                    );

                    self.needs_reposition = false;
                    return;
                }
            }
        }
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}
