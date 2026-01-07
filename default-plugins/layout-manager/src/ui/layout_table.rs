use zellij_tile::prelude::*;
use crate::DisplayLayout;
use super::text_utils::{get_layout_display_info, get_last_modified_string, truncate_with_ellipsis};

fn create_table_row(name: &str, metadata: Option<&LayoutMetadata>, is_error: bool, is_builtin: bool, max_name_width: Option<usize>, matched_indices: Option<&Vec<usize>>) -> Vec<Text> {
    let display_name = if let Some(max_width) = max_name_width {
        truncate_with_ellipsis(name, max_width)
    } else {
        name.to_string()
    };

    let name_text = if let Some(indices) = matched_indices {
        // Create text with highlighted matched characters
        let mut text = Text::new(&display_name);

        // Apply base color to entire text
        text = if is_error {
            text.error_color_all()
        } else {
            text.color_all(1)
        };

        text = text.color_indices(3, indices.iter().cloned().collect());

        text
    } else {
        // No matches, use original coloring
        if is_error {
            Text::new(&display_name).error_color_all()
        } else {
            Text::new(&display_name).color_all(1)
        }
    };

    let last_modified_string = get_last_modified_string(metadata, is_builtin);
    let last_modified = if is_builtin {
        Text::new(&last_modified_string).color_all(0)
    } else {
        Text::new(&last_modified_string)
    };

    vec![name_text, last_modified]
}

fn apply_selection(mut row: Vec<Text>) -> Vec<Text> {
    for cell in &mut row {
        *cell = std::mem::take(cell).selected();
    }
    row
}

pub struct LayoutsTable {
    display_layouts: Vec<DisplayLayout>,
    selected_layout_index: usize,
    hidden_above: usize,
    hidden_below: usize,
    matched_indices: Vec<Option<Vec<usize>>>,
}

impl LayoutsTable {
    pub fn new(display_layouts: Vec<DisplayLayout>, selected_layout_index: usize, hidden_above: usize, hidden_below: usize) -> Self {
        Self {
            display_layouts,
            selected_layout_index,
            hidden_above,
            hidden_below,
            matched_indices: Vec::new(),
        }
    }

    pub fn with_matched_indices(mut self, matched_indices: Vec<Option<Vec<usize>>>) -> Self {
        self.matched_indices = matched_indices;
        self
    }

    pub fn render(&self, x: usize, y: usize, _max_rows: usize, max_cols: usize) {
        let mut table = match self.overflow_indicator(self.hidden_above) {
            Some(overflow_indicator) => {
                Table::new().add_styled_row(vec![Text::new(" "), overflow_indicator])
            }
            None => {
                Table::new().add_row(vec![" ", " "])
            }
        };

        // Calculate the actual width needed for the "last modified" column
        let max_last_modified_width = self.display_layouts.iter()
            .map(|layout| {
                let (_, metadata_opt) = get_layout_display_info(layout);
                get_last_modified_string(metadata_opt, layout.is_builtin()).len()
            })
            .max()
            .unwrap_or(1);

        let table_padding = 2;
        let max_name_width = if max_cols > max_last_modified_width + table_padding {
            Some(max_cols.saturating_sub(max_last_modified_width + table_padding))
        } else {
            Some(max_cols.saturating_div(2))
        };

        for (i, layout) in self.display_layouts.iter().enumerate() {
            let (name, metadata_opt) = get_layout_display_info(layout);
            let matched_indices = self.matched_indices.get(i).and_then(|opt| opt.as_ref());
            let mut row = create_table_row(&name, metadata_opt, layout.is_error(), layout.is_builtin(), max_name_width, matched_indices);

            if i == self.selected_layout_index {
                row = apply_selection(row);
            }

            table = table.add_styled_row(row);
        }

        if let Some(overflow_indicator) = self.overflow_indicator(self.hidden_below) {
            table = table.add_styled_row(vec![Text::new(" "), overflow_indicator]);
        }

        print_table_with_coordinates(table, x, y, Some(max_cols), None);
    }
    fn overflow_indicator(&self, count: usize) -> Option<Text> {
        if count > 0 {
            Some(Text::new(format!("[+{}]", count)).color_all(2))
        } else {
            None
        }
    }
}

fn color_control_text(text: &str, keys: &[&str]) -> Text {
    let mut colored_text = Text::new(text);
    for key in keys {
        colored_text = colored_text.color_substring(3, key)
    }
    colored_text
}

pub struct Controls {
    retain_terminal_panes: bool,
    retain_plugin_panes: bool,
    apply_only_to_active_tab: bool,
    show_more_options: bool,
    typing_filter: bool,
    filter_active: bool,
}

impl Controls {
    pub fn new(
        retain_terminal_panes: bool,
        retain_plugin_panes: bool,
        apply_only_to_active_tab: bool,
        show_more_options: bool,
        typing_filter: bool,
        filter_active: bool,
    ) -> Self {
        Self {
            retain_terminal_panes,
            retain_plugin_panes,
            apply_only_to_active_tab,
            show_more_options,
            typing_filter,
            filter_active,
        }
    }

    fn get_typing_filter_controls(&self, max_cols: usize) -> (&str, &[&str]) {
        let long_text = "- <Enter> - accept filter";
        let short_text = "<Enter> - accept filter";
        let minimum_text = "<Enter> ...";
        let text = if max_cols >= long_text.chars().count() {
            long_text
        } else if max_cols >= short_text.chars().count() {
            short_text
        } else {
            minimum_text
        };
        (text, &["<Enter>"])
    }

    fn get_filter_active_controls(&self, max_cols: usize) -> (&str, &[&str]) {
        let long_text = "- <Enter> Open, <↓↑> Nav, <e> Edit, <r> Rename, <Del> - Delete";
        let short_text = "<Enter>/<↓↑>/<e>/<r>/<Del> Open/Nav/Edit/Rename/Del";
        let minimum_text = "<Enter>/<↓↑>/<e>/<r>/<Del> ...";
        let text = if max_cols >= long_text.chars().count() {
            long_text
        } else if max_cols >= short_text.chars().count() {
            short_text
        } else {
            minimum_text
        };
        (text, &["<Enter>", "<↓↑>", "<e>", "<r>", "<Del>"])
    }

    fn get_default_controls(&self, max_cols: usize) -> (&str, &[&str]) {
        let long_text = "- <Enter> Open, <↓↑> Nav, </> Filter, <e> Edit, <r> Rename, <Del> - Del";
        let short_text = "<Enter>/<↓↑>/</>/<e>/<r>/<Del> Open/Nav/Filter/Edit/Rename/Del";
        let minimum_text = "<Enter>/<↓↑>/</>/<e>/<r>/<Del> ...";
        let text = if max_cols >= long_text.chars().count() {
            long_text
        } else if max_cols >= short_text.chars().count() {
            short_text
        } else {
            minimum_text
        };
        (text, &["<Enter>", "<↓↑>", "</>", "<e>", "<r>", "<Del>"])
    }

    fn get_basic_controls_text_and_keys(&self, max_cols: usize) -> (&str, &[&str]) {
        if self.filter_active {
            self.get_filter_active_controls(max_cols)
        } else {
            self.get_default_controls(max_cols)
        }
    }

    fn get_override_text_and_keys(&self, max_cols: usize) -> (String, &[&str]) {
        let toggle_word = if self.show_more_options { "less" } else { "more" };
        let long_text = format!("- <Tab> Override Session Layout, <?> {} options", toggle_word);
        let short_text = format!("<Tab> Override, <?> {} options", toggle_word);
        let minimum_text = format!("<Tab>/<?> ...");
        let text = if max_cols >= long_text.chars().count() {
            long_text
        } else if max_cols >= short_text.chars().count() {
            short_text
        } else {
            minimum_text
        };
        (
            text,
            &["<Tab>", "<?>"]
        )
    }

    fn get_new_layout_text_and_keys(&self, max_cols: usize) -> (&str, &[&str]) {
        let long_text = "- New Layout: <n> from current session, <i> import";
        let short_text = "New: <n> current, <i> import";
        let minimum_text = "New: <n>/<N>/<i> ...";
        let text = if max_cols >= long_text.chars().count() {
            long_text
        } else if max_cols >= short_text.chars().count() {
            short_text
        } else {
            minimum_text
        };
        (
            text,
            &["<n>", "<i>"]
        )
    }

    pub fn calculate_width(&self, max_cols: usize) -> usize {
        let mut width = 0;

        // Line 1: Basic controls
        let (basic_text, _) = self.get_default_controls(max_cols);
        width = width.max(basic_text.chars().count());

        // Line 2: Override and toggle
        let (override_text, _) = self.get_override_text_and_keys(max_cols);
        width = width.max(override_text.chars().count());

        // Line 3 (or 5 if show_more_options): New layout controls
        let (new_layout_text, _) = self.get_new_layout_text_and_keys(max_cols);
        width = width.max(new_layout_text.chars().count());

        // Lines 4-5: Advanced options (only if show_more_options)
        if self.show_more_options {
            let (retain_text, _) = self.get_retain_text_and_highlight(max_cols);
            width = width.max(retain_text.chars().count());

            let (target_text, _) = self.get_target_text_and_highlight(max_cols);
            width = width.max(target_text.chars().count());
        }

        width
    }

    pub fn render(&self, x: usize, y: usize, max_cols: usize) {
        if self.typing_filter {
            self.render_typing_filter_controls(x, y, max_cols);
        } else {
            self.render_basic_controls(x, y, max_cols);
            self.render_override_and_toggle(x, y + 1, max_cols);

            let new_layout_y = if self.show_more_options { y + 4 } else { y + 2 };
            self.render_new_layout_controls(x, new_layout_y, max_cols);

            if self.show_more_options {
                self.render_advanced_options(x, y + 2, max_cols);
            }
        }
    }

    fn render_basic_controls(&self, x: usize, y: usize, max_width: usize) {
        let (text, keys) = self.get_basic_controls_text_and_keys(max_width);
        let controls_line = color_control_text(text, keys);
        print_text_with_coordinates(controls_line, x, y, None, None);
    }

    fn render_override_and_toggle(&self, x: usize, y: usize, max_cols: usize) {
        let (text, keys) = self.get_override_text_and_keys(max_cols);
        let override_line = color_control_text(&text, keys);
        print_text_with_coordinates(override_line, x, y, None, None);
    }

    fn render_new_layout_controls(&self, x: usize, y: usize, max_cols: usize) {
        let (text, keys) = self.get_new_layout_text_and_keys(max_cols);
        let new_layout_line = color_control_text(text, keys).color_substring(2, "New Layout:");
        print_text_with_coordinates(new_layout_line, x, y, None, None);
    }

    fn render_typing_filter_controls(&self, x: usize, y: usize, max_cols: usize) {
        let (text, keys) = self.get_typing_filter_controls(max_cols);
        let controls_line = color_control_text(text, keys);
        print_text_with_coordinates(controls_line, x, y, None, None);
    }

    fn render_advanced_options(&self, x: usize, y: usize, max_cols: usize) {
        self.render_retain_option(x, y, max_cols);
        self.render_target_option(x, y + 1, max_cols);
    }

    fn render_retain_option(&self, x: usize, y: usize, max_cols: usize) {
        let (retain_text, retain_substring) = self.get_retain_text_and_highlight(max_cols);
        let retain_line = color_control_text(retain_text, &["<t>"])
            .color_substring(0, retain_substring);
        print_text_with_coordinates(retain_line, x, y, None, None);
    }

    fn get_retain_text_and_highlight(&self, max_cols: usize) -> (&str, &str) {
        let long_text = "  <t> Retain:  Terminals  |  Plugins  |  Both  |  None ";
        let short_text = "  <t> Retain:  Term | Pl | Both | None ";
        if max_cols >= long_text.chars().count() {
            match (self.retain_terminal_panes, self.retain_plugin_panes) {
                (true, false) =>  ("  <t> Retain: [Terminals] |  Plugins  |  Both  |  None", "[Terminals]"),
                (false, true) =>  ("  <t> Retain:  Terminals  | [Plugins] |  Both  |  None", "[Plugins]"),
                (true, true) =>   ("  <t> Retain:  Terminals  |  Plugins  | [Both] |  None", "[Both]"),
                (false, false) => ("  <t> Retain:  Terminals  |  Plugins  |  Both  | [None]", "[None]"),
            }
        } else if max_cols >= short_text.chars().count() {
            match (self.retain_terminal_panes, self.retain_plugin_panes) {
                (true, false) =>  ("  <t> Retain: [Term]| Pl | Both | None", "[Term]"),
                (false, true) =>  ("  <t> Retain:  Term |[Pl]| Both | None", "[Pl]"),
                (true, true) =>   ("  <t> Retain:  Term | Pl |[Both]| None", "[Both]"),
                (false, false) => ("  <t> Retain:  Term | Pl | Both |[None]", "[None]"),
            }
        } else {
            match (self.retain_terminal_panes, self.retain_plugin_panes) {
                (true, false) =>  ("  <t> R: [T] | P | B | N ...", "[T]"),
                (false, true) =>  ("  <t> R:  T  |[P]| B | N ...", "[P]"),
                (true, true) =>   ("  <t> R:  T  | P |[B]| N ...", "[B]"),
                (false, false) => ("  <t> R:  T  | P | B |[N]...", "[N]"),
            }
        }
    }

    fn render_target_option(&self, x: usize, y: usize, max_cols: usize) {
        let (target_text, target_substring) = self.get_target_text_and_highlight(max_cols);
        let target_line = color_control_text(target_text, &["<a>"])
            .color_substring(2, target_substring);
        print_text_with_coordinates(target_line, x, y, None, None);
    }

    fn get_target_text_and_highlight(&self, max_cols: usize) -> (&str, &str) {
        if self.apply_only_to_active_tab {
            let long_text =  "  <a> Target:  All Tabs   | [Current]";
            let short_text = "  <a> Target:  All |[Current]";
            if max_cols >= long_text.chars().count() {
                (long_text, "[Current]")
            } else {
                (short_text, "[Current]")
            }
        } else {
            let long_text =  "  <a> Target: [All Tabs]  |  Current ";
            let short_text = "  <a> Target: [All]| Current ";
            if max_cols >= long_text.chars().count() {
                (long_text, "[All Tabs]")
            } else {
                (short_text, "[All]")
            }
        }
    }
}

pub struct Title<'a> {
    text: &'a str,
}

impl<'a> Title<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }

    pub fn render(&self, x: usize, y: usize) {
        let title = Text::new(self.text).color_all(2);
        print_text_with_coordinates(title, x, y, None, None);
    }
}

pub struct ErrorMessage<'a> {
    message: &'a str,
}

impl<'a> ErrorMessage<'a> {
    pub fn new(message: &'a str) -> Self {
        Self { message }
    }

    pub fn render(&self, x: usize, y: usize) {
        let title = Text::new("Error").error_color_all();
        print_text_with_coordinates(title, x, y, None, None);

        let message = Text::new(self.message).error_color_all();
        print_text_with_coordinates(message, x, y + 2, None, None);

        let help = Text::new("Press any key to continue");
        print_text_with_coordinates(help, x, y + 4, None, None);
    }
}
