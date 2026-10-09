mod first_line;
mod hover;
mod one_line_ui;
mod second_line;
mod tip;

use ansi_term::{
    ANSIString,
    Colour::{Fixed, RGB},
    Style,
};

use std::collections::BTreeMap;
use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::{palette_match, style};

use first_line::first_line;
use one_line_ui::one_line_ui;
use second_line::{
    ascended_to_host_session_hint, descended_into_nested_session_hint, floating_panes_are_visible,
    fullscreen_panes_to_hide, keybinds, locked_floating_panes_are_visible,
    locked_fullscreen_panes_to_hide, system_clipboard_error, text_copied_hint,
};
use tip::utils::get_cached_tip_name;

use crate::click_actions::{region_at, ActionRunner, ClickRegion};
use crate::keybinds::KeybindStore;
use crate::ClientSeed;
use hover::with_hovered;

const RUNNER_OWNER: &str = "status-bar";

static ARROW_SEPARATOR: &str = "";
static MORE_MSG: &str = " ... ";
const TO_NORMAL: Action = Action::SwitchToMode {
    input_mode: InputMode::Normal,
};

#[derive(Default)]
struct SlotConfig {
    tip_name: String,
    classic_ui: bool,
}

#[derive(Default)]
struct ClientState {
    tabs: Vec<TabInfo>,
    mode_info: ModeInfo,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    base_mode_is_locked: bool,
    hovered: Option<Vec<Action>>,
    hover_at: Option<(SlotId, usize)>,
}

#[derive(Default)]
struct SlotClientState {
    clicks: Vec<ClickRegion>,
    cols: usize,
}

#[derive(Default)]
pub struct StatusBar {
    slots: BTreeMap<SlotId, SlotConfig>,
    clients: BTreeMap<ClientId, ClientState>,
    slot_clients: BTreeMap<(SlotId, ClientId), SlotClientState>,
    runner: ActionRunner,
}

#[derive(Default)]
pub struct LinePart {
    part: String,
    len: usize,
    clicks: Vec<ClickRegion>,
}

impl LinePart {
    pub fn append(&mut self, to_append: &LinePart) {
        let offset = self.len;
        self.clicks
            .extend(to_append.clicks.iter().map(|click| click.shifted(offset)));
        self.part.push_str(&to_append.part);
        self.len += to_append.len;
    }
    pub fn prepend_padding(&mut self, padding: &str, padding_len: usize) {
        self.part = format!("{}{}", padding, self.part);
        self.len += padding_len;
        for click in self.clicks.iter_mut() {
            *click = click.shifted(padding_len);
        }
    }
}

impl Display for LinePart {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.part)
    }
}

#[derive(Clone, Copy)]
pub struct ColoredElements {
    pub selected: SegmentStyle,
    pub unselected: SegmentStyle,
    pub unselected_alternate: SegmentStyle,
    pub disabled: SegmentStyle,
    pub superkey_prefix: Style,
    pub superkey_suffix_separator: Style,
}

#[derive(Clone, Copy)]
pub struct SegmentStyle {
    pub prefix_separator: Style,
    pub char_left_separator: Style,
    pub char_shortcut: Style,
    pub char_right_separator: Style,
    pub styled_text: Style,
    pub suffix_separator: Style,
}

fn color_elements(
    palette: Styling,
    different_color_alternates: bool,
    dimmed: bool,
) -> ColoredElements {
    if dimmed {
        let background = palette.text_unselected.background;
        let foreground = palette.text_unselected.base;
        let ribbon_background = palette.ribbon_unselected.background;
        let italic_on_ribbon = style!(palette.ribbon_unselected.base, ribbon_background).italic();
        let dim_segment = SegmentStyle {
            prefix_separator: style!(background, ribbon_background),
            char_left_separator: italic_on_ribbon,
            char_shortcut: italic_on_ribbon,
            char_right_separator: italic_on_ribbon,
            styled_text: italic_on_ribbon,
            suffix_separator: style!(ribbon_background, background),
        };
        return ColoredElements {
            selected: dim_segment,
            unselected: dim_segment,
            unselected_alternate: dim_segment,
            disabled: dim_segment,
            superkey_prefix: style!(foreground, background).italic(),
            superkey_suffix_separator: style!(background, background),
        };
    }
    let background = palette.text_unselected.background;
    let foreground = palette.text_unselected.base;
    let alternate_background_color = if different_color_alternates {
        palette.ribbon_unselected.base
    } else {
        palette.ribbon_unselected.background
    };
    ColoredElements {
        selected: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_selected.background),
            char_left_separator: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            char_shortcut: style!(
                palette.ribbon_selected.emphasis_0,
                palette.ribbon_selected.background
            )
            .bold(),
            char_right_separator: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            styled_text: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            suffix_separator: style!(palette.ribbon_selected.background, background).bold(),
        },
        unselected: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_unselected.background),
            char_left_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            char_shortcut: style!(
                palette.ribbon_unselected.emphasis_0,
                palette.ribbon_unselected.background
            )
            .bold(),
            char_right_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            styled_text: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            suffix_separator: style!(palette.ribbon_unselected.background, background).bold(),
        },
        unselected_alternate: SegmentStyle {
            prefix_separator: style!(background, alternate_background_color),
            char_left_separator: style!(background, alternate_background_color).bold(),
            char_shortcut: style!(
                palette.ribbon_unselected.emphasis_0,
                alternate_background_color
            )
            .bold(),
            char_right_separator: style!(background, alternate_background_color).bold(),
            styled_text: style!(palette.ribbon_unselected.base, alternate_background_color).bold(),
            suffix_separator: style!(alternate_background_color, background).bold(),
        },
        disabled: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_unselected.background),
            char_left_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            char_shortcut: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            char_right_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            styled_text: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            suffix_separator: style!(palette.ribbon_unselected.background, background),
        },
        superkey_prefix: style!(foreground, background).bold(),
        superkey_suffix_separator: style!(background, background),
    }
}

impl StatusBar {
    pub fn has_slots(&self) -> bool {
        !self.slots.is_empty()
    }

    pub fn slot_added(&mut self, slot: &Slot) {
        let tip_name = get_cached_tip_name();
        let classic_ui = slot
            .configuration
            .get("classic")
            .map(|c| c == "true")
            .unwrap_or(false);
        self.slots.insert(
            slot.id,
            SlotConfig {
                tip_name,
                classic_ui,
            },
        );
        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::CopyToClipboard,
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
            EventType::InitialKeybinds,
            EventType::Mouse,
            EventType::ActionComplete,
        ]);
    }

    pub fn slot_removed(&mut self, slot_id: SlotId) {
        self.slots.remove(&slot_id);
        self.slot_clients.retain(|(s, _), _| *s != slot_id);
    }

    pub fn client_connected(&mut self, client_id: ClientId) {
        self.clients.entry(client_id).or_default();
    }

    pub fn client_disconnected(&mut self, client_id: ClientId) {
        self.clients.remove(&client_id);
        self.slot_clients.retain(|(_, c), _| *c != client_id);
    }

    pub fn client_ids(&self) -> impl Iterator<Item = ClientId> + '_ {
        self.clients.keys().copied()
    }

    pub fn reset(&mut self) {
        self.clients.clear();
        self.slot_clients.clear();
    }

    pub fn snapshot(&self) -> BTreeMap<ClientId, ClientSeed> {
        self.clients
            .iter()
            .map(|(client_id, client)| {
                (
                    *client_id,
                    ClientSeed {
                        mode_info: client.mode_info.clone(),
                        tabs: client.tabs.clone(),
                    },
                )
            })
            .collect()
    }

    pub fn seed(&mut self, seeds: &BTreeMap<ClientId, ClientSeed>) {
        for (client_id, seed) in seeds {
            let client = self.clients.entry(*client_id).or_default();
            client.mode_info = seed.mode_info.clone();
            client.base_mode_is_locked = client.mode_info.base_mode == Some(InputMode::Locked);
            client.tabs = seed.tabs.clone();
        }
    }

    pub fn update(&mut self, event: &Event, context: EventContext) -> RenderResponse {
        let target_clients: Vec<ClientId> = match context.client_id {
            Some(client_id) => vec![client_id],
            None => self.clients.keys().copied().collect(),
        };
        let mut should_render = false;
        for client_id in target_clients {
            if self.update_client(event, client_id, context.slot_id) {
                should_render = true;
            }
        }
        if !should_render {
            RenderResponse::Nothing
        } else if let Some(client_id) = context.client_id {
            RenderResponse::Client(client_id)
        } else {
            RenderResponse::Slots(self.slots.keys().copied().collect())
        }
    }

    fn update_client(
        &mut self,
        event: &Event,
        client_id: ClientId,
        slot_id: Option<SlotId>,
    ) -> bool {
        let client = self.clients.entry(client_id).or_default();
        let empty_slot_client = SlotClientState::default();
        let slot_client = slot_id
            .and_then(|slot_id| self.slot_clients.get(&(slot_id, client_id)))
            .unwrap_or(&empty_slot_client);
        let mut should_render = false;
        match event {
            Event::InitialKeybinds(_) => {
                should_render = true;
            },
            Event::ModeUpdate(mode_info) => {
                if &client.mode_info != mode_info {
                    should_render = true;
                }
                client.mode_info = mode_info.clone();
                client.base_mode_is_locked = client.mode_info.base_mode == Some(InputMode::Locked);
            },
            Event::TabUpdate(tabs) => {
                if &client.tabs != tabs {
                    should_render = true;
                }
                client.tabs = tabs.clone();
            },
            Event::CopyToClipboard(copy_destination) => {
                let copy_destination = *copy_destination;
                match client.text_copy_destination {
                    Some(text_copy_destination) => {
                        if text_copy_destination != copy_destination {
                            should_render = true;
                        }
                    },
                    None => {
                        should_render = true;
                    },
                }
                client.text_copy_destination = Some(copy_destination);
            },
            Event::SystemClipboardFailure => {
                should_render = true;
                client.display_system_clipboard_failure = true;
            },
            Event::InputReceived => {
                if client.text_copy_destination.is_some()
                    || client.display_system_clipboard_failure == true
                {
                    should_render = true;
                }
                client.text_copy_destination = None;
                client.display_system_clipboard_failure = false;
            },
            Event::ActionComplete(..) => {
                self.runner.action_completed(RUNNER_OWNER, client_id, event);
            },
            Event::Mouse(mouse_event) => match mouse_event {
                Mouse::RightClick(line, col) => {
                    open_context_menu(ContextMenuTarget::Bar, (*line).max(0) as usize, *col);
                },
                Mouse::LeftClick(_, col) => {
                    if let Some(region) = region_at(&slot_client.clicks, *col) {
                        self.runner
                            .run(RUNNER_OWNER, client_id, region.actions.clone());
                    }
                },
                Mouse::Hover(_, col) => {
                    if let Some(slot_id) = slot_id {
                        let col = *col;
                        let on_screen = col < slot_client.cols;
                        let owns_hover = client
                            .hover_at
                            .map(|(hover_slot, _)| hover_slot == slot_id)
                            .unwrap_or(true);
                        if on_screen || owns_hover {
                            client.hover_at = if on_screen {
                                Some((slot_id, col))
                            } else {
                                None
                            };
                            let hovered = region_at(&slot_client.clicks, col)
                                .filter(|_| on_screen)
                                .map(|region| region.actions.clone());
                            if client.hovered != hovered {
                                client.hovered = hovered;
                                should_render = true;
                            }
                        }
                    }
                },
                _ => {},
            },
            _ => {},
        };
        should_render
    }

    pub fn render(
        &mut self,
        rows: usize,
        cols: usize,
        slot_id: SlotId,
        client_id: ClientId,
        keybinds: &mut KeybindStore,
    ) {
        let lent = self.swap_keybinds(client_id, keybinds);
        self.render_client(rows, cols, slot_id, client_id);
        if lent {
            self.swap_keybinds(client_id, keybinds);
        }
    }

    fn swap_keybinds(&mut self, client_id: ClientId, keybinds: &mut KeybindStore) -> bool {
        match self.clients.get_mut(&client_id) {
            Some(client) => keybinds.swap(client_id, &mut client.mode_info.keybinds),
            None => false,
        }
    }

    fn render_client(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {
        let Some(client) = self.clients.get(&client_id) else {
            return;
        };
        let Some(slot) = self.slots.get(&slot_id) else {
            return;
        };
        let supports_arrow_fonts = !client.mode_info.capabilities.arrow_fonts;
        let separator = if supports_arrow_fonts {
            ARROW_SEPARATOR
        } else {
            ""
        };

        let background = client.mode_info.style.colors.text_unselected.background;

        if rows == 1 && !slot.classic_ui {
            let fill_bg = match background {
                PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m\u{1b}[0K", r, g, b),
                PaletteColor::EightBit(color) => format!("\u{1b}[48;5;{}m\u{1b}[0K", color),
            };
            let active_tab = client.tabs.iter().find(|t| t.active);
            let render_line = |hovered: Option<Vec<Action>>| {
                with_hovered(hovered, || {
                    one_line_ui(
                        &client.mode_info,
                        active_tab,
                        cols,
                        separator,
                        client.base_mode_is_locked,
                        client.text_copy_destination,
                        client.display_system_clipboard_failure,
                    )
                })
            };
            let hover_col = client
                .hover_at
                .filter(|(hover_slot, _)| *hover_slot == slot_id)
                .map(|(_, col)| col);
            let mut hovered = hover_col.and(client.hovered.clone());
            let mut line = render_line(hovered.clone());
            if let Some(col) = hover_col {
                let under_mouse = region_at(&line.clicks, col).map(|region| region.actions.clone());
                if under_mouse != hovered {
                    hovered = under_mouse;
                    line = render_line(hovered.clone());
                }
            }
            if hover_col.is_some() {
                if let Some(client) = self.clients.get_mut(&client_id) {
                    client.hovered = hovered;
                }
            }
            let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
            slot_client.clicks = std::mem::take(&mut line.clicks);
            slot_client.cols = cols;
            print!("{}{}", line, fill_bg);
            return;
        }
        let slot_client = self.slot_clients.entry((slot_id, client_id)).or_default();
        slot_client.clicks.clear();
        slot_client.cols = cols;

        let active_tab = client.tabs.iter().find(|t| t.active);
        let first_line = first_line(&client.mode_info, active_tab, cols, separator);
        let second_line = second_line(client, &slot.tip_name, cols);
        let show_nested_session_hint = client.mode_info.session_dimmed.unwrap_or(false)
            || client.mode_info.session_ascended.unwrap_or(false);

        match background {
            PaletteColor::Rgb((r, g, b)) => {
                if rows > 1 {
                    println!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
                } else {
                    if show_nested_session_hint {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    } else if client.mode_info.mode == InputMode::Normal {
                        print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
                    } else {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    }
                }
            },
            PaletteColor::EightBit(color) => {
                if rows > 1 {
                    println!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
                } else {
                    if show_nested_session_hint {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    } else if client.mode_info.mode == InputMode::Normal {
                        print!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
                    } else {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    }
                }
            },
        }

        if rows > 1 {
            print!("\u{1b}[m{}\u{1b}[0K", second_line);
        }
    }
}

fn second_line(client: &ClientState, tip_name: &str, cols: usize) -> LinePart {
    let active_tab = client.tabs.iter().find(|t| t.active);

    if client.mode_info.session_dimmed.unwrap_or(false) {
        return descended_into_nested_session_hint(&client.mode_info, cols);
    }
    if client.mode_info.session_ascended.unwrap_or(false) {
        return ascended_to_host_session_hint(&client.mode_info, cols);
    }
    if let Some(copy_destination) = client.text_copy_destination {
        text_copied_hint(copy_destination)
    } else if client.display_system_clipboard_failure {
        system_clipboard_error(&client.mode_info.style.colors)
    } else if let Some(active_tab) = active_tab {
        if active_tab.is_fullscreen_active {
            match client.mode_info.mode {
                InputMode::Normal => fullscreen_panes_to_hide(
                    &client.mode_info.style.colors,
                    active_tab.panes_to_hide,
                ),
                InputMode::Locked => locked_fullscreen_panes_to_hide(
                    &client.mode_info.style.colors,
                    active_tab.panes_to_hide,
                ),
                _ => keybinds(&client.mode_info, tip_name, cols),
            }
        } else if active_tab.are_floating_panes_visible {
            match client.mode_info.mode {
                InputMode::Normal => floating_panes_are_visible(&client.mode_info),
                InputMode::Locked => {
                    locked_floating_panes_are_visible(&client.mode_info.style.colors)
                },
                _ => keybinds(&client.mode_info, tip_name, cols),
            }
        } else {
            keybinds(&client.mode_info, tip_name, cols)
        }
    } else {
        LinePart::default()
    }
}

pub fn get_common_modifiers(mut keyvec: Vec<&KeyWithModifier>) -> Vec<KeyModifier> {
    if keyvec.is_empty() {
        return vec![];
    }
    let mut common_modifiers = keyvec.pop().unwrap().key_modifiers.clone();
    for key in keyvec {
        common_modifiers = common_modifiers
            .intersection(&key.key_modifiers)
            .cloned()
            .collect();
    }
    common_modifiers.into_iter().collect()
}

pub fn action_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &[Action],
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter_map(|(key, acvec)| {
            let matching = acvec
                .iter()
                .zip(action)
                .filter(|(a, b)| a.shallow_eq(b))
                .count();

            if matching == acvec.len() && matching == action.len() {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect::<Vec<KeyWithModifier>>()
}

pub fn action_key_group(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    actions: &[&[Action]],
) -> Vec<KeyWithModifier> {
    let mut ret = vec![];
    for action in actions {
        ret.extend(action_key(keymap, action));
    }
    ret
}

pub fn style_key_with_modifier(
    keyvec: &[KeyWithModifier],
    palette: &Styling,
    background: Option<PaletteColor>,
) -> Vec<ANSIString<'static>> {
    if keyvec.is_empty() {
        return vec![];
    }

    let text_color = palette_match!(palette.text_unselected.base);
    let green_color = palette_match!(palette.text_unselected.emphasis_2);
    let orange_color = palette_match!(palette.text_unselected.emphasis_0);
    let mut ret = vec![];

    let common_modifiers = get_common_modifiers(keyvec.iter().collect());

    let no_common_modifier = common_modifiers.is_empty();
    let modifier_str = common_modifiers
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let painted_modifier = if modifier_str.is_empty() {
        Style::new().paint("")
    } else {
        if let Some(background) = background {
            let background = palette_match!(background);
            Style::new()
                .fg(orange_color)
                .on(background)
                .bold()
                .paint(modifier_str)
        } else {
            Style::new().fg(orange_color).bold().paint(modifier_str)
        }
    };
    ret.push(painted_modifier);

    let group_start_str = if no_common_modifier { "<" } else { " + <" };
    if let Some(background) = background {
        let background = palette_match!(background);
        ret.push(
            Style::new()
                .fg(text_color)
                .on(background)
                .paint(group_start_str),
        );
    } else {
        ret.push(Style::new().fg(text_color).paint(group_start_str));
    }

    let key = keyvec
        .iter()
        .map(|key| {
            if no_common_modifier {
                format!("{}", key)
            } else {
                let key_modifier_for_key = key
                    .key_modifiers
                    .iter()
                    .filter(|m| !common_modifiers.contains(m))
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if key_modifier_for_key.is_empty() {
                    format!("{}", key.bare_key)
                } else {
                    format!("{} {}", key_modifier_for_key, key.bare_key)
                }
            }
        })
        .collect::<Vec<String>>();

    let key_string = key.join("");
    let key_separator = match &key_string[..] {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        _ => "|",
    };

    for (idx, key) in key.iter().enumerate() {
        if idx > 0 && !key_separator.is_empty() {
            if let Some(background) = background {
                let background = palette_match!(background);
                ret.push(
                    Style::new()
                        .fg(text_color)
                        .on(background)
                        .paint(key_separator),
                );
            } else {
                ret.push(Style::new().fg(text_color).paint(key_separator));
            }
        }
        if let Some(background) = background {
            let background = palette_match!(background);
            ret.push(
                Style::new()
                    .fg(green_color)
                    .on(background)
                    .bold()
                    .paint(key.clone()),
            );
        } else {
            ret.push(Style::new().fg(green_color).bold().paint(key.clone()));
        }
    }

    let group_end_str = ">";
    if let Some(background) = background {
        let background = palette_match!(background);
        ret.push(
            Style::new()
                .fg(text_color)
                .on(background)
                .paint(group_end_str),
        );
    } else {
        ret.push(Style::new().fg(text_color).paint(group_end_str));
    }

    ret
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ansi_term::unstyle;
    use ansi_term::ANSIStrings;

    fn big_keymap() -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (KeyWithModifier::new(BareKey::Char('a')), vec![Action::Quit]),
            (
                KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
                vec![Action::ScrollUp],
            ),
            (
                KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
                vec![Action::ScrollDown],
            ),
            (
                KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier(),
                vec![
                    Action::ScrollDown,
                    Action::SwitchToMode {
                        input_mode: InputMode::Normal,
                    },
                ],
            ),
            (
                KeyWithModifier::new(BareKey::Char('1')),
                vec![
                    TO_NORMAL,
                    Action::SwitchToMode {
                        input_mode: InputMode::Locked,
                    },
                ],
            ),
        ]
    }

    #[test]
    fn common_modifier_with_ctrl_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![KeyModifier::Ctrl]);
    }

    #[test]
    fn common_modifier_with_alt_keys_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![KeyModifier::Alt]);
    }

    #[test]
    fn common_modifier_with_mixed_alt_ctrl_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![]);
    }

    #[test]
    fn common_modifier_with_any_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![]);
    }

    #[test]
    fn action_key_simple_pattern_match_exact() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[Action::Quit]);
        assert_eq!(ret, vec![KeyWithModifier::new(BareKey::Char('f'))]);
    }

    #[test]
    fn action_key_simple_pattern_match_pattern_too_long() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[Action::Quit, Action::ScrollUp]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_simple_pattern_match_pattern_empty() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_long_pattern_match_exact() {
        let keymap = big_keymap();
        let ret = action_key(&keymap, &[Action::ScrollDown, TO_NORMAL]);
        assert_eq!(
            ret,
            vec![KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier()]
        );
    }

    #[test]
    fn action_key_long_pattern_match_too_short() {
        let keymap = big_keymap();
        let ret = action_key(&keymap, &[TO_NORMAL]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_group_single_pattern() {
        let keymap = big_keymap();
        let ret = action_key_group(&keymap, &[&[Action::Quit]]);
        assert_eq!(ret, vec![KeyWithModifier::new(BareKey::Char('a'))]);
    }

    #[test]
    fn action_key_group_two_patterns() {
        let keymap = big_keymap();
        let ret = action_key_group(&keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
        assert_eq!(
            ret,
            vec![
                KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
                KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier()
            ]
        );
    }

    #[test]
    fn style_key_with_modifier_only_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')),
            KeyWithModifier::new(BareKey::Char('b')),
            KeyWithModifier::new(BareKey::Char('c')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<a|b|c>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_hjkl() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('h')),
            KeyWithModifier::new(BareKey::Char('j')),
            KeyWithModifier::new(BareKey::Char('k')),
            KeyWithModifier::new(BareKey::Char('l')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<hjkl>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_all_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left),
            KeyWithModifier::new(BareKey::Down),
            KeyWithModifier::new(BareKey::Up),
            KeyWithModifier::new(BareKey::Right),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<←↓↑→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_left_right_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left),
            KeyWithModifier::new(BareKey::Right),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<←→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_down_up_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Down),
            KeyWithModifier::new(BareKey::Up),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<↓↑>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_ctrl_modifier_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Ctrl + <a|b|c|d>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_alt_modifier_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('d')).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <a|b|c|d>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_alt_modifier_with_special_group_all_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Down).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Up).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Right).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <←↓↑→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_ctrl_alt_char_mixed() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<Alt a|Ctrl b|c>".to_string())
    }

    #[test]
    fn style_key_with_modifier_unprintables() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Backspace),
            KeyWithModifier::new(BareKey::Enter),
            KeyWithModifier::new(BareKey::Char(' ')),
            KeyWithModifier::new(BareKey::Tab),
            KeyWithModifier::new(BareKey::PageDown),
            KeyWithModifier::new(BareKey::Delete),
            KeyWithModifier::new(BareKey::Home),
            KeyWithModifier::new(BareKey::End),
            KeyWithModifier::new(BareKey::Insert),
            KeyWithModifier::new(BareKey::Tab),
            KeyWithModifier::new(BareKey::Esc),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(
            ret,
            "<BACKSPACE|ENTER|SPACE|TAB|PgDn|DEL|HOME|END|INS|TAB|ESC>".to_string()
        )
    }

    #[test]
    fn style_key_with_modifier_unprintables_with_common_ctrl_modifier() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Enter).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char(' ')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Tab).with_ctrl_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Ctrl + <ENTER|SPACE|TAB>".to_string())
    }

    #[test]
    fn style_key_with_modifier_unprintables_with_common_alt_modifier() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Enter).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char(' ')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Tab).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <ENTER|SPACE|TAB>".to_string())
    }
}

#[cfg(test)]
mod hover_tests {
    use super::*;

    fn ctrl(c: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(c)).with_ctrl_modifier()
    }

    fn to_mode(input_mode: InputMode) -> Vec<Action> {
        vec![Action::SwitchToMode { input_mode }]
    }

    fn mode_info(mode: InputMode) -> ModeInfo {
        ModeInfo {
            mode,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![
                (
                    InputMode::Normal,
                    vec![
                        (ctrl('g'), to_mode(InputMode::Locked)),
                        (ctrl('p'), to_mode(InputMode::Pane)),
                        (ctrl('q'), vec![Action::Quit]),
                    ],
                ),
                (
                    InputMode::Pane,
                    vec![(ctrl('p'), to_mode(InputMode::Normal))],
                ),
                (
                    InputMode::Locked,
                    vec![(ctrl('g'), to_mode(InputMode::Normal))],
                ),
            ],
            ..Default::default()
        }
    }

    fn status_bar(mode: InputMode) -> StatusBar {
        let mut bar = StatusBar::default();
        bar.slots.insert(0, SlotConfig::default());
        bar.clients.insert(
            1,
            ClientState {
                mode_info: mode_info(mode),
                ..Default::default()
            },
        );
        bar
    }

    fn region_start(bar: &StatusBar, actions: &[Action]) -> usize {
        bar.slot_clients[&(0, 1)]
            .clicks
            .iter()
            .find(|region| region.actions.as_slice() == actions)
            .map(|region| region.start)
            .unwrap_or_else(|| panic!("{:?} is not clickable", actions))
    }

    #[test]
    fn a_resting_mouse_keeps_its_hover_when_the_bar_changes_under_it() {
        let mut bar = status_bar(InputMode::Normal);
        bar.render_client(1, 200, 0, 1);
        let lock_col = region_start(&bar, &to_mode(InputMode::Locked));
        assert!(bar.update_client(&Event::Mouse(Mouse::Hover(0, lock_col)), 1, Some(0)));
        assert_eq!(bar.clients[&1].hovered, Some(to_mode(InputMode::Locked)));

        bar.update_client(&Event::Mouse(Mouse::LeftClick(0, lock_col)), 1, Some(0));
        bar.update_client(&Event::ModeUpdate(mode_info(InputMode::Locked)), 1, Some(0));
        bar.render_client(1, 200, 0, 1);
        assert_eq!(region_start(&bar, &to_mode(InputMode::Normal)), lock_col);
        assert_eq!(bar.clients[&1].hovered, Some(to_mode(InputMode::Normal)));
    }

    #[test]
    fn leaving_another_slot_does_not_clear_the_hover() {
        let mut bar = status_bar(InputMode::Normal);
        bar.slots.insert(7, SlotConfig::default());
        bar.render_client(1, 200, 0, 1);
        bar.render_client(1, 200, 7, 1);
        let pane_col = region_start(&bar, &to_mode(InputMode::Pane));
        bar.update_client(&Event::Mouse(Mouse::Hover(0, pane_col)), 1, Some(0));
        assert!(!bar.update_client(&Event::Mouse(Mouse::Hover(0, 65535)), 1, Some(7)));
        assert_eq!(bar.clients[&1].hovered, Some(to_mode(InputMode::Pane)));
        assert!(bar.update_client(&Event::Mouse(Mouse::Hover(0, 65535)), 1, Some(0)));
        assert_eq!(bar.clients[&1].hovered, None);
        assert_eq!(bar.clients[&1].hover_at, None);
    }
}
