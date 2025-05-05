mod line;
mod tab;

use std::cmp::{max, min};
use std::collections::BTreeMap;
use std::convert::TryInto;

use tab::get_tab_to_focus;
use zellij_tile::prelude::*;

use crate::line::tab_line;
use crate::tab::tab_style;

#[derive(Debug, Default)]
pub struct LinePart {
    part: String,
    len: usize,
    tab_index: Option<usize>,
}

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    active_tab_idx: usize,
    mode_info: ModeInfo,
    tab_line: Vec<LinePart>,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    own_client_id: Option<ClientId>,
    grouped_panes_count: Option<usize>,
}

static ARROW_SEPARATOR: &str = "";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        set_selectable(false);
        subscribe(&[
            EventType::TabUpdate,
            EventType::ModeUpdate,
            EventType::Mouse,
            EventType::CopyToClipboard,
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
            EventType::PaneUpdate,
        ]);
        self.own_client_id = Some(get_plugin_ids().client_id);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info
            },
            Event::TabUpdate(tabs) => {
                if let Some(active_tab_index) = tabs.iter().position(|t| t.active) {
                    // tabs are indexed starting from 1 so we need to add 1
                    let active_tab_idx = active_tab_index + 1;
                    if self.active_tab_idx != active_tab_idx || self.tabs != tabs {
                        should_render = true;
                    }
                    self.active_tab_idx = active_tab_idx;
                    self.tabs = tabs;
                } else {
                    eprintln!("Could not find active tab.");
                }
            },
            Event::Mouse(me) => match me {
                Mouse::LeftClick(_, col) => {
                    let tab_to_focus = get_tab_to_focus(&self.tab_line, self.active_tab_idx, col);
                    if let Some(idx) = tab_to_focus {
                        switch_tab_to(idx.try_into().unwrap());
                    }
                },
                Mouse::ScrollUp(_) => {
                    switch_tab_to(min(self.active_tab_idx + 1, self.tabs.len()) as u32);
                },
                Mouse::ScrollDown(_) => {
                    switch_tab_to(max(self.active_tab_idx.saturating_sub(1), 1) as u32);
                },
                _ => {},
            },
            Event::CopyToClipboard(copy_destination) => {
                match self.text_copy_destination {
                    Some(text_copy_destination) => {
                        if text_copy_destination != copy_destination {
                            should_render = true;
                        }
                    },
                    None => {
                        should_render = true;
                    },
                }
                self.text_copy_destination = Some(copy_destination);
            },
            Event::SystemClipboardFailure => {
                should_render = true;
                self.display_system_clipboard_failure = true;
            },
            Event::InputReceived => {
                if self.text_copy_destination.is_some()
                    || self.display_system_clipboard_failure == true
                {
                    should_render = true;
                }
                self.text_copy_destination = None;
                self.display_system_clipboard_failure = false;
            },
            Event::PaneUpdate(pane_manifest) => {
                if let Some(own_client_id) = self.own_client_id {
                    let mut grouped_panes_count = 0;
                    for (_tab_index, pane_infos) in pane_manifest.panes {
                        for pane_info in pane_infos {
                            let is_in_pane_group =
                                pane_info.index_in_pane_group.get(&own_client_id).is_some();
                            if is_in_pane_group {
                                grouped_panes_count += 1;
                            }
                        }
                    }
                    if Some(grouped_panes_count) != self.grouped_panes_count {
                        if grouped_panes_count == 0 {
                            self.grouped_panes_count = None;
                        } else {
                            self.grouped_panes_count = Some(grouped_panes_count);
                        }
                        should_render = true;
                    }
                }
            },
            _ => {
                eprintln!("Got unrecognized event: {:?}", event);
            },
        };
        should_render
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        if let Some(copy_destination) = self.text_copy_destination {
            let hint = text_copied_hint(copy_destination).part;

            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", hint, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", hint, color);
                },
            }
        } else if self.display_system_clipboard_failure {
            let hint = system_clipboard_error().part;
            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", hint, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", hint, color);
                },
            }
        } else {
            if self.tabs.is_empty() {
                return;
            }
            let mut all_tabs: Vec<LinePart> = vec![];
            let mut active_tab_index = 0;
            let mut active_swap_layout_name = None;
            let mut is_swap_layout_dirty = false;
            let mut is_alternate_tab = false;
            for t in &mut self.tabs {
                let mut tabname = t.name.clone();
                if t.active && self.mode_info.mode == InputMode::RenameTab {
                    if tabname.is_empty() {
                        tabname = String::from("Enter name...");
                    }
                    active_tab_index = t.position;
                } else if t.active {
                    active_tab_index = t.position;
                    is_swap_layout_dirty = t.is_swap_layout_dirty;
                    active_swap_layout_name = t.active_swap_layout_name.clone();
                }
                let tab = tab_style(
                    tabname,
                    t,
                    is_alternate_tab,
                    self.mode_info.style.colors,
                    self.mode_info.capabilities,
                );
                is_alternate_tab = !is_alternate_tab;
                all_tabs.push(tab);
            }
            self.tab_line = tab_line(
                self.mode_info.session_name.as_deref(),
                self.mode_info.web_clients_allowed.unwrap_or(false),
                all_tabs,
                active_tab_index,
                cols.saturating_sub(1),
                self.mode_info.style.colors,
                self.mode_info.capabilities,
                self.mode_info.style.hide_session_name,
                self.mode_info.mode,
                &active_swap_layout_name,
                is_swap_layout_dirty,
                &self.mode_info,
                self.grouped_panes_count,
            );
            let output = self
                .tab_line
                .iter()
                .fold(String::new(), |output, part| output + &part.part);
            let background = self.mode_info.style.colors.text_unselected.background;
            match background {
                PaletteColor::Rgb((r, g, b)) => {
                    print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", output, r, g, b);
                },
                PaletteColor::EightBit(color) => {
                    print!("{}\u{1b}[48;5;{}m\u{1b}[0K", output, color);
                },
            }
        }
    }
}

pub fn text_copied_hint(copy_destination: CopyDestination) -> LinePart {
    let hint = match copy_destination {
        CopyDestination::Command => "Text piped to external command",
        #[cfg(not(target_os = "macos"))]
        CopyDestination::Primary => "Text copied to system primary selection",
        #[cfg(target_os = "macos")] // primary selection does not exist on macos
        CopyDestination::Primary => "Text copied to system clipboard",
        CopyDestination::System => "Text copied to system clipboard",
    };
    LinePart {
        part: serialize_text(&Text::new(&hint).color_range(2, ..).opaque()),
        len: hint.len(),
        tab_index: None,
    }
}

pub fn system_clipboard_error() -> LinePart {
    let hint = " Error using the system clipboard.";
    LinePart {
        part: serialize_text(&Text::new(&hint).color_range(2, ..).opaque()),
        len: hint.len(),
        tab_index: None,
    }
}
