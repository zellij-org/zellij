use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::ui::{PaneUiInfo, SessionUiInfo, TabUiInfo};
use crate::{ActiveScreen, NewSessionInfo};

#[derive(Debug)]
pub struct ListItem {
    pub name: String,
    pub session_name: Option<Vec<UiSpan>>,
    pub tab_name: Option<Vec<UiSpan>>,
    pub pane_name: Option<Vec<UiSpan>>,
    colors: Colors,
}

impl ListItem {
    pub fn from_session_info(session_ui_info: &SessionUiInfo, colors: Colors) -> Self {
        let session_ui_line = build_session_ui_line(session_ui_info, colors);
        ListItem {
            name: session_ui_info.name.clone(),
            session_name: Some(session_ui_line),
            tab_name: None,
            pane_name: None,
            colors,
        }
    }
    pub fn from_tab_info(
        session_ui_info: &SessionUiInfo,
        tab_ui_info: &TabUiInfo,
        colors: Colors,
    ) -> Self {
        let session_ui_line = build_session_ui_line(session_ui_info, colors);
        let tab_ui_line = build_tab_ui_line(tab_ui_info, colors);
        ListItem {
            name: tab_ui_info.name.clone(),
            session_name: Some(session_ui_line),
            tab_name: Some(tab_ui_line),
            pane_name: None,
            colors,
        }
    }
    pub fn from_pane_info(
        session_ui_info: &SessionUiInfo,
        tab_ui_info: &TabUiInfo,
        pane_ui_info: &PaneUiInfo,
        colors: Colors,
    ) -> Self {
        let session_ui_line = build_session_ui_line(session_ui_info, colors);
        let tab_ui_line = build_tab_ui_line(tab_ui_info, colors);
        let pane_ui_line = build_pane_ui_line(pane_ui_info, colors);
        ListItem {
            name: pane_ui_info.name.clone(),
            session_name: Some(session_ui_line),
            tab_name: Some(tab_ui_line),
            pane_name: Some(pane_ui_line),
            colors,
        }
    }
    pub fn line_count(&self) -> usize {
        let mut line_count = 0;
        if self.session_name.is_some() {
            line_count += 1
        };
        if self.tab_name.is_some() {
            line_count += 1
        };
        if self.pane_name.is_some() {
            line_count += 1
        };
        line_count
    }
    pub fn render(&self, indices: Option<Vec<usize>>, max_cols: usize) -> Vec<LineToRender> {
        let mut lines_to_render = vec![];
        if let Some(session_name) = &self.session_name {
            let indices = if self.tab_name.is_none() && self.pane_name.is_none() {
                indices.clone()
            } else {
                None
            };
            let mut line_to_render = LineToRender::new(self.colors);
            let mut remaining_cols = max_cols;
            for span in session_name {
                span.render(
                    indices
                        .clone()
                        .map(|i| (SpanStyle::ForegroundBold(self.colors.palette.magenta), i)),
                    &mut line_to_render,
                    &mut remaining_cols,
                );
            }
            lines_to_render.push(line_to_render);
        }
        if let Some(tab_name) = &self.tab_name {
            let indices = if self.pane_name.is_none() {
                indices.clone()
            } else {
                None
            };
            let mut line_to_render = LineToRender::new(self.colors);
            let mut remaining_cols = max_cols;
            for span in tab_name {
                span.render(
                    indices
                        .clone()
                        .map(|i| (SpanStyle::ForegroundBold(self.colors.palette.magenta), i)),
                    &mut line_to_render,
                    &mut remaining_cols,
                );
            }
            lines_to_render.push(line_to_render);
        }
        if let Some(pane_name) = &self.pane_name {
            let mut line_to_render = LineToRender::new(self.colors);
            let mut remaining_cols = max_cols;
            for span in pane_name {
                span.render(
                    indices
                        .clone()
                        .map(|i| (SpanStyle::ForegroundBold(self.colors.palette.magenta), i)),
                    &mut line_to_render,
                    &mut remaining_cols,
                );
            }
            lines_to_render.push(line_to_render);
        }
        lines_to_render
    }
}

#[derive(Debug)]
pub enum UiSpan {
    UiSpanTelescope(UiSpanTelescope),
    TruncatableUiSpan(TruncatableUiSpan),
}

impl UiSpan {
    pub fn render(
        &self,
        indices: Option<(SpanStyle, Vec<usize>)>,
        line_to_render: &mut LineToRender,
        remaining_cols: &mut usize,
    ) {
        match self {
            UiSpan::UiSpanTelescope(ui_span_telescope) => {
                ui_span_telescope.render(line_to_render, remaining_cols)
            },
            UiSpan::TruncatableUiSpan(truncatable_ui_span) => {
                truncatable_ui_span.render(indices, line_to_render, remaining_cols)
            },
        }
    }
}

#[allow(dead_code)] // in the future this will be moved to be its own component
#[derive(Debug)]
pub enum SpanStyle {
    None,
    Bold,
    Foreground(PaletteColor),
    ForegroundBold(PaletteColor),
}

impl SpanStyle {
    pub fn style_string(&self, to_style: &str) -> String {
        match self {
            SpanStyle::None => to_style.to_owned(),
            SpanStyle::Bold => format!("\u{1b}[1m{}\u{1b}[22m", to_style),
            SpanStyle::Foreground(color) => match color {
                PaletteColor::EightBit(byte) => {
                    format!("\u{1b}[38;5;{byte}m{}\u{1b}[39m", to_style)
                },
                PaletteColor::Rgb((r, g, b)) => {
                    format!("\u{1b}[38;2;{};{};{}m{}\u{1b}[39m", r, g, b, to_style)
                },
            },
            SpanStyle::ForegroundBold(color) => match color {
                PaletteColor::EightBit(byte) => {
                    format!("\u{1b}[38;5;{byte};1m{}\u{1b}[39;22m", to_style)
                },
                PaletteColor::Rgb((r, g, b)) => {
                    format!("\u{1b}[38;2;{};{};{};1m{}\u{1b}[39;22m", r, g, b, to_style)
                },
            },
        }
    }
}

impl Default for SpanStyle {
    fn default() -> Self {
        SpanStyle::None
    }
}

#[derive(Debug, Default)]
pub struct TruncatableUiSpan {
    text: String,
    style: SpanStyle,
}

impl TruncatableUiSpan {
    pub fn new(text: String, style: SpanStyle) -> Self {
        TruncatableUiSpan { text, style }
    }
    pub fn render(
        &self,
        indices: Option<(SpanStyle, Vec<usize>)>,
        line_to_render: &mut LineToRender,
        remaining_cols: &mut usize,
    ) {
        let mut rendered = String::new();
        let truncated = if *remaining_cols >= self.text.width() {
            self.text.clone()
        } else {
            let mut truncated = String::new();
            for character in self.text.chars() {
                if truncated.width() + character.width().unwrap_or(0) <= *remaining_cols {
                    truncated.push(character);
                } else {
                    break;
                }
            }
            truncated
        };
        match indices {
            Some((index_style, indices)) => {
                for (i, character) in truncated.chars().enumerate() {
                    // TODO: optimize this by splitting the string up by its indices and only pushing those
                    // chu8nks
                    if indices.contains(&i) {
                        rendered.push_str(&index_style.style_string(&character.to_string()));
                    } else {
                        rendered.push_str(&self.style.style_string(&character.to_string()));
                    }
                }
            },
            None => {
                rendered.push_str(&self.style.style_string(&truncated));
            },
        }
        *remaining_cols = remaining_cols.saturating_sub(truncated.width());
        line_to_render.append(&rendered);
    }
}

#[derive(Debug, Default)]
pub struct UiSpanTelescope(Vec<StringAndLength>);

impl UiSpanTelescope {
    pub fn new(string_and_lengths: Vec<StringAndLength>) -> Self {
        UiSpanTelescope(string_and_lengths)
    }
    pub fn render(&self, line_to_render: &mut LineToRender, remaining_cols: &mut usize) {
        for string_and_length in &self.0 {
            if string_and_length.length < *remaining_cols {
                line_to_render.append(&string_and_length.string);
                *remaining_cols -= string_and_length.length;
                break;
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct StringAndLength {
    pub string: String,
    pub length: usize,
}

impl StringAndLength {
    pub fn new(string: String, length: usize) -> Self {
        StringAndLength { string, length }
    }
}

#[derive(Debug, Clone)]
pub struct LineToRender {
    line: String,
    is_selected: bool,
    truncated_result_count: usize,
    colors: Colors,
}

impl LineToRender {
    pub fn new(colors: Colors) -> Self {
        LineToRender {
            line: String::default(),
            is_selected: false,
            truncated_result_count: 0,
            colors,
        }
    }
    pub fn append(&mut self, to_append: &str) {
        self.line.push_str(to_append)
    }
    pub fn make_selected_as_search(&mut self, add_arrows: bool) {
        self.is_selected = true;
        let arrows = if add_arrows {
            self.colors.magenta(" <↓↑> ")
        } else {
            "      ".to_owned()
        };
        match self.colors.palette.bg {
            PaletteColor::EightBit(byte) => {
                self.line = format!(
                    "\u{1b}[48;5;{byte}m\u{1b}[K\u{1b}[48;5;{byte}m{arrows}{}",
                    self.line
                );
            },
            PaletteColor::Rgb((r, g, b)) => {
                self.line = format!(
                    "\u{1b}[48;2;{};{};{}m\u{1b}[K\u{1b}[48;2;{};{};{}m{arrows}{}",
                    r, g, b, r, g, b, self.line
                );
            },
        }
    }
    pub fn make_selected(&mut self, add_arrows: bool) {
        self.is_selected = true;
        let arrows = if add_arrows {
            self.colors.magenta("<←↓↑→>")
        } else {
            "      ".to_owned()
        };
        match self.colors.palette.bg {
            PaletteColor::EightBit(byte) => {
                self.line = format!(
                    "\u{1b}[48;5;{byte}m\u{1b}[K\u{1b}[48;5;{byte}m{arrows}{}",
                    self.line
                );
            },
            PaletteColor::Rgb((r, g, b)) => {
                self.line = format!(
                    "\u{1b}[48;2;{};{};{}m\u{1b}[K\u{1b}[48;2;{};{};{}m{arrows}{}",
                    r, g, b, r, g, b, self.line
                );
            },
        }
    }
    pub fn render(&self) -> String {
        let mut line = self.line.clone();

        let more = if self.truncated_result_count > 0 {
            self.colors
                .red(&format!(" [+{}]", self.truncated_result_count))
        } else {
            String::new()
        };

        line.push_str(&more);
        if self.is_selected {
            self.line.clone()
        } else {
            format!("\u{1b}[49m      {}", line)
        }
    }
    pub fn add_truncated_results(&mut self, result_count: usize) {
        self.truncated_result_count += result_count;
    }
}

pub fn build_session_ui_line(session_ui_info: &SessionUiInfo, colors: Colors) -> Vec<UiSpan> {
    let mut ui_spans = vec![];
    let tab_count_text = session_ui_info.tabs.len();
    let total_pane_count_text = session_ui_info
        .tabs
        .iter()
        .fold(0, |acc, tab| acc + tab.panes.len());
    let tab_count = format!("{}", tab_count_text);
    let tab_count_styled = colors.cyan(&tab_count);
    let total_pane_count = format!("{}", total_pane_count_text);
    let total_pane_count_styled = colors.green(&total_pane_count);
    let session_name = &session_ui_info.name;
    let connected_users = format!("{}", session_ui_info.connected_users);
    let connected_users_styled = colors.orange(&connected_users);
    let session_bullet_span =
        UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![StringAndLength::new(
            format!(" > "),
            3,
        )]));
    let session_name_span = UiSpan::TruncatableUiSpan(TruncatableUiSpan::new(
        session_name.clone(),
        SpanStyle::ForegroundBold(colors.palette.orange),
    ));
    let tab_and_pane_count = UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![
        StringAndLength::new(
            format!(" ({tab_count_styled} tabs, {total_pane_count_styled} panes)"),
            2 + tab_count.width() + 7 + total_pane_count.width() + 7,
        ),
        StringAndLength::new(
            format!(" ({tab_count_styled}, {total_pane_count_styled})"),
            2 + tab_count.width() + 2 + total_pane_count.width() + 3,
        ),
    ]));
    let connected_users_count = UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![
        StringAndLength::new(
            format!(" [{connected_users_styled} connected users]"),
            2 + connected_users.width() + 17,
        ),
        StringAndLength::new(
            format!(" [{connected_users_styled}]"),
            2 + connected_users.width() + 1,
        ),
    ]));
    ui_spans.push(session_bullet_span);
    ui_spans.push(session_name_span);
    ui_spans.push(tab_and_pane_count);
    ui_spans.push(connected_users_count);
    if session_ui_info.is_current_session {
        let current_session_indication = UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![
            StringAndLength::new(colors.orange(&format!(" <CURRENT SESSION>")), 18),
            StringAndLength::new(colors.orange(&format!(" <CURRENT>")), 10),
            StringAndLength::new(colors.orange(&format!(" <C>")), 4),
        ]));
        ui_spans.push(current_session_indication);
    }
    ui_spans
}

pub fn build_tab_ui_line(tab_ui_info: &TabUiInfo, colors: Colors) -> Vec<UiSpan> {
    let mut ui_spans = vec![];
    let tab_name = &tab_ui_info.name;
    let pane_count_text = tab_ui_info.panes.len();
    let pane_count = format!("{}", pane_count_text);
    let pane_count_styled = colors.green(&pane_count);
    let tab_bullet_span =
        UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![StringAndLength::new(
            format!("  - "),
            4,
        )]));
    let tab_name_span = UiSpan::TruncatableUiSpan(TruncatableUiSpan::new(
        tab_name.clone(),
        SpanStyle::ForegroundBold(colors.palette.cyan),
    ));
    let connected_users_count_span = UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![
        StringAndLength::new(
            format!(" ({pane_count_styled} panes)"),
            2 + pane_count.width() + 7,
        ),
        StringAndLength::new(
            format!(" ({pane_count_styled})"),
            2 + pane_count.width() + 1,
        ),
    ]));
    ui_spans.push(tab_bullet_span);
    ui_spans.push(tab_name_span);
    ui_spans.push(connected_users_count_span);
    ui_spans
}

pub fn build_pane_ui_line(pane_ui_info: &PaneUiInfo, colors: Colors) -> Vec<UiSpan> {
    let mut ui_spans = vec![];
    let pane_name = pane_ui_info.name.clone();
    let exit_code = pane_ui_info.exit_code.map(|exit_code_number| {
        let exit_code = format!("{}", exit_code_number);
        let exit_code = if exit_code_number == 0 {
            colors.green(&exit_code)
        } else {
            colors.red(&exit_code)
        };
        exit_code
    });
    let pane_bullet_span =
        UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![StringAndLength::new(
            format!("    > "),
            6,
        )]));
    ui_spans.push(pane_bullet_span);
    let pane_name_span =
        UiSpan::TruncatableUiSpan(TruncatableUiSpan::new(pane_name, SpanStyle::Bold));
    ui_spans.push(pane_name_span);
    if let Some(exit_code) = exit_code {
        let pane_name_span = UiSpan::UiSpanTelescope(UiSpanTelescope::new(vec![
            StringAndLength::new(
                format!(" (EXIT CODE: {exit_code})"),
                13 + exit_code.width() + 1,
            ),
            StringAndLength::new(format!(" ({exit_code})"), 2 + exit_code.width() + 1),
        ]));
        ui_spans.push(pane_name_span);
    }
    ui_spans
}

pub fn minimize_lines(
    total_count: usize,
    line_count_to_remove: usize,
    selected_index: Option<usize>,
) -> (usize, usize, usize, usize) {
    // returns: (start_index, anchor_index, end_index, lines_left_to_remove)
    let (count_to_render, line_count_to_remove) = if line_count_to_remove > total_count {
        (1, line_count_to_remove.saturating_sub(total_count) + 1)
    } else {
        (total_count.saturating_sub(line_count_to_remove), 0)
    };
    let anchor_index = selected_index.unwrap_or(0); // 5
    let mut start_index = anchor_index.saturating_sub(count_to_render / 2);
    let mut end_index = start_index + count_to_render;
    if end_index > total_count {
        start_index = start_index.saturating_sub(end_index - total_count);
        end_index = total_count;
    }
    (start_index, anchor_index, end_index, line_count_to_remove)
}

pub fn render_prompt(search_term: &str, colors: Colors, x: usize, y: usize) {
    let prompt = colors.green(&format!("Search:"));
    let search_term = colors.bold(&format!("{}_", search_term));
    println!("\u{1b}[{};{}H{} {}\n", y + 1, x, prompt, search_term);
}

pub fn render_screen_toggle(active_screen: ActiveScreen, x: usize, y: usize, max_cols: usize) {
    let key_indication_text = "<TAB>";
    let (new_session_text, running_sessions_text, exited_sessions_text) = if max_cols > 66 {
        ("New Session", "Attach to Session", "Resurrect Session")
    } else {
        ("New", "Attach", "Resurrect")
    };
    let key_indication_len = key_indication_text.chars().count() + 1;
    let first_ribbon_length = new_session_text.chars().count() + 4;
    let second_ribbon_length = running_sessions_text.chars().count() + 4;
    let third_ribbon_length = exited_sessions_text.chars().count() + 4;
    let total_len =
        key_indication_len + first_ribbon_length + second_ribbon_length + third_ribbon_length;
    let key_indication_x = x;
    let first_ribbon_x = key_indication_x + key_indication_len;
    let second_ribbon_x = first_ribbon_x + first_ribbon_length;
    let third_ribbon_x = second_ribbon_x + second_ribbon_length;
    let mut new_session_text = Text::new(new_session_text);
    let mut running_sessions_text = Text::new(running_sessions_text);
    let mut exited_sessions_text = Text::new(exited_sessions_text);
    match active_screen {
        ActiveScreen::NewSession => {
            new_session_text = new_session_text.selected();
        },
        ActiveScreen::AttachToSession => {
            running_sessions_text = running_sessions_text.selected();
        },
        ActiveScreen::ResurrectSession => {
            exited_sessions_text = exited_sessions_text.selected();
        },
    }
    print_text_with_coordinates(
        Text::new(key_indication_text).color_range(3, ..),
        key_indication_x,
        y,
        None,
        None,
    );
    print_ribbon_with_coordinates(new_session_text, first_ribbon_x, y, None, None);
    print_ribbon_with_coordinates(running_sessions_text, second_ribbon_x, y, None, None);
    print_ribbon_with_coordinates(exited_sessions_text, third_ribbon_x, y, None, None);
}

pub fn render_new_session_block(
    new_session_info: &NewSessionInfo,
    colors: Colors,
    max_rows_of_new_session_block: usize,
    max_cols_of_new_session_block: usize,
    x: usize,
    y: usize,
) {
    let enter = colors.magenta("<ENTER>");
    if new_session_info.entering_new_session_name() {
        let prompt = "New session name:";
        let long_instruction = "when done, blank for random";
        let new_session_name = new_session_info.name();
        if max_cols_of_new_session_block
            > prompt.width() + long_instruction.width() + new_session_name.width() + 15
        {
            println!(
                "\u{1b}[m{}{} {}_ ({} {})",
                format!("\u{1b}[{};{}H", y + 1, x + 1),
                colors.green(prompt),
                colors.orange(&new_session_name),
                enter,
                long_instruction,
            );
        } else {
            println!(
                "\u{1b}[m{}{} {}_ {}",
                format!("\u{1b}[{};{}H", y + 1, x + 1),
                colors.green(prompt),
                colors.orange(&new_session_name),
                enter,
            );
        }
    } else if new_session_info.entering_layout_search_term() {
        let new_session_name = if new_session_info.name().is_empty() {
            "<RANDOM>"
        } else {
            new_session_info.name()
        };
        let prompt = "New session name:";
        let long_instruction = "to correct";
        let esc = colors.magenta("<ESC>");
        if max_cols_of_new_session_block
            > prompt.width() + long_instruction.width() + new_session_name.width() + 15
        {
            println!(
                "\u{1b}[m{}{}: {} ({} to correct)",
                format!("\u{1b}[{};{}H", y + 1, x + 1),
                colors.green("New session name"),
                colors.orange(new_session_name),
                esc,
            );
        } else {
            println!(
                "\u{1b}[m{}{}: {} {}",
                format!("\u{1b}[{};{}H", y + 1, x + 1),
                colors.green("New session name"),
                colors.orange(new_session_name),
                esc,
            );
        }
        render_layout_selection_list(
            new_session_info,
            max_rows_of_new_session_block.saturating_sub(1),
            max_cols_of_new_session_block,
            x,
            y + 1,
        );
    }
}

pub fn render_layout_selection_list(
    new_session_info: &NewSessionInfo,
    max_rows_of_new_session_block: usize,
    max_cols_of_new_session_block: usize,
    x: usize,
    y: usize,
) {
    let layout_search_term = new_session_info.layout_search_term();
    let search_term_len = layout_search_term.width();
    let layout_indication_line = if max_cols_of_new_session_block > 73 + search_term_len {
        Text::new(format!(
            "New session layout: {}_ (Search and select from list, <ENTER> when done)",
            layout_search_term
        ))
        .color_range(2, ..20 + search_term_len)
        .color_range(3, 20..20 + search_term_len)
        .color_range(3, 52 + search_term_len..59 + search_term_len)
    } else {
        Text::new(format!(
            "New session layout: {}_ <ENTER>",
            layout_search_term
        ))
        .color_range(2, ..20 + search_term_len)
        .color_range(3, 20..20 + search_term_len)
        .color_range(3, 22 + search_term_len..)
    };
    print_text_with_coordinates(layout_indication_line, x, y + 1, None, None);
    println!();
    let mut table = Table::new();
    for (i, (layout_info, indices, is_selected)) in
        new_session_info.layouts_to_render().into_iter().enumerate()
    {
        let layout_name = layout_info.name();
        let layout_name_len = layout_name.width();
        let is_builtin = layout_info.is_builtin();
        if i > max_rows_of_new_session_block {
            break;
        } else {
            let mut layout_cell = if is_builtin {
                Text::new(format!("{} (built-in)", layout_name))
                    .color_range(1, 0..layout_name_len)
                    .color_range(0, layout_name_len + 1..)
                    .color_indices(3, indices)
            } else {
                Text::new(format!("{}", layout_name))
                    .color_range(1, ..)
                    .color_indices(3, indices)
            };
            if is_selected {
                layout_cell = layout_cell.selected();
            }
            let arrow_cell = if is_selected {
                Text::new(format!("<↓↑>")).selected().color_range(3, ..)
            } else {
                Text::new(format!("    ")).color_range(3, ..)
            };
            table = table.add_styled_row(vec![arrow_cell, layout_cell]);
        }
    }
    print_table_with_coordinates(table, x, y + 3, None, None);
}

pub fn render_error(error_text: &str, rows: usize, columns: usize, x: usize, y: usize) {
    print_text_with_coordinates(
        Text::new(format!("Error: {}", error_text)).color_range(3, ..),
        x,
        y + rows,
        Some(columns),
        None,
    );
}

pub fn render_renaming_session_screen(
    new_session_name: &str,
    rows: usize,
    columns: usize,
    x: usize,
    y: usize,
) {
    if rows == 0 || columns == 0 {
        return;
    }
    let text = Text::new(format!(
        "New name for current session: {}_ (<ENTER> when done)",
        new_session_name
    ))
    .color_range(2, ..29)
    .color_range(
        3,
        33 + new_session_name.width()..40 + new_session_name.width(),
    );
    print_text_with_coordinates(text, x, y, None, None);
}

pub fn render_controls_line(
    active_screen: ActiveScreen,
    max_cols: usize,
    colors: Colors,
    x: usize,
    y: usize,
) {
    match active_screen {
        ActiveScreen::NewSession => {
            if max_cols >= 50 {
                print!(
                    "\u{1b}[m\u{1b}[{y};{x}H\u{1b}[1mHelp: Fill in the form to start a new session."
                );
            }
        },
        ActiveScreen::AttachToSession => {
            let arrows = colors.magenta("<←↓↑→>");
            let navigate = colors.bold("Navigate");
            let enter = colors.magenta("<ENTER>");
            let select = colors.bold("Attach");
            let rename = colors.magenta("<Ctrl r>");
            let rename_text = colors.bold("Rename");
            let disconnect = colors.magenta("<Ctrl x>");
            let disconnect_text = colors.bold("Disconnect others");
            let kill = colors.magenta("<Del>");
            let kill_text = colors.bold("Kill");
            let kill_all = colors.magenta("<Ctrl d>");
            let kill_all_text = colors.bold("Kill all");

            if max_cols > 90 {
                print!(
                    "\u{1b}[m\u{1b}[{y};{x}HHelp: {rename} - {rename_text}, {disconnect} - {disconnect_text}, {kill} - {kill_text}, {kill_all} - {kill_all_text}"
                );
            } else if max_cols >= 28 {
                print!("\u{1b}[m\u{1b}[{y};{x}H{rename}/{disconnect}/{kill}/{kill_all}");
            }
        },
        ActiveScreen::ResurrectSession => {
            let arrows = colors.magenta("<↓↑>");
            let navigate = colors.bold("Navigate");
            let enter = colors.magenta("<ENTER>");
            let select = colors.bold("Resurrect");
            let del = colors.magenta("<DEL>");
            let del_text = colors.bold("Delete");
            let del_all = colors.magenta("<Ctrl d>");
            let del_all_text = colors.bold("Delete all");

            if max_cols > 83 {
                print!(
                    "\u{1b}[m\u{1b}[{y};{x}HHelp: {arrows} - {navigate}, {enter} - {select}, {del} - {del_text}, {del_all} - {del_all_text}"
                );
            } else if max_cols >= 28 {
                print!("\u{1b}[m\u{1b}[{y};{x}H{arrows}/{enter}/{del}/{del_all}");
            }
        },
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Colors {
    pub palette: Palette,
}
impl Colors {
    pub fn new(palette: Palette) -> Self {
        Colors { palette }
    }
    pub fn bold(&self, text: &str) -> String {
        format!("\u{1b}[1m{}\u{1b}[22m", text)
    }

    fn color(&self, color: &PaletteColor, text: &str) -> String {
        match color {
            PaletteColor::EightBit(byte) => {
                format!("\u{1b}[38;5;{};1m{}\u{1b}[39;22m", byte, text)
            },
            PaletteColor::Rgb((r, g, b)) => {
                format!("\u{1b}[38;2;{};{};{};1m{}\u{1b}[39;22m", r, g, b, text)
            },
        }
    }
    pub fn orange(&self, text: &str) -> String {
        self.color(&self.palette.orange, text)
    }

    pub fn green(&self, text: &str) -> String {
        self.color(&self.palette.green, text)
    }

    pub fn red(&self, text: &str) -> String {
        self.color(&self.palette.red, text)
    }

    pub fn cyan(&self, text: &str) -> String {
        self.color(&self.palette.cyan, text)
    }

    pub fn magenta(&self, text: &str) -> String {
        self.color(&self.palette.magenta, text)
    }
}
