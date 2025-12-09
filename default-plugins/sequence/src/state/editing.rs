use crate::path_formatting;
use crate::state::CommandEntry;
use crate::ui::text_input::TextInput;
use std::path::PathBuf;

pub struct Editing {
    pub editing_input: Option<TextInput>,
}

impl Editing {
    pub fn new() -> Self {
        Self {
            editing_input: Some(TextInput::new("".to_owned())),
        }
    }

    pub fn start_editing(&mut self, text: String) {
        self.editing_input = Some(TextInput::new(text));
    }

    pub fn cancel_editing(&mut self) {
        self.editing_input = None;
    }

    pub fn input_text(&self) -> Option<String> {
        self.editing_input.as_ref().map(|e| e.get_text().to_owned())
    }

    pub fn set_input_text(&mut self, text: String) {
        self.editing_input.as_mut().map(|e| e.set_text(text));
    }

    pub fn handle_submit(
        &mut self,
        selection_index: Option<usize>,
        all_commands: &mut Vec<CommandEntry>,
        current_cwd: &Option<PathBuf>,
    ) -> (bool, Option<usize>) {
        let mut handled_internally = false;
        let mut new_selection_index = None;

        if let Some(current_text) = self.editing_input.as_ref().map(|i| i.get_text()) {
            if let Some(index) = selection_index {
                if let Some(command) = all_commands.get_mut(index) {
                    if current_text.starts_with("cd ") || current_text == "cd" {
                        let path = if current_text == "cd" {
                            "~"
                        } else {
                            current_text[3..].trim()
                        };

                        if let Some(new_cwd) =
                            path_formatting::resolve_path(current_cwd.as_ref(), path)
                        {
                            command.set_cwd(Some(new_cwd));
                        }

                        command.set_text("".to_owned());
                        self.editing_input.as_mut().map(|c| c.clear());
                        new_selection_index = selection_index; // remain editing after a cd command
                        handled_internally = true;
                        return (handled_internally, new_selection_index); // avoid setting edit_input to None below, we
                                                                          // still want to be in editing mode after
                                                                          // changing directory with cd
                    } else {
                        command.set_text(current_text.to_owned());
                    }
                }
            }
        }
        self.editing_input = None;
        (handled_internally, new_selection_index)
    }
}

impl Default for Editing {
    fn default() -> Self {
        Self::new()
    }
}
