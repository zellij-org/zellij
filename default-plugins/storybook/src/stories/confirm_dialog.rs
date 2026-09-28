use super::{sample, Base, Story};
use crate::frame::Frame;
use zellij_tile::prelude::*;

const MESSAGE: &str =
    "You have unsaved changes. If you leave now they will be lost. Do you want to discard them?";

fn two_buttons() -> ConfirmDialog {
    ConfirmDialog::new("Discard changes?", MESSAGE).buttons(vec!["Discard", "Keep editing"])
}

fn three_buttons() -> ConfirmDialog {
    ConfirmDialog::new("Close session", "The session has 3 running commands.")
        .buttons(vec!["Detach", "Kill", "Cancel"])
}

pub struct ConfirmDialogStory {
    base: Base,
    dialog: ConfirmDialog,
    last_opened: &'static str,
}

impl ConfirmDialogStory {
    pub fn new() -> Self {
        let base = Base::new("Confirm dialog")
            .intro(vec![
                "A centred, opaque box with a title, a wrapped message and 2-3 buttons.",
                "Tab or Left/Right move between buttons, Enter or a click chooses, Esc cancels.",
                "While it is open it takes all keys and clicks, and it is drawn last.",
            ])
            .sample(sample("Two buttons", 40, 8, |x, y| {
                two_buttons().width(40).serialize(x, y, 40, 8)
            }))
            .sample(sample("Three buttons", 40, 6, |x, y| {
                three_buttons().selected(2).width(40).serialize(x, y, 40, 6)
            }))
            .live(
                "Open 2-button dialog",
                Button::new("Open 2-button dialog"),
                26,
                1,
            )
            .live(
                "Open 3-button dialog",
                Button::new("Open 3-button dialog"),
                26,
                1,
            );
        ConfirmDialogStory {
            base,
            dialog: two_buttons(),
            last_opened: "Dialog",
        }
    }
    fn open_from(&mut self, status: &Option<String>) {
        match status.as_deref() {
            Some("Open 2-button dialog → Activated") => {
                self.dialog = two_buttons();
                self.dialog.open();
                self.last_opened = "2-button dialog";
            },
            Some("Open 3-button dialog → Activated") => {
                self.dialog = three_buttons();
                self.dialog.open();
                self.last_opened = "3-button dialog";
            },
            _ => {},
        }
    }
    fn dialog_status(&self, response: UiResponse) -> Option<String> {
        if response.is_handled() {
            Some(format!("{} → {}", self.last_opened, response))
        } else {
            None
        }
    }
}

impl Story for ConfirmDialogStory {
    fn title(&self) -> &'static str {
        self.base.title
    }
    fn layout(&mut self, width: usize, height: usize) -> usize {
        self.base.layout(width, height)
    }
    fn render(&mut self, frame: &Frame) {
        self.base.render(frame);
    }
    fn render_overlays(&mut self, frame: &Frame) {
        self.base.render_overlays(frame);
        self.dialog.render_centered(frame.bottom(), frame.pane_cols);
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        if self.dialog.is_open() {
            let response = self.dialog.handle_key(key);
            return self.dialog_status(response);
        }
        let status = self.base.handle_key(key);
        self.open_from(&status);
        status
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        if self.dialog.is_open() {
            let response = self.dialog.handle_mouse(mouse);
            return match mouse {
                Mouse::Hover(..) => None,
                _ => self.dialog_status(response),
            };
        }
        let status = self.base.handle_mouse(mouse);
        self.open_from(&status);
        status
    }
    fn focus_first(&mut self) {
        self.base.focus_first();
    }
    fn blur(&mut self) {
        self.base.blur();
    }
    fn focused_rows(&self) -> Option<(usize, usize)> {
        self.base.focused_rows()
    }
    fn handle_timer(&mut self) -> bool {
        self.base.group.handle_timer()
    }
    fn is_modal(&self) -> bool {
        self.dialog.is_open()
    }
}
