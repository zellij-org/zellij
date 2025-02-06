// mod presets;
// mod presets_screen;
// mod rebind_leaders_screen;
// mod ui_components;

use zellij_tile::prelude::*;

// use presets_screen::PresetsScreen;
// use rebind_leaders_screen::RebindLeadersScreen;
// use ui_components::top_tab_menu;

use std::collections::BTreeMap;

pub static UI_SIZE: usize = 15;
pub static WIDTH_BREAKPOINTS: (usize, usize) = (62, 35);
pub static POSSIBLE_MODIFIERS: [KeyModifier; 4] = [
    KeyModifier::Ctrl,
    KeyModifier::Alt,
    KeyModifier::Super,
    KeyModifier::Shift,
];

#[derive(Debug)]
pub struct MainScreen {
    version: String,
    link_coordinates: Vec<LinkCoordinates>,
    item_coordinates: Vec<ItemCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
    pub selected_item_index: Option<usize>,
    pub selected_hover_index: Option<usize>,
}

impl Default for MainScreen {
    fn default() -> Self {
        MainScreen {
            version: "0.42.0".to_owned(),
            link_coordinates: vec![],
            item_coordinates: vec![],
            hover_coordinates: None,
            selected_item_index: None,
            selected_hover_index: None,
        }
    }
}

impl MainScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        self.link_coordinates.clear();
        self.item_coordinates.clear();
        let ui_width = 74; // length of please support line
        let ui_height = 12;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Hi there, welcome to Zellij {}!", self.version);
        let title = Text::new(title_text).color_range(2, 21..=27 + self.version.chars().count());
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let whats_new_text = format!("What's new?");
        let whats_new = Text::new(whats_new_text);
        print_text_with_coordinates(whats_new, base_x, base_y + 2, None, None);

        let stacked_resize_text = format!("1. Stacked resize");
        let mut stacked_resize = Text::new(stacked_resize_text).color_range(0, 3..);
        if self.selected_item_index() == Some(0) {
            stacked_resize = stacked_resize.selected();
        }
        self.item_coordinates.push(ItemCoordinates::new(base_x, base_y + 3, ui_width, 0));
        print_text_with_coordinates(stacked_resize, base_x, base_y + 3, Some(ui_width), None);

        let pinned_floating_panes_text = format!("2. Pinned floating panes");
        let mut pinned_floating_panes = Text::new(pinned_floating_panes_text).color_range(0, 3..);
        if self.selected_item_index() == Some(1) {
            pinned_floating_panes = pinned_floating_panes.selected();
        }
        self.item_coordinates.push(ItemCoordinates::new(base_x, base_y + 4, ui_width, 1));
        print_text_with_coordinates(pinned_floating_panes, base_x, base_y + 4, Some(ui_width), None);

        let new_theme_def_text = format!("3. New theme definition spec");
        let mut new_theme_def = Text::new(new_theme_def_text).color_range(0, 3..);
        if self.selected_item_index() == Some(2) {
            new_theme_def = new_theme_def.selected();
        }
        self.item_coordinates.push(ItemCoordinates::new(base_x, base_y + 5, ui_width, 2));
        print_text_with_coordinates(new_theme_def, base_x, base_y + 5, Some(ui_width), None);

        let new_plugin_apis_text = format!("4. New plugin APIs");
        let mut new_plugin_apis = Text::new(new_plugin_apis_text).color_range(0, 3..);
        if self.selected_item_index() == Some(3) {
            new_plugin_apis = new_plugin_apis.selected();
        }
        self.item_coordinates.push(ItemCoordinates::new(base_x, base_y + 6, ui_width, 3));
        print_text_with_coordinates(new_plugin_apis, base_x, base_y + 6, Some(ui_width), None);

        let mouse_anyevent_text = format!("5. Mouse AnyEvent Handling");
        let mut mouse_anyevent = Text::new(mouse_anyevent_text).color_range(0, 3..);
        if self.selected_item_index() == Some(4) {
            mouse_anyevent = mouse_anyevent.selected();
        }
        self.item_coordinates.push(ItemCoordinates::new(base_x, base_y + 7, ui_width, 4));
        print_text_with_coordinates(mouse_anyevent, base_x, base_y + 7, Some(ui_width), None);

        self.render_changelog_link(base_x, base_y + 9);
        self.render_sponsor_link(base_x, base_y + 11);
        self.render_help(rows);
    }
    pub fn hover_item(&self) -> Option<usize> {
        self.selected_hover_index
    }
    fn selected_item_index(&self) -> Option<usize> {
        self.selected_hover_index.or(self.selected_item_index)
    }
    fn render_sponsor_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 40, y, 34, "https://github.com/sponsors/imsnif");
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                let support_text = format!("Please support the Zellij developer <3:");
                self.link_coordinates.push(link_coordinates);
                let support = Text::new(support_text).color_range(3, 0..=38);
                print_text_with_coordinates(support, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif", y + 1, x + 41);
                return;
            }
        }
        let support_text = format!("Please support the Zellij developer <3: https://github.com/sponsors/imsnif");
        self.link_coordinates.push(link_coordinates);
        let support = Text::new(support_text).color_range(3, 0..=38);
        print_text_with_coordinates(support, x, y, None, None);
    }
    fn render_changelog_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 16, y, 51 + self.version.chars().count(), &format!("https://github.com/zellij-org/zellij/releases/tag/v{}", self.version));
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let full_changelog_text = format!("Full Changelog:");
                let full_changelog = Text::new(full_changelog_text);
                print_text_with_coordinates(full_changelog, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}", y + 1, x + 17, self.version);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let full_changelog_text = format!("Full Changelog: https://github.com/zellij-org/zellij/releases/tag/v{}", self.version);
        let full_changelog = Text::new(full_changelog_text);
        print_text_with_coordinates(full_changelog, x, y, None, None);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else if self.selected_item_index.is_some() {
            let help_text = format!("Help: <↓↑> - Navigate, <ENTER> - Learn More, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=29)
                .color_range(1, 45..=49);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <↓↑> - Navigate, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=27);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if key.bare_key == BareKey::Down && key.has_no_modifiers() {
            self.move_selection_down();
            should_render = true;
        } else if key.bare_key == BareKey::Up && key.has_no_modifiers() {
            self.move_selection_up();
            should_render = true;
        }
        should_render
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        run_command(
                            &["xdg-open", &link_coordinates.destination_url], // TODO: use open on
                                                                              // macos
                             Default::default()
                        );
                        break;
                    }
                }
            }
            Mouse::Hover(line, col) => {
                let mut contained = false;
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        self.hover_coordinates = Some((line, col));
                        should_render = true;
                        contained = true;
                        break;
                    }
                }
                for item_coordinates in &self.item_coordinates {
                    if item_coordinates.contains(line, col) {
                        let prev_index = self.selected_hover_index;
                        // self.hover_coordinates = Some((line, col));
                        self.selected_hover_index = Some(item_coordinates.index);
                        self.selected_item_index = None;
                        contained = true;
                        if self.selected_hover_index != prev_index {
                            should_render = true;
                        }
                    }
                }
                if !contained {
                    if self.hover_coordinates.is_some() || self.selected_hover_index.is_some() {
                        // so that we clear the hover indication
                        should_render = true;
                    }
                    self.hover_coordinates = None;
                    self.selected_hover_index = None;
                }
            }
            _ => {}
        }
        should_render
    }
    fn move_selection_down(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(0);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 4 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index + 1);
            }
        }
    }
    fn move_selection_up(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(4);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 0 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index.saturating_sub(1));
            }
        }
    }
}

#[derive(Debug, Default)]
struct StackedResizeScreen {
    link_coordinates: Vec<LinkCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
}

impl StackedResizeScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        let ui_width = 80; // length of please support line
        let ui_height = 15;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Stacked Resize");
        let title = Text::new(title_text).color_range(0, ..);
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let explanation_1_text = format!("This version includes a new resizing algorithm that helps better manage panes");
        let explanation_1 = Text::new(explanation_1_text);
        print_text_with_coordinates(explanation_1, base_x, base_y + 2, None, None);

        let explanation_2_text = format!("into stacks.");
        let explanation_2 = Text::new(explanation_2_text);
        print_text_with_coordinates(explanation_2, base_x, base_y + 3, None, None);

        let try_it_out_text = format!("To try it out:");
        let try_it_out = Text::new(try_it_out_text).color_range(2 , ..);
        print_text_with_coordinates(try_it_out, base_x, base_y + 5, None, None);

        let bulletin_1_text = format!("1. Hide this pane with Alt f (you can bring it back with Alt f again)");
        let bulletin_1 = Text::new(bulletin_1_text).color_range(3, 23..=27).color_range(3, 57..=61);
        print_text_with_coordinates(bulletin_1, base_x, base_y + 6, Some(ui_width), None);

        let bulletin_2_text = format!("2. Open 4-5 panes with Alt n");
        let bulletin_2 = Text::new(bulletin_2_text).color_range(3, 23..=27);
        print_text_with_coordinates(bulletin_2, base_x, base_y + 7, Some(ui_width), None);

        let bulletin_3_text = format!("3. Press Alt + until you reach full screen");
        let bulletin_3 = Text::new(bulletin_3_text).color_range(3, 9..=13);
        print_text_with_coordinates(bulletin_3, base_x, base_y + 8, Some(ui_width), None);

        let bulletin_4_text = format!("4. Press Alt - until you are back at the original state");
        let bulletin_4 = Text::new(bulletin_4_text).color_range(3, 9..=13);
        print_text_with_coordinates(bulletin_4, base_x, base_y + 9, Some(ui_width), None);

        let bulletin_5_text = format!("5. You can always snap back to the built-in swap layouts with Alt <[]>");
        let bulletin_5 = Text::new(bulletin_5_text).color_range(3, 62..=64).color_range(3, 67..=68);
        print_text_with_coordinates(bulletin_5, base_x, base_y + 10, Some(ui_width), None);

        let to_disable_text = format!("To disable, add stacked_resize false to the Zellij Configuration");
        let to_disable = Text::new(to_disable_text).color_range(3, 16..=35);
        print_text_with_coordinates(to_disable, base_x, base_y + 12, Some(ui_width), None);

        self.render_more_details_link(base_x, base_y + 14);
        self.render_help(rows);
    }
    fn render_more_details_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 23, y, 45, &format!("https://zellij.dev/screencasts/stacked-resize")); // TODO: proper link
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let more_details_text = format!("For more details, see:");
                let more_details = Text::new(more_details_text).color_range(2, ..=21);
                print_text_with_coordinates(more_details, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/screencasts/stacked-resize", y + 1, x + 24);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let more_details_text = format!("For more details, see: https://zellij.dev/screencasts/stacked-resize"); // TODO: proper link
        let more_details = Text::new(more_details_text).color_range(2, ..=21);
        print_text_with_coordinates(more_details, x, y, None, None);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <ESC> - go back to main screen");
            let help = Text::new(help_text)
                .color_range(1, 6..=10);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        run_command(
                            &["xdg-open", &link_coordinates.destination_url], // TODO: use open on
                                                                              // macos
                             Default::default()
                        );
                        break;
                    }
                }
            }
            Mouse::Hover(line, col) => {
                let mut contained_in_link = false;
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        self.hover_coordinates = Some((line, col));
                        should_render = true;
                        contained_in_link = true;
                        break;
                    }
                }
                if !contained_in_link {
                    if self.hover_coordinates.is_some() {
                        // so that we clear the hover indication
                        should_render = true;
                    }
                    self.hover_coordinates = None;
                }
            }
            _ => {}
        }
        should_render
    }
}

#[derive(Debug, Default)]
struct PinnedFloatingPanesScreen {
    link_coordinates: Vec<LinkCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
}

impl PinnedFloatingPanesScreen {
    pub fn render(&mut self, rows: usize, cols: usize, base_mode: InputMode) {
        let ui_width = 67; // length of first explanation line
        let ui_height = 11;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Pinned Floating Panes");
        let title = Text::new(title_text).color_range(0, ..);
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let explanation_1_text = format!("This version adds the ability to \"pin\" a floating pane so that it");
        let explanation_1 = Text::new(explanation_1_text);
        print_text_with_coordinates(explanation_1, base_x, base_y + 2, None, None);

        let explanation_2_text = format!("will always be visible even if floating panes are hidden.");
        let explanation_2 = Text::new(explanation_2_text);
        print_text_with_coordinates(explanation_2, base_x, base_y + 3, None, None);

        let try_it_out_text = format!("Floating panes can be \"pinned\": ");
        let try_it_out = Text::new(try_it_out_text).color_range(2 , ..);
        print_text_with_coordinates(try_it_out, base_x, base_y + 5, None, None);

        let bulletin_1_text = format!("1. With a mouse click on their top right corner");
        let bulletin_1 = Text::new(bulletin_1_text).color_range(3, 10..=20);
        print_text_with_coordinates(bulletin_1, base_x, base_y + 6, Some(ui_width), None);

        match base_mode {
            InputMode::Locked => {
                let bulletin_2_text = format!("2. With Ctrl g + p + i");
                let bulletin_2 = Text::new(bulletin_2_text).color_range(3, 8..=13).color_range(3, 17..18).color_range(3, 21..22);
                print_text_with_coordinates(bulletin_2, base_x, base_y + 7, Some(ui_width), None);
            },
            _ => {
                let bulletin_2_text = format!("2. With Ctrl p + i");
                let bulletin_2 = Text::new(bulletin_2_text).color_range(3, 8..=13).color_range(3, 17..18);
                print_text_with_coordinates(bulletin_2, base_x, base_y + 7, Some(ui_width), None);
            }
        }

        let use_case_1_text = format!("A great use case for these is to tail log files or to show");
        let use_case_1 = Text::new(use_case_1_text);
        print_text_with_coordinates(use_case_1, base_x, base_y + 9, Some(ui_width), None);

        let use_case_2_text = format!("real-time compiler output while working in other panes.");
        let use_case_2 = Text::new(use_case_2_text);
        print_text_with_coordinates(use_case_2, base_x, base_y + 10, Some(ui_width), None);

        self.render_help(rows);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <ESC> - go back to main screen");
            let help = Text::new(help_text)
                .color_range(1, 6..=10);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
}

#[derive(Debug, Default)]
struct NewThemeDefinitionSpecScreen {

}

impl NewThemeDefinitionSpecScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        println!("NewThemeDefinitionSpec (TBD)");
    }
}

#[derive(Debug, Default)]
struct NewPluginApisScreen{
    hover_coordinates: Option<(isize, usize)>, // line/col
}

impl NewPluginApisScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        let ui_width = 53; // length of first explanation line
        let ui_height = 10;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("New Plugin APIs");
        let title = Text::new(title_text).color_range(0, ..);
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let explanation_1_text = format!("New APIs were added in this version affording plugins");
        let explanation_1 = Text::new(explanation_1_text);
        print_text_with_coordinates(explanation_1, base_x, base_y + 2, None, None);

        let explanation_2_text = format!("finer control over the workspace.");
        let explanation_2 = Text::new(explanation_2_text);
        print_text_with_coordinates(explanation_2, base_x, base_y + 3, None, None);
        
        let try_it_out_text = format!("Some examples:");
        let try_it_out = Text::new(try_it_out_text).color_range(2 , ..);
        print_text_with_coordinates(try_it_out, base_x, base_y + 5, None, None);

        let bulletin_1_text = format!("1. Change floating panes' coordinates and size");
        let bulletin_1 = Text::new(bulletin_1_text).color_range(3, 26..=36).color_range(3, 42..=45);
        print_text_with_coordinates(bulletin_1, base_x, base_y + 6, Some(ui_width), None);

        let bulletin_2_text = format!("2. Stack arbitrary panes");
        let bulletin_2 = Text::new(bulletin_2_text).color_range(3, 3..=7);
        print_text_with_coordinates(bulletin_2, base_x, base_y + 7, Some(ui_width), None);

        let bulletin_3_text = format!("3. Change /host folder");
        let bulletin_3 = Text::new(bulletin_3_text).color_range(3, 10..=14);
        print_text_with_coordinates(bulletin_3, base_x, base_y + 8, Some(ui_width), None);

        let bulletin_4_text = format!("4. Discover the user's $SHELL and $EDITOR");
        let bulletin_4 = Text::new(bulletin_4_text).color_range(3, 23..=28).color_range(3, 34..=40);
        print_text_with_coordinates(bulletin_4, base_x, base_y + 9, Some(ui_width), None);
        self.render_help(rows);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <ESC> - go back to main screen");
            let help = Text::new(help_text)
                .color_range(1, 6..=10);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
}

#[derive(Debug, Default)]
struct MouseAnyEventHandlingScreen {
    hover_coordinates: Option<(isize, usize)>, // line/col
}

impl MouseAnyEventHandlingScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        // Mouse Any-Event Tracking
        //
        // This version adds the capability to track mouse motions more accurately
        // both in Zellij, in terminal panes and in plugin panes.
        //
        // Future versions will also build on this capability to improve the Zellij
        // UI.
        let ui_width = 75; // length of first explanation line
        let ui_height = 6;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Mouse Any-Event Tracking");
        let title = Text::new(title_text).color_range(0, ..);
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let explanation_1_text = format!("This version adds the capability to track mouse motions more accurately");
        let explanation_1 = Text::new(explanation_1_text);
        print_text_with_coordinates(explanation_1, base_x, base_y + 2, None, None);

        let explanation_2_text = format!("both in Zellij, in terminal panes and in plugin panes.");
        let explanation_2 = Text::new(explanation_2_text);
        print_text_with_coordinates(explanation_2, base_x, base_y + 3, None, None);
        
        let explanation_3_text = format!("Future versions will also build on this capability to improve the Zellij UI");
        let explanation_3 = Text::new(explanation_3_text);
        print_text_with_coordinates(explanation_3, base_x, base_y + 5, None, None);

        self.render_help(rows);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <ESC> - go back to main screen");
            let help = Text::new(help_text)
                .color_range(1, 6..=10);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
}

#[derive(Debug)]
enum Screen {
    Main(MainScreen),
    StackedResize(StackedResizeScreen),
    PinnedFloatingPanes(PinnedFloatingPanesScreen),
    NewThemeDefinitionSpec(NewThemeDefinitionSpecScreen),
    NewPluginApis(NewPluginApisScreen),
    MouseAnyEventHandling(MouseAnyEventHandlingScreen),
//     RebindLeaders(RebindLeadersScreen),
//     Presets(PresetsScreen),
}

impl Screen {
    pub fn main_menu_item_index(&self) -> Option<usize> {
        match self {
            Screen::Main(main_screen) => main_screen.selected_item_index(),
            _ => None
        }
    }
    pub fn go_to_stacked_resize(&mut self) {
        *self = Screen::StackedResize(Default::default());
    }
    pub fn go_to_pinned_floating_panes(&mut self) {
        *self = Screen::PinnedFloatingPanes(Default::default());
    }
    pub fn go_to_new_theme_definition_spec(&mut self) {
        *self = Screen::NewThemeDefinitionSpec(Default::default());
    }
    pub fn go_to_new_plugin_apis(&mut self) {
        *self = Screen::NewPluginApis(Default::default());
    }
    pub fn go_to_mouse_anyevent_handling(&mut self) {
        *self = Screen::MouseAnyEventHandling(Default::default());
    }
    pub fn render(&mut self, rows: usize, cols: usize, base_mode: InputMode) {
        match self {
            Screen::Main(ref mut screen) => {
                screen.render(rows, cols);
            }
            Screen::StackedResize(ref mut screen) => {
                screen.render(rows, cols);
            }
            Screen::PinnedFloatingPanes(ref mut screen) => {
                screen.render(rows, cols, base_mode);
            }
            Screen::NewThemeDefinitionSpec(ref mut screen) => {
                screen.render(rows, cols);
            }
            Screen::NewPluginApis(ref mut screen) => {
                screen.render(rows, cols);
            }
            Screen::MouseAnyEventHandling(ref mut screen) => {
                screen.render(rows, cols);
            }
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        match self {
            Screen::Main(ref mut main_screen) => {
                should_render = main_screen.handle_key(key);
            }
            _ => {}
        }
        should_render
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse) -> bool {
        let mut should_render = false;
        match self {
            Screen::Main(ref mut main_screen) => {
                match mouse_event {
                    Mouse::Hover(..) => {
                        should_render = main_screen.handle_mouse_event(mouse_event);
                    },
                    Mouse::LeftClick(..) => {
                        match main_screen.hover_item() {
                            Some(0) => {
                                self.go_to_stacked_resize();
                                should_render = true;
                            }
                            Some(1) => {
                                self.go_to_pinned_floating_panes();
                                should_render = true;
                            }
                            Some(2) => {
                                self.go_to_new_theme_definition_spec();
                                should_render = true;
                            }
                            Some(3) => {
                                self.go_to_new_plugin_apis();
                                should_render = true;
                            }
                            Some(4) => {
                                self.go_to_mouse_anyevent_handling();
                                should_render = true;
                            },
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            Screen::StackedResize(ref mut stacked_resize_screen) => {
                should_render = stacked_resize_screen.handle_mouse_event(mouse_event);
            }
            _ => {}
        }
        should_render
    }
    pub fn reset_state(&mut self, is_setup_wizard: bool) {
//         if is_setup_wizard {
//             Screen::new_reset_keybindings_screen(Some(0));
//         } else {
//             match self {
//                 Screen::RebindLeaders(r) => {
//                     let notification = r.drain_notification();
//                     *r = Default::default();
//                     r.set_notification(notification);
//                 },
//                 Screen::Presets(r) => {
//                     let notification = r.drain_notification();
//                     *r = Default::default();
//                     r.set_notification(notification);
//                 },
//             }
//         }
    }
    pub fn update_mode_info(&mut self, latest_mode_info: ModeInfo) {
//         match self {
//             Screen::RebindLeaders(r) => r.update_mode_info(latest_mode_info),
//             Screen::Presets(r) => r.update_mode_info(latest_mode_info),
//         }
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main(Default::default())
    }
}

impl Screen {
    pub fn new_reset_keybindings_screen(selected_index: Option<usize>) -> Self {
        unimplemented!()
        // Screen::Presets(PresetsScreen::new(selected_index))
    }
}

#[derive(Debug)]
struct LinkCoordinates {
    x: usize,
    y: usize,
    width: usize,
    destination_url: String,
}

impl LinkCoordinates {
    pub fn new(x: usize, y: usize, width: usize, destination_url: &str) -> Self {
        LinkCoordinates {
            x,
            y,
            width,
            destination_url: destination_url.to_owned()
        }
    }
    pub fn contains(&self, line: isize, col: usize) -> bool {
        line == self.y as isize && col >= self.x && col < self.x + self.width
    }
}

#[derive(Debug)]
struct ItemCoordinates {
    x: usize,
    y: usize,
    width: usize,
    index: usize,
}

impl ItemCoordinates {
    pub fn new(x: usize, y: usize, width: usize, index: usize) -> Self {
        ItemCoordinates {
            x,
            y,
            width,
            index,
        }
    }
    pub fn contains(&self, line: isize, col: usize) -> bool {
        line == self.y as isize && col >= self.x && col < self.x + self.width
    }
}

struct State {
    version: String,
    link_coordinates: Vec<LinkCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
    selected_item_index: Option<usize>,
    notification: Option<String>,
    is_setup_wizard: bool,
    is_release_notes: bool,
    ui_size: usize,
    current_screen: Screen,
    latest_mode_info: Option<ModeInfo>,
    colors: Palette,
    base_mode: InputMode,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: String::from("0.42.0"), // TODO: from Zellij
            link_coordinates: vec![],
            hover_coordinates: None,
            selected_item_index: None,
            notification: None,
            is_setup_wizard: false,
            is_release_notes: false,
            ui_size: UI_SIZE,
            current_screen: Screen::default(),
            latest_mode_info: None,
            colors: Palette::default(),
            base_mode: InputMode::default(),
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.is_release_notes = configuration
            .get("is_release_notes")
            .map(|v| v == "true")
            .unwrap_or(false);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::FailedToWriteConfigToDisk,
            EventType::ModeUpdate,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        if self.is_release_notes {
            rename_plugin_pane(own_plugin_id, format!("Release Notes {}", self.version));
        } else {
            rename_plugin_pane(own_plugin_id, "About Zellij");
        }
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Mouse(mouse_event) => {
                should_render = self.current_screen.handle_mouse_event(mouse_event);
            }
            Event::ModeUpdate(mode_info) => {
                // self.colors = mode_info.style.colors;
                let prev_base_mode = self.base_mode;
                if let Some(base_mode) = mode_info.base_mode {
                    self.base_mode = base_mode;
                    if prev_base_mode != self.base_mode {
                        should_render = true;
                    }
                }
                // self.base_mode = mode_info.base_mode;
//                 if self.latest_mode_info.as_ref().and_then(|l| l.base_mode) != mode_info.base_mode {
//                     // reset ui state
//                     self.current_screen.reset_state(self.is_setup_wizard);
//                 }
//                 self.latest_mode_info = Some(mode_info.clone());
//                 self.current_screen.update_mode_info(mode_info.clone());
                // should_render = true;
            },
            Event::Key(key) => {
                if self.notification.is_some() {
                    self.notification = None;
                    should_render = true;
                } else if key.bare_key == BareKey::Enter
                    && key.has_no_modifiers()
                    && !self.is_setup_wizard
                {
                    match self.current_screen.main_menu_item_index() {
                        Some(0) => {
                            self.current_screen.go_to_stacked_resize();
                            should_render = true;
                        }
                        Some(1) => {
                            self.current_screen.go_to_pinned_floating_panes();
                            should_render = true;
                        }
                        Some(2) => {
                            self.current_screen.go_to_new_theme_definition_spec();
                            should_render = true;
                        }
                        Some(3) => {
                            self.current_screen.go_to_new_plugin_apis();
                            should_render = true;
                        }
                        Some(4) => {
                            self.current_screen.go_to_mouse_anyevent_handling();
                            should_render = true;
                        },
                        _ => {}
                    }
                } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
                    match self.current_screen {
                        Screen::Main(_) => {
                            if self.current_screen.main_menu_item_index().is_some() {
                                // this clears up the selection as well as other pieces of state
                                self.current_screen = Default::default();
                                should_render = true;
                            } else {
                                close_self();
                            }
                        }
                        _ => {
                            self.current_screen = Default::default();
                            should_render = true;
                        }
                    }
                } else {
                    should_render = self.current_screen.handle_key(key);
                }
            },
            Event::FailedToWriteConfigToDisk(config_file_path) => {
                match config_file_path {
                    Some(failed_path) => {
                        self.notification = Some(format!(
                            "Failed to write configuration file: {}",
                            failed_path
                        ));
                    },
                    None => {
                        self.notification = Some(format!("Failed to write configuration file."));
                    },
                }
                should_render = true;
            },
            _ => (),
        };
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.current_screen.render(rows, cols, self.base_mode);
    }
}

impl State {
    fn render_sponsor_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 40, y, 34, "https://github.com/sponsors/imsnif");
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                let support_text = format!("Please support the Zellij developer <3:");
                self.link_coordinates.push(link_coordinates);
                let support = Text::new(support_text).color_range(3, 0..=38);
                print_text_with_coordinates(support, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif", y + 1, x + 41);
                return;
            }
        }
        let support_text = format!("Please support the Zellij developer <3: https://github.com/sponsors/imsnif");
        self.link_coordinates.push(link_coordinates);
        let support = Text::new(support_text).color_range(3, 0..=38);
        print_text_with_coordinates(support, x, y, None, None);
    }
    fn render_changelog_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 16, y, 51 + self.version.chars().count(), &format!("https://github.com/zellij-org/zellij/releases/tag/v{}", self.version));
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let full_changelog_text = format!("Full Changelog:");
                let full_changelog = Text::new(full_changelog_text);
                print_text_with_coordinates(full_changelog, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}", y + 1, x + 17, self.version);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let full_changelog_text = format!("Full Changelog: https://github.com/zellij-org/zellij/releases/tag/v{}", self.version);
        let full_changelog = Text::new(full_changelog_text);
        print_text_with_coordinates(full_changelog, x, y, None, None);
    }
    fn render_help(&self, rows: usize) {
        if self.hover_coordinates.is_some() {
            let help_text = format!("Help: Click or Shift-Click to open in browser");
            let help = Text::new(help_text)
                .color_range(3, 6..=10)
                .color_range(3, 15..=25);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else if self.selected_item_index.is_some() {
            let help_text = format!("Help: <↓↑> - Navigate, <ENTER> - Learn More, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=29)
                .color_range(1, 45..=49);
            print_text_with_coordinates(help, 0, rows, None, None);
        } else {
            let help_text = format!("Help: <↓↑> - Navigate, <ESC> - Dismiss");
            let help = Text::new(help_text)
                .color_range(1, 6..=9)
                .color_range(1, 23..=27);
            print_text_with_coordinates(help, 0, rows, None, None);
        }
    }
    fn move_selection_down(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(0);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 4 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index + 1);
            }
        }
    }
    fn move_selection_up(&mut self) {
        if self.selected_item_index.is_none() {
            self.selected_item_index = Some(4);
        } else if let Some(selected_item_index) = self.selected_item_index.take() {
            if selected_item_index == 0 {
                self.selected_item_index = None;
            } else {
                self.selected_item_index = Some(selected_item_index.saturating_sub(1));
            }
        }
    }
//     fn is_in_main_screen(&self) -> bool {
//         match &self.current_screen {
//             Screen::RebindLeaders(_) => true,
//             Screen::Presets(presets_screen) => {
//                 if self.is_setup_wizard || presets_screen.rebinding_leaders() {
//                     false
//                 } else {
//                     true
//                 }
//             },
//         }
//     }
//     fn switch_screen(&mut self) {
//         match &self.current_screen {
//             Screen::RebindLeaders(_) => {
//                 self.current_screen = Screen::Presets(Default::default());
//             },
//             Screen::Presets(_) => {
//                 self.current_screen = Screen::RebindLeaders(
//                     RebindLeadersScreen::default().with_mode_info(self.latest_mode_info.clone()),
//                 );
//             },
//         }
//         if let Some(mode_info) = &self.latest_mode_info {
//             self.current_screen.update_mode_info(mode_info.clone());
//         }
//     }
}
