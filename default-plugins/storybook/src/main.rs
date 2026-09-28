mod frame;
mod stories;

use frame::Frame;
use std::collections::BTreeMap;
use stories::{all_stories, Story};
use zellij_tile::prelude::*;

const SIDE_WIDTH: usize = 24;
const MIN_ROWS: usize = 16;
const MIN_COLS: usize = 70;
const KEY_HINTS: &str = "Tab/S-Tab focus · Esc menu · PgUp/PgDn scroll · q quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Menu,
    Story,
}

struct Storybook {
    stories: Vec<Box<dyn Story>>,
    menu: SideMenu,
    scroll: ScrollView,
    focus: Focus,
    status: String,
    follow_focus: bool,
}

impl Default for Storybook {
    fn default() -> Self {
        let stories = all_stories();
        let titles: Vec<&'static str> = stories.iter().map(|s| s.title()).collect();
        Storybook {
            stories,
            menu: SideMenu::new(titles).focused(),
            scroll: ScrollView::new(0),
            focus: Focus::Menu,
            status: String::from("Pick a story with Up/Down, press Enter or Tab to try it"),
            follow_focus: false,
        }
    }
}

register_plugin!(Storybook);

impl Storybook {
    fn story(&mut self) -> &mut Box<dyn Story> {
        let index = self.menu.selected_index();
        &mut self.stories[index]
    }
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.menu.set_focused(focus == Focus::Menu);
        if focus == Focus::Menu {
            self.story().blur();
        }
    }
    fn enter_story(&mut self) {
        self.story().focus_first();
        self.set_focus(Focus::Story);
        self.follow_focus = true;
    }
    fn switch_story(&mut self, response: UiResponse) {
        for story in self.stories.iter_mut() {
            story.blur();
        }
        self.scroll.set_offset(0);
        self.status = format!("Side menu → {}", response);
    }
    fn handle_menu_key(&mut self, key: &KeyWithModifier) -> bool {
        match self.menu.handle_key(key) {
            response @ UiResponse::Changed(_) => {
                self.switch_story(response);
                return true;
            },
            UiResponse::Submitted(_) => {
                self.enter_story();
                return true;
            },
            _ => {},
        }
        if key.is_key_without_modifier(BareKey::Tab) || key.is_key_without_modifier(BareKey::Right)
        {
            self.enter_story();
            return true;
        }
        self.handle_global_key(key)
    }
    fn handle_story_key(&mut self, key: &KeyWithModifier) -> bool {
        if let Some(status) = self.story().handle_key(key) {
            self.status = status;
            self.follow_focus = true;
            return true;
        }
        if key.is_key_without_modifier(BareKey::Esc) {
            self.set_focus(Focus::Menu);
            self.status = String::from("Back to the side menu");
            return true;
        }
        self.handle_global_key(key)
    }
    fn handle_global_key(&mut self, key: &KeyWithModifier) -> bool {
        if key.is_key_without_modifier(BareKey::Char('q')) {
            close_self();
            return false;
        }
        let response = self.scroll.handle_key(key);
        response.is_handled()
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        match mouse {
            Mouse::Hover(..) => {
                self.menu.handle_mouse(mouse);
                self.scroll.handle_mouse(mouse);
                self.story().handle_mouse(mouse);
                return true;
            },
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) => {
                if let Some(status) = self.story().handle_mouse(mouse) {
                    self.status = status;
                    return true;
                }
                if self.menu.handle_mouse(mouse).is_handled() {
                    return true;
                }
                return self.scroll.handle_mouse(mouse).is_handled();
            },
            Mouse::LeftClick(line, column) if self.menu.hit_test(line, column) => {
                if let response @ UiResponse::Changed(_) = self.menu.handle_mouse(mouse) {
                    self.switch_story(response);
                }
                self.set_focus(Focus::Menu);
                return true;
            },
            _ => {},
        }
        if let Some(status) = self.story().handle_mouse(mouse) {
            if matches!(mouse, Mouse::LeftClick(..)) {
                self.status = status;
                self.set_focus(Focus::Story);
            }
            return true;
        }
        match mouse {
            Mouse::LeftClick(..) => self.scroll.handle_mouse(mouse).is_handled(),
            Mouse::Release(..) => true,
            _ => false,
        }
    }
    fn render_too_small(&self, rows: usize, cols: usize) {
        let message = format!(
            "Pane too small ({}x{}). The storybook needs at least {}x{}.",
            cols, rows, MIN_COLS, MIN_ROWS
        );
        print_text_with_coordinates(
            Text::new(message).error_color_range(0..14),
            0,
            rows / 2,
            Some(cols),
            None,
        );
    }
}

impl ZellijPlugin for Storybook {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[EventType::Key, EventType::Mouse, EventType::Timer]);
    }
    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) => {
                if self.story().is_modal() {
                    if let Some(status) = self.story().handle_key(&key) {
                        self.status = status;
                    }
                    return true;
                }
                match self.focus {
                    Focus::Menu => self.handle_menu_key(&key),
                    Focus::Story => self.handle_story_key(&key),
                }
            },
            Event::Mouse(mouse) => {
                if self.story().is_modal() {
                    if let Some(status) = self.story().handle_mouse(mouse) {
                        if matches!(mouse, Mouse::LeftClick(..)) {
                            self.status = status;
                        }
                    }
                    return true;
                }
                self.handle_mouse(mouse)
            },
            Event::Timer(_) => self
                .stories
                .iter_mut()
                .fold(false, |changed, story| story.handle_timer() || changed),
            _ => false,
        }
    }
    fn render(&mut self, rows: usize, cols: usize) {
        if rows < MIN_ROWS || cols < MIN_COLS {
            self.render_too_small(rows, cols);
            return;
        }
        let body_height = rows - 1;
        self.menu.render(0, 0, SIDE_WIDTH, body_height);
        let content_x = SIDE_WIDTH + 1;
        let content_width = cols - content_x;
        let index = self.menu.selected_index();
        let content_height = self.stories[index].layout(content_width - 1, body_height);
        self.scroll.set_total_rows(content_height);
        self.scroll.layout(content_x, 0, content_width, body_height);
        if self.scroll.needs_indicators() {
            let narrower = self.scroll.content_width().saturating_sub(1);
            let content_height = self.stories[index].layout(narrower, body_height);
            self.scroll.set_total_rows(content_height);
            self.scroll.layout(content_x, 0, content_width, body_height);
        }
        if self.follow_focus {
            if let Some((row, height)) = self.stories[index].focused_rows() {
                self.scroll.ensure_range_visible(row, height);
            }
            self.follow_focus = false;
        }
        let frame = Frame {
            x: content_x,
            y: self.scroll.content_y(),
            width: self.scroll.content_width().saturating_sub(1),
            height: self.scroll.viewport_height(),
            offset: self.scroll.offset(),
            pane_cols: cols,
        };
        self.stories[index].render(&frame);
        self.scroll.render_indicators();
        self.stories[index].render_overlays(&frame);
        let status = format!("{}   {}", self.status, KEY_HINTS);
        let hints_start = self.status.chars().count() + 3;
        print_text_with_coordinates(
            Text::new(status)
                .color_range(0, 0..self.status.chars().count())
                .dim_range(hints_start..),
            0,
            rows - 1,
            Some(cols),
            None,
        );
    }
}
