use zellij_tile::prelude::*;

use std::collections::BTreeMap;
use std::cell::RefCell;
use std::rc::Rc;

const UI_ROWS: usize = 20;
const UI_COLUMNS: usize = 90;

#[derive(Debug)]
pub struct MainScreen {
    link_coordinates: Vec<LinkCoordinates>,
    item_coordinates: Vec<ItemCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
    pub selected_item_index: Option<usize>,
    pub selected_hover_index: Option<usize>,
}

impl Default for MainScreen {
    fn default() -> Self {
        MainScreen {
            link_coordinates: vec![],
            item_coordinates: vec![],
            hover_coordinates: None,
            selected_item_index: None,
            selected_hover_index: None,
        }
    }
}

impl MainScreen {
    pub fn render(&mut self, rows: usize, cols: usize, version: &str) {
        self.link_coordinates.clear();
        self.item_coordinates.clear();
        let ui_width = 74; // length of please support line
        let ui_height = 12;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("Hi there, welcome to Zellij {}!", version);
        let title = Text::new(title_text).color_range(2, 21..=27 + version.chars().count());
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

        self.render_changelog_link(base_x, base_y + 9, version);
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
    fn render_changelog_link(&mut self, x: usize, y: usize, version: &str) {
        let link_coordinates = LinkCoordinates::new(x + 16, y, 51 + version.chars().count(), &format!("https://github.com/zellij-org/zellij/releases/tag/v{}", version));
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let full_changelog_text = format!("Full Changelog:");
                let full_changelog = Text::new(full_changelog_text);
                print_text_with_coordinates(full_changelog, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}", y + 1, x + 17, version);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let full_changelog_text = format!("Full Changelog: https://github.com/zellij-org/zellij/releases/tag/v{}", version);
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
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse, open_command: Option<&str>) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        if let Some(open_command) = open_command {
                            run_command(
                                &[open_command, &link_coordinates.destination_url],
                                 Default::default()
                            );
                        } else {
                            eprintln!("Did not find xdg-open or open, cannot open link");
                        }
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
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse, open_command: Option<&str>) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        if let Some(open_command) = open_command {
                            run_command(
                                &[open_command, &link_coordinates.destination_url],
                                 Default::default()
                            );
                        } else {
                            eprintln!("Did not find xdg-open or open, cannot open link");
                        }
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
        let help_text = format!("Help: <ESC> - go back to main screen");
        let help = Text::new(help_text)
            .color_range(1, 6..=10);
        print_text_with_coordinates(help, 0, rows, None, None);
    }
}

#[derive(Debug, Default)]
struct NewThemeDefinitionSpecScreen {
    link_coordinates: Vec<LinkCoordinates>,
    hover_coordinates: Option<(isize, usize)>, // line/col
}

impl NewThemeDefinitionSpecScreen {
    pub fn render(&mut self, rows: usize, cols: usize) {
        let ui_width = 79; // length of third explanation line
        let ui_height = 9;
        let base_x = cols.saturating_sub(ui_width) / 2;
        let base_y = rows.saturating_sub(ui_height) / 2;

        let title_text = format!("New Theme Definition Spec");
        let title = Text::new(title_text).color_range(0, ..);
        print_text_with_coordinates(title, base_x, base_y, None, None);

        let explanation_1_text = format!("Starting this version, themes can be defined by UI components");
        let explanation_1 = Text::new(explanation_1_text).color_range(3, 37..=60);
        print_text_with_coordinates(explanation_1, base_x, base_y + 2, None, None);

        let explanation_2_text = format!("instead of the previously obscure color-to-color definitions.");
        let explanation_2 = Text::new(explanation_2_text);
        print_text_with_coordinates(explanation_2, base_x, base_y + 3, None, None);

        let explanation_3_text = format!("This both improves the convenience of theme creation and allows greater freedom");
        let explanation_3 = Text::new(explanation_3_text);
        print_text_with_coordinates(explanation_3, base_x, base_y + 5, None, None);

        let explanation_4_text = format!("for theme authors.");
        let explanation_4 = Text::new(explanation_4_text);
        print_text_with_coordinates(explanation_4, base_x, base_y + 6, None, None);

        self.render_docs_link(base_x, base_y + 8);
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
    fn render_docs_link(&mut self, x: usize, y: usize) {
        let link_coordinates = LinkCoordinates::new(x + 22, y, 39, &format!("https://zellij.dev/documentation/themes"));
        if let Some((line, col)) = self.hover_coordinates.as_ref() {
            if link_coordinates.contains(*line, *col) {
                self.link_coordinates.push(link_coordinates);
                let theme_link_text = format!("For more information:");
                let theme_link = Text::new(theme_link_text).color_range(2, ..);
                print_text_with_coordinates(theme_link, x, y, None, None);
                print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/documentation/themes", y + 1, x + 23);
                return;
            }
        }
        self.link_coordinates.push(link_coordinates);
        let theme_link_text = format!("For more information: https://zellij.dev/documentation/themes");
        let theme_link = Text::new(theme_link_text).color_range(2, ..=20);
        print_text_with_coordinates(theme_link, x, y, None, None);
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse, open_command: Option<&str>) -> bool {
        let mut should_render = false;
        match mouse_event {
            Mouse::LeftClick(line, col) => {
                for link_coordinates in &self.link_coordinates {
                    if link_coordinates.contains(line, col) {
                        if let Some(open_command) = open_command {
                            run_command(
                                &[open_command, &link_coordinates.destination_url],
                                 Default::default()
                            );
                        } else {
                            eprintln!("Did not find xdg-open or open, cannot open link");
                        }
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
    pub fn render(&mut self, rows: usize, cols: usize, base_mode: InputMode, version: &str) {
        match self {
            Screen::Main(ref mut screen) => {
                screen.render(rows, cols, version);
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
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse, open_command: Option<&str>) -> bool {
        let mut should_render = false;
        match self {
            Screen::Main(ref mut main_screen) => {
                match mouse_event {
                    Mouse::Hover(..) => {
                        should_render = main_screen.handle_mouse_event(mouse_event, open_command);
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
                            _ => {
                                should_render = main_screen.handle_mouse_event(mouse_event, open_command);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Screen::StackedResize(ref mut stacked_resize_screen) => {
                should_render = stacked_resize_screen.handle_mouse_event(mouse_event, open_command);
            }
            Screen::NewThemeDefinitionSpec(ref mut new_theme_screen) => {
                should_render = new_theme_screen.handle_mouse_event(mouse_event, open_command);
            }
            _ => {}
        }
        should_render
    }
}

impl Default for Screen {
    fn default() -> Self {
        Screen::Main(Default::default())
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
    zellij_version: String,
    own_plugin_id: Option<u32>,
    notification: Option<String>,
    is_setup_wizard: bool,
    is_release_notes: bool,
    current_screen: Screen,
    base_mode: InputMode,
    found_xdg_open_cli: bool,
    found_open_cli: bool,
    tab_rows: usize,
    tab_columns: usize,
    app: Option<App>,
}

impl Default for State {
    fn default() -> Self {
        State {
            zellij_version: String::from("0.42.0"), // TODO: from Zellij
            own_plugin_id: None,
            notification: None,
            is_setup_wizard: false,
            is_release_notes: false,
            current_screen: Screen::default(),
            base_mode: InputMode::default(),
            found_xdg_open_cli: false,
            found_open_cli: false,
            tab_rows: 0,
            tab_columns: 0,

            app: None,
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
            EventType::RunCommandResult,
            EventType::TabUpdate,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        self.own_plugin_id = Some(own_plugin_id);
        self.zellij_version = get_zellij_version();
        if self.is_release_notes {
            rename_plugin_pane(own_plugin_id, format!("Release Notes {}", self.zellij_version));
        } else {
            rename_plugin_pane(own_plugin_id, "About Zellij");
        }
        let mut xdg_open_context = BTreeMap::new();
        xdg_open_context.insert("xdg_open_cli".to_owned(), String::new());
        run_command(
            &["xdg-open", "--help"],
            xdg_open_context,
        );
        let mut open_context = BTreeMap::new();
        open_context.insert("open_cli".to_owned(), String::new());
        run_command(
            &["open", "--help"],
            open_context,
        );
        // TODO: CONTINUE HERE - integrate app better (maybe consider turning it into the new
        // State?) then delete all the dead code and commit - yay!
        self.app = Some(App::new(self.zellij_version.clone(), Rc::new(RefCell::new("".to_owned())), Rc::new(RefCell::new(InputMode::Normal))));
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::TabUpdate(tab_info) => {
                // TODO: move to app
                self.center_own_pane(tab_info);
            }
            Event::Mouse(mouse_event) => {
                self.app.as_mut().unwrap().handle_mouse_event(mouse_event);
                if self.app.as_ref().unwrap().should_render() {
                    should_render = true;
                }
//                 let open_command = if self.found_xdg_open_cli {
//                     Some("xdg-open")
//                 } else if self.found_open_cli {
//                     Some("open")
//                 } else {
//                     None
//                 };
//                should_render = self.current_screen.handle_mouse_event(mouse_event, open_command);
            }
            Event::ModeUpdate(mode_info) => {
                // TODO: move to App
                let prev_base_mode = self.base_mode;
                if let Some(base_mode) = mode_info.base_mode {
                    self.app.as_mut().unwrap().update_base_mode(base_mode);
                    self.base_mode = base_mode;
                    if prev_base_mode != self.base_mode {
                        should_render = true;
                    }
                }
            },
            Event::RunCommandResult(exit_code, _stdout, _stderr, context) => {
                // TODO: handle in App
                let is_xdg_open = context.get("xdg_open_cli").is_some();
                let is_open = context.get("open_cli").is_some();
                if is_xdg_open {
                    if exit_code == Some(0) {
                        self.app.as_mut().unwrap().update_link_executable("xdg-open".to_owned());
                        self.found_xdg_open_cli = true;
                    }
                } else if is_open {
                    if exit_code == Some(0) {
                        self.app.as_mut().unwrap().update_link_executable("open".to_owned());
                        self.found_open_cli = true;
                    }
                }
            }
            Event::Key(key) => {
                 should_render = self.app.as_mut().unwrap().handle_key(key);
//                 // TODO: handle in App
//                 if self.notification.is_some() {
//                     self.notification = None;
//                     should_render = true;
//                 } else if key.bare_key == BareKey::Enter
//                     && key.has_no_modifiers()
//                     && !self.is_setup_wizard
//                 {
//                     match self.current_screen.main_menu_item_index() {
//                         Some(0) => {
//                             self.current_screen.go_to_stacked_resize();
//                             should_render = true;
//                         }
//                         Some(1) => {
//                             self.current_screen.go_to_pinned_floating_panes();
//                             should_render = true;
//                         }
//                         Some(2) => {
//                             self.current_screen.go_to_new_theme_definition_spec();
//                             should_render = true;
//                         }
//                         Some(3) => {
//                             self.current_screen.go_to_new_plugin_apis();
//                             should_render = true;
//                         }
//                         Some(4) => {
//                             self.current_screen.go_to_mouse_anyevent_handling();
//                             should_render = true;
//                         },
//                         _ => {}
//                     }
//                 } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
//                     match self.current_screen {
//                         Screen::Main(_) => {
//                             if self.current_screen.main_menu_item_index().is_some() {
//                                 // this clears up the selection as well as other pieces of state
//                                 self.current_screen = Default::default();
//                                 should_render = true;
//                             } else {
//                                 close_self();
//                             }
//                         }
//                         _ => {
//                             self.current_screen = Default::default();
//                             should_render = true;
//                         }
//                     }
//                 } else {
//                     should_render = self.current_screen.handle_key(key);
//                 }
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
        self.app.as_mut().unwrap().render(rows, cols);
        // self.current_screen.render(rows, cols, self.base_mode, &self.zellij_version);
    }
}

impl State {
    fn center_own_pane(&mut self, tab_info: Vec<TabInfo>) {
        // we only take the size of the first tab because at the time of writing this is
        // identical to all tabs, but this might not always be the case...
        if let Some(first_tab) = tab_info.get(0) {
            let prev_tab_columns = self.tab_columns;
            let prev_tab_rows = self.tab_rows;
            self.tab_columns = first_tab.display_area_columns;
            self.tab_rows = first_tab.display_area_rows;
            if self.tab_columns != prev_tab_columns || self.tab_rows != prev_tab_rows {
                let desired_x_coords = self.tab_columns.saturating_sub(UI_COLUMNS) / 2;
                let desired_y_coords = self.tab_rows.saturating_sub(UI_ROWS) / 2;
                change_floating_panes_coordinates(vec![(PaneId::Plugin(self.own_plugin_id.unwrap()), FloatingPaneCoordinates::new(
                    Some(desired_x_coords.to_string()),
                    Some(desired_y_coords.to_string()),
                    Some(UI_COLUMNS.to_string()),
                    Some(UI_ROWS.to_string()),
                    None
                ).unwrap())]);
            }
        }
    }
}

#[derive(Debug)]
struct App {
    active_page: Page,
    should_render: bool,
    link_executable: Rc<RefCell<String>>,
    zellij_version: String,
    base_mode: Rc<RefCell<InputMode>>,

}
impl App {
    pub fn new(zellij_version: String, link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Self {
        App {
            active_page: new_main_screen(link_executable.clone(), zellij_version.clone(), base_mode.clone()),
            should_render: true,
            link_executable,
            zellij_version,
            base_mode
        }
    }
    pub fn should_render(&self) -> bool {
        self.should_render
    }
    pub fn update_link_executable(&mut self, new_link_executable: String) {
        *self.link_executable.borrow_mut() = new_link_executable;
    }
    pub fn update_base_mode(&mut self, new_base_mode: InputMode) {
        *self.base_mode.borrow_mut() = new_base_mode;
    }
    pub fn handle_mouse_event(&mut self, mouse_event: Mouse) {
        match mouse_event {
            Mouse::LeftClick(line, column) => {
                if let Some(new_page) = self.active_page.handle_mouse_left_click(column, line as usize) {
                    self.active_page = new_page;
                    self.should_render = true;
                }
            }
            Mouse::Hover(line, column) => {
                if self.active_page.handle_mouse_hover(column, line as usize) {
                    self.should_render = true;
                }
            }
            _ => {}
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let mut should_render = false;
        if key.bare_key == BareKey::Enter && key.has_no_modifiers() {
            if let Some(new_page) = self.active_page.handle_selection() {
                self.active_page = new_page;
                should_render = true;
            }
        } else if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            if self.active_page.is_main_screen {
                close_self();
            } else {
                self.active_page = new_main_screen(self.link_executable.clone(), self.zellij_version.clone(), self.base_mode.clone());
                should_render = true;
            }
        } else {
            should_render = self.active_page.handle_key(key);
        }
        should_render

    }
    pub fn render(&mut self, rows: usize, cols: usize) {
        self.active_page.render(rows, cols);
        self.should_render = false;
    }
}

fn new_main_screen(link_executable: Rc<RefCell<String>>, zellij_version: String, base_mode: Rc<RefCell<InputMode>>) -> Page {
    Page::new()
        .main_screen()
        .with_title(main_screen_title(&zellij_version))
        .with_bulletin_list(BulletinList::new(whats_new_title())
            .with_items(vec![
                ActiveComponent::new(TextOrCustomRender::Text(main_menu_item("Stacked Resize")))
                    .with_hover(TextOrCustomRender::Text(main_menu_item("Stacked Resize").selected()))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_stacked_resize(link_executable.clone())
                    })),
                ActiveComponent::new(TextOrCustomRender::Text(main_menu_item("Pinned Floating Panes")))
                    .with_hover(TextOrCustomRender::Text(main_menu_item("Pinned Floating Panes").selected()))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_pinned_panes(link_executable.clone(), base_mode.clone())
                    })),
                ActiveComponent::new(TextOrCustomRender::Text(main_menu_item("New Theme Definition Spec")))
                    .with_hover(TextOrCustomRender::Text(main_menu_item("New Theme Definition Spec").selected()))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_theme_definition_spec(link_executable.clone())
                    })),
                ActiveComponent::new(TextOrCustomRender::Text(main_menu_item("New Plugin APIs")))
                    .with_hover(TextOrCustomRender::Text(main_menu_item("New Plugin APIs").selected()))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_plugin_apis(link_executable.clone())
                    })),
                ActiveComponent::new(TextOrCustomRender::Text(main_menu_item("Mouse Any-Event Handling")))
                    .with_hover(TextOrCustomRender::Text(main_menu_item("Mouse Any-Event Handling").selected()))
                    .with_left_click_action(ClickAction::new_change_page({
                        let link_executable = link_executable.clone();
                        move || Page::new_mouse_any_event(link_executable.clone())
                    }))
            ])
        )
        .with_paragraph(vec![
            ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(full_changelog_text())),
                ActiveComponent::new(TextOrCustomRender::Text(changelog_link_unselected(zellij_version.clone())))
                    .with_hover(TextOrCustomRender::CustomRender(Box::new(changelog_link_selected(zellij_version.clone())), Box:: new(changelog_link_selected_len(zellij_version.clone()))))
                    .with_left_click_action(ClickAction::new_open_link(
                        format!("https://github.com/zellij-org/zellij/releases/tag/v{}", zellij_version),
                        link_executable.clone())
                    )
            ])
        ])
        .with_paragraph(vec![
            ComponentLine::new(vec![
                ActiveComponent::new(TextOrCustomRender::Text(support_the_developer_text())),
                ActiveComponent::new(TextOrCustomRender::Text(sponsors_link_text_unselected()))
                    .with_hover(TextOrCustomRender::CustomRender(Box::new(sponsors_link_text_selected), Box::new(sponsors_link_text_selected_len)))
                    .with_left_click_action(ClickAction::new_open_link("https://github.com/sponsors/imsnif".to_owned(), link_executable.clone()))
            ])
        ])
        .with_help(Box::new(|hovering_over_link, menu_item_is_selected| main_screen_help_text(hovering_over_link, menu_item_is_selected)))
}

fn full_changelog_text() -> Text {
    Text::new("Full Changelog: ")
}

fn changelog_link_unselected(version: String) -> Text {
    let full_changelog_text = format!("https://github.com/zellij-org/zellij/releases/tag/v{}", version);
    Text::new(full_changelog_text)
}

fn changelog_link_selected(version: String) -> Box<dyn Fn(usize, usize)-> usize>  {
    Box::new(move |x, y|{
        print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/zellij-org/zellij/releases/tag/v{}", y + 1, x + 1, version);
        51 + version.chars().count()
    })
}

fn changelog_link_selected_len(version: String) -> Box<dyn Fn() -> usize>  {
    Box::new(move ||{
        51 + version.chars().count()
    })
}

fn sponsors_link_text_unselected() -> Text {
    Text::new("https://github.com/sponsors/imsnif")
}

fn sponsors_link_text_selected(x: usize, y: usize) -> usize {
    print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://github.com/sponsors/imsnif", y + 1, x + 1);
    34
}

fn sponsors_link_text_selected_len() -> usize {
    34
}

fn stacked_resize_screencast_link_selected(x: usize, y: usize) -> usize {
    print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/screencasts/stacked-resize", y + 1, x + 1);
    45
}

fn stacked_resize_screencast_link_selected_len() -> usize {
    45
}

fn theme_link_selected(x: usize, y: usize) -> usize {
    print!("\u{1b}[{};{}H\u{1b}[m\u{1b}[1;4mhttps://zellij.dev/documentation/themes", y + 1, x + 1);
    39
}
fn theme_link_selected_len() -> usize {
    39
}

// Text components
fn whats_new_title() -> Text {
    Text::new("What's new?")
}

fn main_screen_title(version: &str) -> Text {
    let title_text = format!("Hi there, welcome to Zellij {}!", version);
    Text::new(title_text).color_range(2, 21..=27 + version.chars().count())
}

fn main_screen_help_text(hovering_over_link: bool, menu_item_is_selected: bool) -> Text {
    if hovering_over_link {
        let help_text = format!("Help: Click or Shift-Click to open in browser");
        Text::new(help_text)
            .color_range(3, 6..=10)
            .color_range(3, 15..=25)
    } else if menu_item_is_selected {
        let help_text = format!("Help: <↓↑> - Navigate, <ENTER> - Learn More, <ESC> - Dismiss");
        Text::new(help_text)
            .color_range(1, 6..=9)
            .color_range(1, 23..=29)
            .color_range(1, 45..=49)
    } else {
        let help_text = format!("Help: <↓↑> - Navigate, <ESC> - Dismiss");
        Text::new(help_text)
            .color_range(1, 6..=9)
            .color_range(1, 23..=27)
    }
}

fn esc_go_back_plus_link_hover(hovering_over_link: bool, _menu_item_is_selected: bool) -> Text {
    if hovering_over_link {
        let help_text = format!("Help: Click or Shift-Click to open in browser");
        Text::new(help_text)
            .color_range(3, 6..=10)
            .color_range(3, 15..=25)
    } else {
        let help_text = format!("Help: <ESC> - Go back");
        Text::new(help_text)
            .color_range(1, 6..=10)
    }
}


fn esc_to_go_back_help() -> Text {
    let help_text = format!("Help: <ESC> - Go back");
    Text::new(help_text)
        .color_range(1, 6..=10)
}

fn main_menu_item(item_name: &str) -> Text {
    Text::new(item_name).color_range(0, ..)
}

fn support_the_developer_text() -> Text {
    let support_text = format!("Please support the Zellij developer <3: ");
    Text::new(support_text).color_range(3, ..)
}

enum ClickAction {
    ChangePage(Box<dyn FnOnce() -> Page>),
    OpenLink(String, Rc<RefCell<String>>), // (destination, executable)
}

impl std::fmt::Debug for ClickAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickAction::ChangePage(_) => write!(f, "ChangePage"),
            ClickAction::OpenLink(destination, executable) => write!(f, "OpenLink: {}, {:?}", destination, executable),
        }
    }
}

impl ClickAction {
    pub fn new_change_page<F>(go_to_page: F) -> Self
    where F: FnOnce() -> Page + 'static
    {
        ClickAction::ChangePage(Box::new(go_to_page))
    }
    pub fn new_open_link(destination: String, executable: Rc<RefCell<String>>) -> Self {
        ClickAction::OpenLink(destination, executable)
    }
}

enum TextOrCustomRender {
    Text(Text),
    CustomRender(
        Box<dyn Fn(usize, usize) -> usize>, // (rows, columns) -> text_len (render function)
        Box<dyn Fn() -> usize>, // length of rendered component
    ),
}

impl TextOrCustomRender {
    pub fn len(&self) -> usize {
        match self {
            TextOrCustomRender::Text(text) => text.len(),
            TextOrCustomRender::CustomRender(_render_fn, len_fn) => len_fn(),
        }
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) -> usize {
        match self {
            TextOrCustomRender::Text(text) => {
                print_text_with_coordinates(text.clone(), x, y, Some(columns), Some(rows));
                text.len()
            },
            TextOrCustomRender::CustomRender(render_fn, _len_fn) => {
                render_fn(x, y)
            }
        }
    }
}

impl std::fmt::Debug for TextOrCustomRender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextOrCustomRender::Text(text) => write!(f, "Text {{ {:?} }}", text),
            TextOrCustomRender::CustomRender(..) => write!(f, "CustomRender"),
        }
    }
}

#[derive(Debug)]
struct ActiveComponent {
    text_no_hover: TextOrCustomRender,
    text_hover: Option<TextOrCustomRender>,
    left_click_action: Option<ClickAction>,
    last_rendered_coordinates: Option<ComponentCoordinates>,
    pub is_active: bool,
}

impl ActiveComponent {
    pub fn new(text_no_hover: TextOrCustomRender) -> Self {
        ActiveComponent {
            text_no_hover,
            text_hover: None,
            left_click_action: None,
            is_active: false,
            last_rendered_coordinates: None,
        }
    }
    pub fn with_hover(mut self, text_hover: TextOrCustomRender) -> Self {
        self.text_hover = Some(text_hover);
        self
    }
    pub fn with_left_click_action(mut self, left_click_action: ClickAction) -> Self {
        self.left_click_action = Some(left_click_action);
        self
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) -> usize{
        let mut component_width = 0;
        match self.text_hover.as_mut() {
            Some(text) if self.is_active => {
                let text_len = text.render(x, y, rows, columns);
                component_width += text_len;
            },
            _ => {
                let text_len = self.text_no_hover.render(x, y, rows, columns);
                component_width += text_len;
            }
        }
        self.last_rendered_coordinates = Some(ComponentCoordinates::new(x, y, 1, columns));
        component_width
    }
    pub fn left_click_action(&mut self) -> Option<Page> {
        match self.left_click_action.take() {
            Some(ClickAction::ChangePage(go_to_page)) => Some(go_to_page()),
            Some(ClickAction::OpenLink(link, executable)) => {
                self.left_click_action = Some(ClickAction::OpenLink(link.clone(), executable.clone()));
                run_command(
                    &[&executable.borrow(), &link],
                     Default::default()
                );
                None
            }
            None => None
        }
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        let Some(last_rendered_coordinates) = &self.last_rendered_coordinates else {
            return None;
        };
        if last_rendered_coordinates.contains(x, y) {
            self.left_click_action()
        } else {
            None
        }
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        let Some(last_rendered_coordinates) = &self.last_rendered_coordinates else {
            return false;
        };
        if last_rendered_coordinates.contains(x, y) && self.text_hover.is_some() {
            self.is_active = true;
            true
        } else {
            false
        }
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        if self.is_active {
            self.left_click_action()
        } else {
            None
        }
    }
    pub fn column_count(&self) -> usize {
        match self.text_hover.as_ref() {
            Some(text) if self.is_active => {
                text.len()
            },
            _ => {
                self.text_no_hover.len()
            }
        }
    }
    pub fn clear_hover(&mut self) {
        self.is_active = false;
    }
}

#[derive(Debug)]
struct ComponentCoordinates {
    x: usize,
    y: usize,
    rows: usize,
    columns: usize
}

impl ComponentCoordinates {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.columns &&
            y >= self.y && y < self.y + self.rows
    }
}

impl ComponentCoordinates {
    pub fn new(x: usize, y: usize, rows: usize, columns: usize) -> Self {
        ComponentCoordinates {
            x,
            y,
            rows,
            columns,
        }
    }
}

#[derive(Debug)]
struct Page {
    title: Option<Text>,
    components_to_render: Vec<RenderedComponent>,
    component_coordinates: Vec<ComponentCoordinates>,
    should_render: bool,
    has_hover: bool,
    hovering_over_link: bool,
    menu_item_is_selected: bool,
    pub is_main_screen: bool,
}

// Pages
impl Page {
    pub fn new_stacked_resize(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("Stacked Resize").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("This version includes a new resizing algorithm that helps better manage panes"))),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("into stacks."))),
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new("To try it out:").color_range(2, ..))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Hide this pane with Alt f (you can bring it back with Alt f again)")
                                .color_range(3, 20..=24)
                                .color_range(3, 54..=58)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Open 4-5 panes with Alt n")
                                .color_range(3, 20..=24)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt + until you reach full screen")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("Press Alt - until you are back at the original state")
                                .color_range(3, 6..=10)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("5. You can always snap back to the built-in swap layouts with Alt <[]>")
                                .color_range(3, 62..=64)
                                .color_range(3, 67..=68)
                    )),
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("To disable, add stacked_resize false to the Zellij Configuration")
                                .color_range(3, 16..=35)
                    )),
                ])
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more details, see: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/screencasts/stacked-resize")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(stacked_resize_screencast_link_selected), Box::new(stacked_resize_screencast_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/screencasts/stacked-resize".to_owned(), link_executable.clone()))
                ])
            ])
            .with_help(Box::new(|hovering_over_link, menu_item_is_selected| esc_go_back_plus_link_hover(hovering_over_link, menu_item_is_selected)))
    }
    fn new_pinned_panes(link_executable: Rc<RefCell<String>>, base_mode: Rc<RefCell<InputMode>>) -> Page {
        Page::new().with_title(Text::new("Pinned Floating Panes").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("This version adds the ability to \"pin\" a floating pane so that it")))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("will always be visible even if floating panes are hidden.")))
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new(format!("Floating panes can be \"pinned\": ")).color_range(2, ..))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new(format!("With a mouse click on their top right corner"))
                                .color_range(3, 7..=17)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        match *base_mode.borrow() {
                            InputMode::Locked => {
                                Text::new(format!("With Ctrl g + p + i"))
                                    .color_range(3, 5..=10)
                                    .color_range(3, 14..15)
                                    .color_range(3, 18..19)
                            },
                            _ => {
                                Text::new("With Ctrl p + i")
                                    .color_range(3, 5..=10)
                                    .color_range(3, 14..15)
                            }
                        }
                    ))
                ])
            )
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new("A great use case for these is to tail log files or to show")
                    )),
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                            Text::new(format!("real-time compiler output while working in other panes."))
                    )),
                ])
            ])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| esc_to_go_back_help()))
    }
    fn new_theme_definition_spec(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("New Theme Definition Spec").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Starting this version, themes can be defined by UI components")
                            .color_range(3, 37..=60)
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("instead of the previously obscure color-to-color definitions.")
                    ))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("This both improves the convenience of theme creation and allows greater freedom")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("for theme authors.")
                    ))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("For more information: ")
                            .color_range(2, ..)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(Text::new("https://zellij.dev/documentation/themes")))
                        .with_hover(TextOrCustomRender::CustomRender(Box::new(theme_link_selected), Box::new(theme_link_selected_len)))
                        .with_left_click_action(ClickAction::new_open_link("https://zellij.dev/documentation/themes".to_owned(), link_executable.clone()))
                ])
            ])
            .with_help(Box::new(|hovering_over_link, menu_item_is_selected| esc_go_back_plus_link_hover(hovering_over_link, menu_item_is_selected)))
    }
    fn new_plugin_apis(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("New Plugin APIs").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("New APIs were added in this version affording plugins")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("finer control over the workspace.")
                    ))
                ]),
            ])
            .with_bulletin_list(BulletinList::new(Text::new("Some examples:").color_range(2, ..))
                .with_items(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Change floating panes' coordinates and size")
                            .color_range(3, 23..=33)
                            .color_range(3, 39..=42)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Stack arbitrary panes")
                            .color_range(3, ..=4)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Change /host folder")
                            .color_range(3, 7..=11)
                    )),
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Discover the user's $SHELL and $EDITOR")
                            .color_range(3, 20..=25)
                            .color_range(3, 31..=37)
                    ))
                ])
            )
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| esc_to_go_back_help()))
    }
    fn new_mouse_any_event(link_executable: Rc<RefCell<String>>) -> Page {
        Page::new()
            .with_title(Text::new("Mosue Any-Event Tracking").color_range(0, ..))
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("This version adds the capability to track mouse motions more accurately")
                    ))
                ]),
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("both in Zellij, in terminal panes and in plugin panes.")
                    ))
                ]),
            ])
            .with_paragraph(vec![
                ComponentLine::new(vec![
                    ActiveComponent::new(TextOrCustomRender::Text(
                        Text::new("Future versions will also build on this capability to improve the Zellij UI")
                    ))
                ]),
            ])
            .with_help(Box::new(|_hovering_over_link, _menu_item_is_selected| esc_to_go_back_help()))
    }
}

impl Page {
    pub fn new() -> Self {
        Page {
            title: None,
            components_to_render: vec![],
            component_coordinates: vec![],
            should_render: false,
            has_hover: false,
            hovering_over_link: false,
            menu_item_is_selected: false,
            is_main_screen: false,
        }
    }
    pub fn main_screen(mut self) -> Self {
        self.is_main_screen = true;
        self
    }
    pub fn with_title(mut self, title: Text) -> Self {
        self.title = Some(title);
        self
    }
    pub fn with_bulletin_list(mut self, bulletin_list: BulletinList) -> Self {
        self.components_to_render.push(RenderedComponent::BulletinList(bulletin_list));
        self
    }
    pub fn with_paragraph(mut self, paragraph: Vec<ComponentLine>) -> Self {
        self.components_to_render.push(RenderedComponent::Paragraph(paragraph));
        self
    }
    pub fn with_help(mut self, help_text_fn: Box<dyn Fn(bool, bool) -> Text>) -> Self {
        self.components_to_render.push(RenderedComponent::HelpText(help_text_fn));
        self
    }
    pub fn should_render(&self) -> bool {
        self.should_render
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
    pub fn handle_mouse_left_click(&mut self, x: usize, y: usize) -> Option<Page> {
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let page_to_render = bulletin_list.handle_left_click_at_position(x, y);
                    if page_to_render.is_some() {
                        return page_to_render;
                    }
                }
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        let page_to_render = component_line.handle_left_click_at_position(x, y);
                        if page_to_render.is_some() {
                            return page_to_render;
                        }
                    }
                },
                _ => {}
            }
        }
        None
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let page_to_render = bulletin_list.handle_selection();
                    if page_to_render.is_some() {
                        return page_to_render;
                    }
                },
                _ => {}
            }
        }
        None
    }
    pub fn handle_mouse_hover(&mut self, x: usize, y: usize) -> bool {
        let hover_cleared = self.clear_hover(); // TODO: do the right thing if the same component was hovered from
                                                // previous motion
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    let should_render = bulletin_list.handle_hover_at_position(x, y);
                    if should_render {
                        self.has_hover = true;
                        self.menu_item_is_selected = true;
                        return should_render;
                    }
                }
                RenderedComponent::Paragraph(paragraph) => {
                    for component_line in paragraph {
                        let should_render = component_line.handle_hover_at_position(x, y);
                        if should_render {
                            self.has_hover = true;
                            self.hovering_over_link = true;
                            return should_render;
                        }
                    }
                },
                _ => {}
            }
        }
        hover_cleared
    }
    fn move_selection_up(&mut self) {
        match self.position_of_active_bulletin() {
            Some(position_of_active_bulletin) if position_of_active_bulletin > 0 => {
                self.clear_active_bulletins();
                self.set_active_bulletin(position_of_active_bulletin.saturating_sub(1));
            },
            Some(0) => {
                self.clear_active_bulletins();
            },
            _ => {
                self.clear_active_bulletins();
                self.set_last_active_bulletin();
            }
        }

    }
    fn move_selection_down(&mut self) {
        match self.position_of_active_bulletin() {
            Some(position_of_active_bulletin) => {
                self.clear_active_bulletins();
                self.set_active_bulletin(position_of_active_bulletin + 1);
            },
            None => {
                self.set_active_bulletin(0);
            }
        }
    }
    fn position_of_active_bulletin(&self) -> Option<usize> {
        self.components_to_render.iter().find_map(|c| match c {
            RenderedComponent::BulletinList(bulletin_list) => bulletin_list.active_component_position(),
            _ => None
        })
    }
    fn clear_active_bulletins(&mut self) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => Some(bulletin_list.clear_active_bulletins()),
                _ => None
            };
        });
    }
    fn set_active_bulletin(&mut self, active_bulletin_position: usize) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => bulletin_list.set_active_bulletin(active_bulletin_position),
                _ => {}
            };
        });
    }
    fn set_last_active_bulletin(&mut self) {
        self.components_to_render.iter_mut().for_each(|c| {
            match c {
                RenderedComponent::BulletinList(bulletin_list) => bulletin_list.set_last_active_bulletin(),
                _ => {}
            };
        });
    }
    fn clear_hover(&mut self) -> bool {
        let had_hover = self.has_hover;
        self.menu_item_is_selected = false;
        self.hovering_over_link = false;
        for rendered_component in &mut self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    bulletin_list.clear_hover();
                }
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        active_component.clear_hover();
                    }
                },
                _ => {}
            }
        }
        self.has_hover = false;
        had_hover
    }
    pub fn ui_column_count(&mut self) -> usize {
        let mut column_count = 0;
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    column_count = std::cmp::max(column_count, bulletin_list.column_count());
                }
                RenderedComponent::Paragraph(paragraph) => {
                    for active_component in paragraph {
                        column_count = std::cmp::max(column_count, active_component.column_count());
                    }
                }
                RenderedComponent::Text(text) => {
                    column_count = std::cmp::max(column_count, text.len())
                }
                RenderedComponent::HelpText(_text) => {} // we ignore help text in column
                                                         // calculation because it's always left
                                                         // justified
            }
        }
        column_count
    }
    pub fn ui_row_count(&mut self) -> usize {
        let mut row_count = 0;
        if self.title.is_some() {
            row_count += 1;
        }
        for rendered_component in &self.components_to_render {
            match rendered_component {
                RenderedComponent::BulletinList(bulletin_list) => {
                    row_count += bulletin_list.len();
                }
                RenderedComponent::Paragraph(paragraph) => {
                    row_count += paragraph.len();
                }
                RenderedComponent::Text(_text) => {
                    row_count += 1;
                }
                RenderedComponent::HelpText(_text) => {} // we ignore help text as it is outside
                                                         // the UI container
            }
        }
        row_count += self.components_to_render.len();
        row_count
    }
    pub fn render(&mut self, rows: usize, columns: usize) {
        let base_x = columns.saturating_sub(self.ui_column_count()) / 2;
        let base_y = rows.saturating_sub(self.ui_row_count()) / 2;
        let mut current_y = base_y;
        if let Some(title) = &self.title {
            print_text_with_coordinates(title.clone(), base_x, current_y, Some(columns), Some(rows));
            current_y += 2;
        }
        for rendered_component in &mut self.components_to_render {
            let is_help = match rendered_component {
                RenderedComponent::HelpText(_) => true,
                _ => false,
            };
            let y = if is_help {
                rows
            } else {
                current_y
            };
            let rendered_rows = rendered_component.render(base_x, y, rows, columns.saturating_sub(base_x * 2), self.hovering_over_link, self.menu_item_is_selected);
            current_y += rendered_rows + 1; // 1 for the line space between components
        }
    }
}

enum RenderedComponent {
    Text(Text),
    HelpText(Box<dyn Fn(bool, bool) -> Text>),
    BulletinList(BulletinList),
    Paragraph(Vec<ComponentLine>),
}

impl std::fmt::Debug for RenderedComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderedComponent::Text(text) => write!(f, "{:?}", text),
            RenderedComponent::HelpText(_) => write!(f, "HelpText"),
            RenderedComponent::BulletinList(bulletinlist) => write!(f, "{:?}", bulletinlist),
            RenderedComponent::Paragraph(component_list) => write!(f, "{:?}", component_list),
        }
    }
}

impl RenderedComponent {
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize, hovering_over_link: bool, menu_item_is_selected: bool) -> usize {
        let mut rendered_rows = 0;
        match self {
            RenderedComponent::Text(text) => {
                rendered_rows += 1;
                print_text_with_coordinates(text.clone(), x, y, Some(columns), Some(rows));
            }
            RenderedComponent::HelpText(text) => {
                rendered_rows += 1;
                print_text_with_coordinates(text(hovering_over_link, menu_item_is_selected), 0, y, Some(columns), Some(rows));
            }
            RenderedComponent::BulletinList(bulletin_list) => {
                rendered_rows += bulletin_list.len();
                bulletin_list.render(x, y, rows, columns);
            }
            RenderedComponent::Paragraph(paragraph) => {
                let mut paragraph_rendered_rows = 0;
                for component_line in paragraph {
                    component_line.render(x, y + paragraph_rendered_rows, rows.saturating_sub(paragraph_rendered_rows), columns);
                    rendered_rows += 1;
                    paragraph_rendered_rows += 1;
                }
            }
        }
        rendered_rows
    }
}

#[derive(Debug)]
struct BulletinList {
    title: Text,
    items: Vec<ActiveComponent>,

}

impl BulletinList {
    pub fn new(title: Text) -> Self {
        BulletinList {
            title,
            items: vec![]
        }
    }
    pub fn with_items(mut self, items: Vec<ActiveComponent>) -> Self {
        self.items = items;
        self
    }
    pub fn len(&self) -> usize {
        self.items.len() + 1 // 1 for the title
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for item in &self.items {
            column_count = std::cmp::max(column_count, item.column_count());
        }
        column_count
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for component in &mut self.items {
            let page_to_render = component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render
            }
        }
        None
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        for component in &mut self.items {
            let page_to_render = component.handle_selection();
            if page_to_render.is_some() {
                return page_to_render
            }
        }
        None
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        for component in &mut self.items {
            let should_render = component.handle_hover_at_position(x, y);
            if should_render {
                return should_render;
            }
        }
        false
    }
    pub fn clear_hover(&mut self) {
        for component in &mut self.items {
            component.clear_hover();
        }
    }
    pub fn active_component_position(&self) -> Option<usize> {
        self.items.iter().position(|i| i.is_active)
    }
    pub fn clear_active_bulletins(&mut self) {
        self.items.iter_mut().for_each(|i| {
            i.is_active = false;
        });
    }
    pub fn set_active_bulletin(&mut self, new_index: usize) {
        self.items.get_mut(new_index).map(|i| {
            i.is_active = true;
        });
    }
    pub fn set_last_active_bulletin(&mut self) {
        self.items.last_mut().map(|i| {
            i.is_active = true;
        });
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        print_text_with_coordinates(self.title.clone(), x, y, Some(columns), Some(rows));
        let mut item_bulletin = 1;
        let mut running_y = y + 1;
        for item in &mut self.items {
            let mut item_bulletin_text = Text::new(format!("{}. ", item_bulletin));
            if item.is_active {
                item_bulletin_text = item_bulletin_text.selected();
            }
            let item_bulletin_text_len = item_bulletin_text.len();
            print_text_with_coordinates(item_bulletin_text, x, running_y, Some(item_bulletin_text_len), Some(rows));
            item.render(x + item_bulletin_text_len, running_y, rows, columns.saturating_sub(item_bulletin_text_len));
            running_y += 1;
            item_bulletin += 1;
        }
    }
}

#[derive(Debug)]
struct ComponentLine {
    components: Vec<ActiveComponent>
}

impl ComponentLine {
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        for active_component in &mut self.components {
            let page_to_render = active_component.handle_left_click_at_position(x, y);
            if page_to_render.is_some() {
                return page_to_render;
            }
        }
        None
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        for active_component in &mut self.components {
            let should_render = active_component.handle_hover_at_position(x, y);
            if should_render {
                return should_render;
            }
        }
        false
    }
    pub fn clear_hover(&mut self) {
        for active_component in &mut self.components {
            active_component.clear_hover();
        }
    }
    pub fn column_count(&self) -> usize {
        let mut column_count = 0;
        for active_component in &self.components {
            column_count += active_component.column_count()
        }
        column_count
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) {
        let mut current_x = x;
        let mut columns_left = columns;
        for component in &mut self.components {
            let component_len = component.render(current_x, y, rows, columns_left);
            current_x += component_len;
            columns_left = columns_left.saturating_sub(component_len);
        }
    }
}

impl ComponentLine {
    pub fn new(components: Vec<ActiveComponent>) -> Self {
        ComponentLine {
            components
        }
    }
}
