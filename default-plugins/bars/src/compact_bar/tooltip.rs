use crate::click_actions::ClickRegion;
use crate::compact_bar::keybind_utils::{KeybindProcessor, TooltipEntry};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct TooltipRegion {
    pub row: usize,
    pub region: ClickRegion,
}

pub fn tooltip_region_at(
    regions: &[TooltipRegion],
    row: usize,
    col: usize,
) -> Option<&ClickRegion> {
    regions
        .iter()
        .find(|r| r.row == row && r.region.contains(col))
        .map(|r| &r.region)
}

struct NormalComponent {
    entry: TooltipEntry,
    x: usize,
    is_first: bool,
}

impl NormalComponent {
    fn key_text(&self) -> String {
        if self.is_first {
            format!("{} ", self.entry.key_text())
        } else {
            format!(" {} ", self.entry.key_text())
        }
    }
    fn key_offset(&self) -> usize {
        if self.is_first {
            0
        } else {
            1
        }
    }
}

fn is_hovered(hovered: Option<&Vec<Action>>, actions: &Option<Vec<Action>>) -> bool {
    match (hovered, actions) {
        (Some(hovered), Some(actions)) => hovered == actions,
        _ => false,
    }
}

fn styled_key_text(
    text: String,
    entry: &TooltipEntry,
    offset: usize,
    hovered: Option<&Vec<Action>>,
) -> (Text, bool) {
    let mut hovered_ranges = vec![];
    let mut plain_ranges = vec![];
    for (start, end, chunk) in entry.chunk_ranges() {
        if is_hovered(hovered, &chunk.actions) {
            hovered_ranges.push((start + offset, end + offset));
        } else {
            plain_ranges.push((start + offset, end + offset));
        }
    }
    if hovered_ranges.is_empty() {
        return (Text::from(text).color_all(3), false);
    }
    let mut styled = Text::from(text);
    for (start, end) in plain_ranges {
        styled = styled.color_range(3, start..end);
    }
    for (start, end) in hovered_ranges {
        styled = styled.color_range(0, start..end);
    }
    (styled.selected(), true)
}

pub struct TooltipRenderer<'a> {
    mode_info: &'a ModeInfo,
}

impl<'a> TooltipRenderer<'a> {
    pub fn new(mode_info: &'a ModeInfo) -> Self {
        Self { mode_info }
    }

    fn entries(&self, mode: InputMode) -> Vec<TooltipEntry> {
        KeybindProcessor::get_tooltip_entries(self.mode_info, mode)
    }

    fn normal_components(&self, mode: InputMode) -> (Vec<NormalComponent>, usize) {
        let mut running_x = 0;
        let mut components = Vec::new();
        let mut max_columns = 0;
        for (index, entry) in self.entries(mode).into_iter().enumerate() {
            let is_first = index == 0;
            let key_len = entry.key_text().chars().count();
            let description_len = entry.description.chars().count();
            let line_length = if is_first {
                key_len + description_len
            } else {
                key_len + 1 + description_len
            };
            components.push(NormalComponent {
                entry,
                x: running_x,
                is_first,
            });
            running_x += line_length + 5;
            max_columns = max_columns.max(running_x);
        }
        (components, max_columns)
    }

    fn table_widths(entries: &[TooltipEntry]) -> (usize, usize) {
        let mut key_width = 0;
        let mut action_width = 0;
        for entry in entries {
            key_width = key_width.max(entry.key_text().chars().count());
            action_width = action_width.max(format!("- {}", entry.description).chars().count());
        }
        (key_width, action_width)
    }

    pub fn regions(&self, rows: usize, cols: usize) -> Vec<TooltipRegion> {
        let current_mode = self.mode_info.mode;
        let mut regions = vec![];
        if current_mode == InputMode::Normal {
            let (components, tooltip_columns) = self.normal_components(current_mode);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(1) / 2;
            for component in components {
                let text_width = component.key_text().chars().count();
                let ribbon_total_width = component.entry.description.chars().count() + 4;
                let start = base_x + component.x;
                if start + text_width + ribbon_total_width + 1 > cols {
                    break;
                }
                let key_start = start + component.key_offset();
                for (chunk_start, chunk_end, chunk) in component.entry.chunk_ranges() {
                    if let Some(actions) = &chunk.actions {
                        regions.push(TooltipRegion {
                            row: base_y,
                            region: ClickRegion::new(
                                key_start + chunk_start,
                                key_start + chunk_end,
                                actions.clone(),
                            ),
                        });
                    }
                }
                if let Some(actions) = component.entry.description_actions() {
                    regions.push(TooltipRegion {
                        row: base_y,
                        region: ClickRegion::new(
                            start + text_width,
                            start + text_width + ribbon_total_width,
                            actions,
                        ),
                    });
                }
            }
        } else {
            let entries = self.entries(current_mode);
            if entries.is_empty() {
                return regions;
            }
            let (key_width, action_width) = Self::table_widths(&entries);
            let total_width = key_width + action_width + 1;
            let tooltip_rows = entries.len() + 1;
            let base_x = cols.saturating_sub(total_width) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;
            let key_column_width = key_width.max(1);
            for (index, entry) in entries.iter().enumerate() {
                let row = base_y + 1 + index;
                for (chunk_start, chunk_end, chunk) in entry.chunk_ranges() {
                    if let Some(actions) = &chunk.actions {
                        regions.push(TooltipRegion {
                            row,
                            region: ClickRegion::new(
                                base_x + chunk_start,
                                base_x + chunk_end,
                                actions.clone(),
                            ),
                        });
                    }
                }
                if let Some(actions) = entry.description_actions() {
                    let start = base_x + key_column_width + 1;
                    let end = start + format!("- {}", entry.description).chars().count();
                    regions.push(TooltipRegion {
                        row,
                        region: ClickRegion::new(start, end, actions),
                    });
                }
            }
        }
        regions
    }

    pub fn render(&self, rows: usize, cols: usize, hovered: Option<&Vec<Action>>) {
        let current_mode = self.mode_info.mode;

        if current_mode == InputMode::Normal {
            let (components, tooltip_columns) = self.normal_components(current_mode);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(1) / 2;

            for component in components {
                let key_text = component.key_text();
                let text_width = key_text.chars().count();
                let ribbon_content_width = component.entry.description.chars().count();
                let ribbon_total_width = ribbon_content_width + 4;
                let total_element_width = text_width + ribbon_total_width + 1;
                let x = component.x;

                if base_x + x + total_element_width > cols {
                    let remaining_space = cols.saturating_sub(base_x + x);
                    let ellipsis = Text::from("...").opaque();
                    print_text_with_coordinates(
                        ellipsis,
                        base_x + x,
                        base_y,
                        Some(remaining_space),
                        None,
                    );
                    break;
                }

                let (text, _) =
                    styled_key_text(key_text, &component.entry, component.key_offset(), hovered);
                let ribbon = Text::from(component.entry.description.clone());
                let ribbon = if is_hovered(hovered, &component.entry.description_actions()) {
                    ribbon.selected()
                } else {
                    ribbon
                };
                print_text_with_coordinates(text.opaque(), base_x + x, base_y, None, None);
                print_ribbon_with_coordinates(ribbon, base_x + x + text_width, base_y, None, None);
            }
        } else {
            let (table, tooltip_rows, tooltip_columns) =
                self.other_mode_tooltip(current_mode, hovered);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;
            print_table_with_coordinates(table, base_x, base_y, None, None);
        }
    }

    pub fn calculate_dimensions(&self, current_mode: InputMode) -> (usize, usize) {
        match current_mode {
            InputMode::Normal => {
                let (_, tooltip_cols) = self.normal_components(current_mode);
                (1, tooltip_cols)
            },
            _ => {
                let (_, tooltip_rows, tooltip_cols) = self.other_mode_tooltip(current_mode, None);
                (tooltip_rows + 1, tooltip_cols)
            },
        }
    }

    fn other_mode_tooltip(
        &self,
        current_mode: InputMode,
        hovered: Option<&Vec<Action>>,
    ) -> (Table, usize, usize) {
        let entries = self.entries(current_mode);

        let mut table = Table::new().add_row(vec![" ".to_owned(); 2]);
        let mut row_count = 1;

        if entries.is_empty() {
            let tooltip_text = match self.mode_info.mode {
                InputMode::EnterSearch => "Entering search term...".to_owned(),
                InputMode::RenameTab => "Renaming tab...".to_owned(),
                InputMode::RenamePane => "Renaming pane...".to_owned(),
                _ => {
                    format!("{:?}", self.mode_info.mode)
                },
            };
            let total_width = tooltip_text.chars().count();
            table = table.add_styled_row(vec![Text::from(tooltip_text).color_all(0)]);
            row_count += 1;
            (table, row_count, total_width)
        } else {
            let (key_width, action_width) = Self::table_widths(&entries);
            for entry in entries.iter() {
                let description = Text::from(format!("- {}", entry.description));
                let description = if is_hovered(hovered, &entry.description_actions()) {
                    description.selected()
                } else {
                    description
                };
                let (key, _) = styled_key_text(entry.key_text(), entry, 0, hovered);
                table = table.add_styled_row(vec![key, description]);
                row_count += 1;
            }

            let total_width = key_width + action_width + 1;
            (table, row_count, total_width)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key)
    }

    fn resize(resize: Resize, direction: Option<Direction>) -> Vec<Action> {
        vec![Action::Resize { resize, direction }]
    }

    fn resize_mode_info() -> ModeInfo {
        ModeInfo {
            mode: InputMode::Resize,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(
                InputMode::Resize,
                vec![
                    (key(BareKey::Char('+')), resize(Resize::Increase, None)),
                    (key(BareKey::Char('-')), resize(Resize::Decrease, None)),
                    (
                        key(BareKey::Left),
                        resize(Resize::Increase, Some(Direction::Left)),
                    ),
                    (
                        key(BareKey::Right),
                        resize(Resize::Increase, Some(Direction::Right)),
                    ),
                ],
            )],
            ..Default::default()
        }
    }

    fn normal_mode_info() -> ModeInfo {
        ModeInfo {
            mode: InputMode::Normal,
            base_mode: Some(InputMode::Normal),
            keybinds: vec![(
                InputMode::Normal,
                vec![
                    (
                        KeyWithModifier::new(BareKey::Char('g')).with_ctrl_modifier(),
                        vec![Action::SwitchToMode {
                            input_mode: InputMode::Locked,
                        }],
                    ),
                    (
                        KeyWithModifier::new(BareKey::Char('p')).with_ctrl_modifier(),
                        vec![Action::SwitchToMode {
                            input_mode: InputMode::Pane,
                        }],
                    ),
                ],
            )],
            ..Default::default()
        }
    }

    #[test]
    fn each_arrow_and_plus_minus_in_the_tooltip_has_its_own_region() {
        let mode_info = resize_mode_info();
        let renderer = TooltipRenderer::new(&mode_info);
        let regions = renderer.regions(10, 60);
        let find = |actions: &Vec<Action>| {
            regions
                .iter()
                .find(|r| &r.region.actions == actions)
                .cloned()
                .unwrap_or_else(|| panic!("{:?} not in {:?}", actions, regions))
        };
        let increase = find(&resize(Resize::Increase, None));
        let decrease = find(&resize(Resize::Decrease, None));
        assert_eq!(increase.row, decrease.row);
        assert_eq!(increase.region.end - increase.region.start, 1);
        assert_eq!(decrease.region.start, increase.region.end);
        let left = find(&resize(Resize::Increase, Some(Direction::Left)));
        let right = find(&resize(Resize::Increase, Some(Direction::Right)));
        assert_eq!(left.row, right.row);
        assert_ne!(left.row, increase.row);
        assert_eq!(right.region.start, left.region.end);
        assert!(regions
            .iter()
            .all(|r| r.region.actions != resize(Resize::Increase, None)
                || r.region.end - r.region.start == 1));
        assert_eq!(
            tooltip_region_at(&regions, left.row, left.region.start).map(|r| r.actions.clone()),
            Some(resize(Resize::Increase, Some(Direction::Left)))
        );
        assert_eq!(tooltip_region_at(&regions, left.row, 0), None);
    }

    #[test]
    fn table_regions_match_the_table_layout() {
        let mode_info = resize_mode_info();
        let renderer = TooltipRenderer::new(&mode_info);
        let (rows, cols) = renderer.calculate_dimensions(InputMode::Resize);
        let regions = renderer.regions(rows, cols + 10);
        let first_row = regions.iter().map(|r| r.row).min().unwrap();
        assert_eq!(first_row, (rows - 3) / 2 + 1);
        let increase = regions
            .iter()
            .find(|r| r.region.actions == resize(Resize::Increase, None))
            .unwrap();
        assert_eq!(increase.region.start, 5 + 1);
    }

    #[test]
    fn normal_mode_tooltip_entries_are_clickable_on_key_and_description() {
        let mode_info = normal_mode_info();
        let renderer = TooltipRenderer::new(&mode_info);
        let regions = renderer.regions(1, 80);
        let lock = vec![Action::SwitchToMode {
            input_mode: InputMode::Locked,
        }];
        let lock_regions: Vec<&TooltipRegion> = regions
            .iter()
            .filter(|r| r.region.actions == lock)
            .collect();
        assert_eq!(lock_regions.len(), 2);
        assert_eq!(lock_regions[0].region.end, lock_regions[1].region.start - 1);
        assert!(regions.iter().any(|r| r.region.actions
            == vec![Action::SwitchToMode {
                input_mode: InputMode::Pane
            }]));
    }

    #[test]
    fn hovering_marks_the_hovered_chunk_and_leaves_others_alone() {
        let mode_info = resize_mode_info();
        let entries = KeybindProcessor::get_tooltip_entries(&mode_info, InputMode::Resize);
        let plus_minus = entries
            .iter()
            .find(|e| e.key_text() == "<+->")
            .unwrap_or_else(|| panic!("{:?}", entries));
        let hovered = resize(Resize::Decrease, None);
        let (_, is_hovered) = styled_key_text(plus_minus.key_text(), plus_minus, 0, Some(&hovered));
        assert!(is_hovered);
        let (_, not_hovered) = styled_key_text(
            plus_minus.key_text(),
            plus_minus,
            0,
            Some(&vec![Action::Detach]),
        );
        assert!(!not_hovered);
        assert_eq!(plus_minus.description_actions(), None);
    }
}
