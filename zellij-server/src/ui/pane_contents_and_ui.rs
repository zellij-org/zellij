use crate::output::Output;
use crate::panes::PaneId;
use crate::tab::Pane;
use crate::ui::boundaries::Boundaries;
use crate::ui::pane_boundaries_frame::FrameParams;
use crate::ClientId;
use std::collections::HashMap;
use zellij_tile::{
    data::{client_id_to_colors, single_client_color, InputMode, PaletteColor},
    prelude::Style,
};
pub struct PaneContentsAndUi<'a> {
    pane: &'a mut Box<dyn Pane>,
    output: &'a mut Output,
    style: Style,
    focused_clients: Vec<ClientId>,
    multiple_users_exist_in_session: bool,
    z_index: Option<usize>,
}

impl<'a> PaneContentsAndUi<'a> {
    pub fn new(
        pane: &'a mut Box<dyn Pane>,
        output: &'a mut Output,
        style: Style,
        active_panes: &HashMap<ClientId, PaneId>,
        multiple_users_exist_in_session: bool,
        z_index: Option<usize>,
    ) -> Self {
        let focused_clients: Vec<ClientId> = active_panes
            .iter()
            .filter(|(_c_id, p_id)| **p_id == pane.pid())
            .map(|(c_id, _p_id)| *c_id)
            .collect();
        PaneContentsAndUi {
            pane,
            output,
            style,
            focused_clients,
            multiple_users_exist_in_session,
            z_index,
        }
    }
    pub fn render_pane_contents_to_multiple_clients(
        &mut self,
        clients: impl Iterator<Item = ClientId>,
    ) {
        if let Some((character_chunks, raw_vte_output, sixel_image_chunks)) = self.pane.render(None)
        {
            let clients: Vec<ClientId> = clients.collect();
            self.output.add_character_chunks_to_multiple_clients(
                character_chunks,
                clients.iter().copied(),
                self.z_index,
            );
            self.output.add_sixel_image_chunks_to_multiple_clients(
                sixel_image_chunks,
                clients.iter().copied(),
                self.z_index,
            );
            if let Some(raw_vte_output) = raw_vte_output {
                self.output.add_post_vte_instruction_to_multiple_clients(
                    clients.iter().copied(),
                    &format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        self.pane.y() + 1,
                        self.pane.x() + 1,
                        raw_vte_output
                    ),
                );
            }
        }
    }
    pub fn render_pane_contents_for_client(&mut self, client_id: ClientId) {
        if let Some((character_chunks, raw_vte_output, sixel_image_chunks)) =
            self.pane.render(Some(client_id))
        {
            self.output
                .add_character_chunks_to_client(client_id, character_chunks, self.z_index);
            self.output.add_sixel_image_chunks_to_client(
                client_id,
                sixel_image_chunks,
                self.z_index,
            );
            if let Some(raw_vte_output) = raw_vte_output {
                self.output.add_post_vte_instruction_to_client(
                    client_id,
                    &format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        self.pane.y() + 1,
                        self.pane.x() + 1,
                        raw_vte_output
                    ),
                );
            }
        }
    }
    pub fn render_fake_cursor_if_needed(&mut self, client_id: ClientId) {
        let pane_focused_for_client_id = self.focused_clients.contains(&client_id);
        let pane_focused_for_different_client = self
            .focused_clients
            .iter()
            .filter(|&&c_id| c_id != client_id)
            .count()
            > 0;
        if pane_focused_for_different_client && !pane_focused_for_client_id {
            let fake_cursor_client_id = self
                .focused_clients
                .iter()
                .find(|&&c_id| c_id != client_id)
                .unwrap();
            if let Some(colors) = client_id_to_colors(*fake_cursor_client_id, self.style.colors) {
                if let Some(vte_output) = self.pane.render_fake_cursor(colors.0, colors.1) {
                    self.output.add_post_vte_instruction_to_client(
                        client_id,
                        &format!(
                            "\u{1b}[{};{}H\u{1b}[m{}",
                            self.pane.y() + 1,
                            self.pane.x() + 1,
                            vte_output
                        ),
                    );
                }
            }
        }
    }
    pub fn render_terminal_title_if_needed(&mut self, client_id: ClientId, client_mode: InputMode) {
        if !self.focused_clients.contains(&client_id) {
            return;
        }
        let vte_output = self.pane.render_terminal_title(client_mode);
        self.output
            .add_post_vte_instruction_to_client(client_id, &vte_output);
    }
    pub fn render_pane_frame(
        &mut self,
        client_id: ClientId,
        client_mode: InputMode,
        session_is_mirrored: bool,
    ) {
        let pane_focused_for_client_id = self.focused_clients.contains(&client_id);
        let other_focused_clients: Vec<ClientId> = self
            .focused_clients
            .iter()
            .filter(|&&c_id| c_id != client_id)
            .copied()
            .collect();
        let pane_focused_for_differet_client = !other_focused_clients.is_empty();

        let frame_color = self.frame_color(client_id, client_mode, session_is_mirrored);
        let focused_client = if pane_focused_for_client_id {
            Some(client_id)
        } else if pane_focused_for_differet_client {
            Some(*other_focused_clients.first().unwrap())
        } else {
            None
        };
        let frame_params = if session_is_mirrored {
            FrameParams {
                focused_client,
                is_main_client: pane_focused_for_client_id,
                other_focused_clients: vec![],
                style: self.style,
                color: frame_color,
                other_cursors_exist_in_session: false,
            }
        } else {
            FrameParams {
                focused_client,
                is_main_client: pane_focused_for_client_id,
                other_focused_clients,
                style: self.style,
                color: frame_color,
                other_cursors_exist_in_session: self.multiple_users_exist_in_session,
            }
        };
        if let Some((frame_terminal_characters, vte_output)) =
            self.pane.render_frame(client_id, frame_params, client_mode)
        {
            self.output.add_character_chunks_to_client(
                client_id,
                frame_terminal_characters,
                self.z_index,
            );
            if let Some(vte_output) = vte_output {
                self.output
                    .add_post_vte_instruction_to_client(client_id, &vte_output);
            }
        }
    }
    pub fn render_pane_boundaries(
        &self,
        client_id: ClientId,
        client_mode: InputMode,
        boundaries: &mut Boundaries,
        session_is_mirrored: bool,
    ) {
        let color = self.frame_color(client_id, client_mode, session_is_mirrored);
        boundaries.add_rect(self.pane.as_ref(), color);
    }
    fn frame_color(
        &self,
        client_id: ClientId,
        mode: InputMode,
        session_is_mirrored: bool,
    ) -> Option<PaletteColor> {
        let pane_focused_for_client_id = self.focused_clients.contains(&client_id);
        if pane_focused_for_client_id {
            match mode {
                InputMode::Normal | InputMode::Locked => {
                    if session_is_mirrored || !self.multiple_users_exist_in_session {
                        let colors = single_client_color(self.style.colors); // mirrored sessions only have one focused color
                        Some(colors.0)
                    } else {
                        let colors = client_id_to_colors(client_id, self.style.colors);
                        colors.map(|colors| colors.0)
                    }
                },
                _ => Some(self.style.colors.orange),
            }
        } else {
            None
        }
    }
}
