use crate::compact_bar::tooltip::TooltipRenderer;
use crate::keybinds::KeybindStore;
use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::graphemes::display_width;
use zellij_tile::prelude::*;

#[derive(Default)]
pub struct ModeHints {
    slots: BTreeSet<SlotId>,
    mode_infos: BTreeMap<ClientId, ModeInfo>,
    requested_sizes: BTreeMap<SlotId, (usize, usize)>,
}

pub fn mode_label(mode: InputMode) -> String {
    format!("{:?}", mode).to_uppercase()
}

fn content_size(mode_info: &ModeInfo) -> (usize, usize) {
    let (rows, cols) = TooltipRenderer::new(mode_info).calculate_dimensions(mode_info.mode);
    let rows = if mode_info.mode == InputMode::Normal {
        rows
    } else {
        rows.saturating_sub(2)
    };
    (cols, rows.max(1))
}

pub fn wanted_size(mode_info: &ModeInfo) -> (usize, usize) {
    let (content_cols, content_rows) = content_size(mode_info);
    let title_cols = display_width(&mode_label(mode_info.mode)) + 4;
    (content_cols.max(title_cols) + 4, content_rows + 2)
}

fn render_frame(title: &str, rows: usize, cols: usize) {
    if rows < 2 || cols < 2 {
        return;
    }
    let inner = cols - 2;
    let title_room = inner.saturating_sub(4);
    let title: String = title.chars().take(title_room).collect();
    let title_width = display_width(&title);
    let top = if title_width > 0 {
        format!(
            "╭─ {} {}╮",
            title,
            "─".repeat(inner.saturating_sub(title_width + 3))
        )
    } else {
        format!("╭{}╮", "─".repeat(inner))
    };
    let title_length = title.chars().count();
    print_text_with_coordinates(
        Text::from(top).color_range(2, 3..3 + title_length),
        0,
        0,
        Some(cols),
        None,
    );
    for row in 1..rows - 1 {
        print_text_with_coordinates(Text::from("│"), 0, row, Some(1), None);
        print_text_with_coordinates(Text::from("│"), cols - 1, row, Some(1), None);
    }
    print_text_with_coordinates(
        Text::from(format!("╰{}╯", "─".repeat(inner))),
        0,
        rows - 1,
        Some(cols),
        None,
    );
}

impl ModeHints {
    pub fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }

    pub fn slot_added(&mut self, slot: &Slot) {
        self.slots.insert(slot.id);
    }

    pub fn slot_removed(&mut self, slot_id: SlotId) {
        self.slots.remove(&slot_id);
        self.requested_sizes.remove(&slot_id);
    }

    pub fn client_disconnected(&mut self, client_id: ClientId) {
        self.mode_infos.remove(&client_id);
    }

    fn all_slots(&self) -> RenderResponse {
        if self.slots.is_empty() {
            RenderResponse::Nothing
        } else {
            RenderResponse::Slots(self.slots.iter().copied().collect())
        }
    }

    pub fn update(&mut self, event: &Event, context: EventContext) -> RenderResponse {
        match event {
            Event::ModeUpdate(mode_info) => {
                let Some(client_id) = context.client_id else {
                    return RenderResponse::Nothing;
                };
                let mut mode_info = mode_info.clone();
                mode_info.keybinds = vec![];
                if self.mode_infos.get(&client_id) == Some(&mode_info) {
                    return RenderResponse::Nothing;
                }
                self.mode_infos.insert(client_id, mode_info);
                self.all_slots()
            },
            Event::InitialKeybinds(_) => self.all_slots(),
            _ => RenderResponse::Nothing,
        }
    }

    pub fn render(
        &mut self,
        rows: usize,
        cols: usize,
        slot_id: SlotId,
        client_id: ClientId,
        keybinds: &mut KeybindStore,
    ) {
        let Some(mode_info) = self.mode_infos.get_mut(&client_id) else {
            return;
        };
        let lent = keybinds.swap(client_id, &mut mode_info.keybinds);
        let wanted = wanted_size(mode_info);
        if (cols, rows) != wanted && self.requested_sizes.get(&slot_id) != Some(&wanted) {
            self.requested_sizes.insert(slot_id, wanted);
            set_popup_size(wanted.0, wanted.1);
        }
        if rows > 2 && cols > 4 {
            let content_rows = if mode_info.mode == InputMode::Normal {
                rows - 2
            } else {
                rows - 1
            };
            TooltipRenderer::new(mode_info)
                .at(
                    2,
                    if mode_info.mode == InputMode::Normal {
                        1
                    } else {
                        0
                    },
                )
                .render(content_rows, cols - 4, None);
        }
        render_frame(&mode_label(mode_info.mode), rows, cols);
        if lent {
            keybinds.swap(client_id, &mut mode_info.keybinds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode_info_with_keys(mode: InputMode) -> ModeInfo {
        ModeInfo {
            mode,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(
                InputMode::Pane,
                vec![(
                    KeyWithModifier::new(BareKey::Char('x')),
                    vec![actions::Action::CloseFocus],
                )],
            )],
            ..Default::default()
        }
    }

    #[test]
    fn the_hints_are_wide_enough_for_the_mode_name_and_the_keys() {
        let mode_info = mode_info_with_keys(InputMode::Pane);
        let (cols, rows) = wanted_size(&mode_info);
        assert!(cols >= display_width("PANE") + 8);
        assert!(rows >= 3);
    }

    #[test]
    fn a_mode_without_keys_still_gets_room_for_its_name() {
        let mode_info = ModeInfo {
            mode: InputMode::RenameTab,
            ..Default::default()
        };
        let (cols, rows) = wanted_size(&mode_info);
        assert!(cols >= display_width("RENAMETAB") + 8);
        assert!(rows >= 3);
    }
}
