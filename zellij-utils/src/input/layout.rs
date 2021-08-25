//! The layout system.
//  Layouts have been moved from [`zellij-server`] to
//  [`zellij-utils`] in order to provide more helpful
//  error messages to the user until a more general
//  logging system is in place.
//  In case there is a logging system in place evaluate,
//  if [`zellij-utils`], or [`zellij-server`] is a proper
//  place.
//  If plugins should be able to depend on the layout system
//  then [`zellij-utils`] could be a proper place.
use crate::{
    input::{command::RunCommand, config::ConfigError},
    pane_size::{Constraint, Dimension, PaneGeom},
    setup,
};
use crate::{serde, serde_yaml};

use serde::{Deserialize, Serialize};
use std::{
    cmp::max,
    path::{Path, PathBuf},
};
use std::{fs::File, io::prelude::*};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "self::serde")]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "self::serde")]
pub enum SplitSize {
    Percent(f64), // 1 to 100
    Fixed(usize), // An absolute number of columns or rows
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "self::serde")]
pub enum Run {
    #[serde(rename = "plugin")]
    Plugin(Option<PathBuf>),
    #[serde(rename = "command")]
    Command(RunCommand),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "self::serde")]
pub struct Layout {
    pub direction: Direction,
    #[serde(default)]
    pub parts: Vec<Layout>,
    pub split_size: Option<SplitSize>,
    pub run: Option<Run>,
    #[serde(default)]
    pub borderless: bool,
}

type LayoutResult = Result<Layout, ConfigError>;

impl Layout {
    pub fn new(layout_path: &Path) -> LayoutResult {
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_path.with_extension("yaml")))
            .map_err(|e| ConfigError::IoPath(e, layout_path.into()))?;

        let mut layout = String::new();
        layout_file.read_to_string(&mut layout)?;
        let layout: Layout = serde_yaml::from_str(&layout)?;
        Ok(layout)
    }

    // It wants to use Path here, but that doesn't compile.
    #[allow(clippy::ptr_arg)]
    pub fn from_dir(layout: &PathBuf, layout_dir: Option<&PathBuf>) -> LayoutResult {
        match layout_dir {
            Some(dir) => Self::new(&dir.join(layout))
                .or_else(|_| Self::from_default_assets(layout.as_path())),
            None => Self::from_default_assets(layout.as_path()),
        }
    }

    pub fn from_path_or_default(
        layout: Option<&PathBuf>,
        layout_path: Option<&PathBuf>,
        layout_dir: Option<PathBuf>,
    ) -> Option<Result<Layout, ConfigError>> {
        layout
            .map(|p| Layout::from_dir(p, layout_dir.as_ref()))
            .or_else(|| layout_path.map(|p| Layout::new(p)))
            .or_else(|| {
                Some(Layout::from_dir(
                    &std::path::PathBuf::from("default"),
                    layout_dir.as_ref(),
                ))
            })
    }

    // Currently still needed but on nightly
    // this is already possible:
    // HashMap<&'static str, Vec<u8>>
    pub fn from_default_assets(path: &Path) -> LayoutResult {
        match path.to_str() {
            Some("default") => Self::default_from_assets(),
            Some("strider") => Self::strider_from_assets(),
            Some("disable-status-bar") => Self::disable_status_from_assets(),
            None | Some(_) => Err(ConfigError::IoPath(
                std::io::Error::new(std::io::ErrorKind::Other, "The layout was not found"),
                path.into(),
            )),
        }
    }

    // TODO Deserialize the assets from bytes &[u8],
    // once serde-yaml supports zero-copy
    pub fn default_from_assets() -> LayoutResult {
        let layout: Layout =
            serde_yaml::from_str(String::from_utf8(setup::DEFAULT_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }

    pub fn strider_from_assets() -> LayoutResult {
        let layout: Layout =
            serde_yaml::from_str(String::from_utf8(setup::STRIDER_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }

    pub fn disable_status_from_assets() -> LayoutResult {
        let layout: Layout =
            serde_yaml::from_str(String::from_utf8(setup::NO_STATUS_LAYOUT.to_vec())?.as_str())?;
        Ok(layout)
    }

    pub fn total_terminal_panes(&self) -> usize {
        let mut total_panes = 0;
        total_panes += self.parts.len();
        for part in self.parts.iter() {
            match part.run {
                Some(Run::Command(_)) | None => {
                    total_panes += part.total_terminal_panes();
                }
                Some(Run::Plugin(_)) => {}
            }
        }
        total_panes
    }

    pub fn total_borderless_panes(&self) -> usize {
        let mut total_borderless_panes = 0;
        total_borderless_panes += self.parts.iter().filter(|p| p.borderless).count();
        for part in self.parts.iter() {
            total_borderless_panes += part.total_borderless_panes();
        }
        total_borderless_panes
    }
    pub fn extract_run_instructions(&self) -> Vec<Option<Run>> {
        let mut run_instructions = vec![];
        if self.parts.is_empty() {
            run_instructions.push(self.run.clone());
        }
        for part in self.parts.iter() {
            let mut current_runnables = part.extract_run_instructions();
            run_instructions.append(&mut current_runnables);
        }
        run_instructions
    }

    pub fn position_panes_in_space(&self, space: &PaneGeom) -> Vec<(Layout, PaneGeom)> {
        split_space(space, self)
    }
}

fn layout_size(direction: Direction, layout: &Layout) -> usize {
    fn child_layout_size(
        direction: Direction,
        parent_direction: Direction,
        layout: &Layout,
    ) -> usize {
        let size = if parent_direction == direction { 1 } else { 0 };
        if layout.parts.is_empty() {
            size
        } else {
            let children_size = layout
                .parts
                .iter()
                .map(|p| child_layout_size(direction, layout.direction, p))
                .sum::<usize>();
            max(size, children_size)
        }
    }
    child_layout_size(direction, direction, layout)
}

fn split_space(space_to_split: &PaneGeom, layout: &Layout) -> Vec<(Layout, PaneGeom)> {
    let mut pane_positions = Vec::new();
    let sizes: Vec<Option<SplitSize>> = layout.parts.iter().map(|part| part.split_size).collect();

    // FIXME: Merge the two branches of this match statement and deduplicate
    let split_parts = match layout.direction {
        Direction::Vertical => {
            let mut split_parts = Vec::new();
            let mut current_x_position = space_to_split.x;

            let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

            // First fit in the parameterized sizes
            for (&size, part) in sizes.iter().zip(&layout.parts) {
                let cols = match size {
                    Some(SplitSize::Percent(percent)) => Dimension::percent(percent),
                    Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
                    None => {
                        let free_percent =
                            if let Constraint::Percent(p) = space_to_split.cols.constraint {
                                p - sizes
                                    .iter()
                                    .map(|&s| {
                                        if let Some(SplitSize::Percent(ip)) = s {
                                            ip
                                        } else {
                                            0.0
                                        }
                                    })
                                    .sum::<f64>()
                            } else {
                                panic!("Implicit sizing within fixed-size panes is not supported");
                            };
                        Dimension::percent(free_percent / flex_parts as f64)
                    }
                };
                let mut rows = space_to_split.rows;
                rows.set_inner(
                    layout
                        .parts
                        .iter()
                        .map(|p| layout_size(Direction::Horizontal, p))
                        .max()
                        .unwrap(),
                );
                split_parts.push(PaneGeom {
                    x: current_x_position,
                    y: space_to_split.y,
                    // FIXME: This is likely wrong and percent should be considered!
                    cols,
                    // FIXME: Set the inner layout usize using layout_size for fib.yaml
                    rows,
                });
                current_x_position += layout_size(Direction::Vertical, part);
            }

            split_parts
        }
        Direction::Horizontal => {
            let mut split_parts = Vec::new();
            let mut current_y_position = space_to_split.y;

            let flex_parts = sizes.iter().filter(|s| s.is_none()).count();

            for (&size, part) in sizes.iter().zip(&layout.parts) {
                let rows = match size {
                    Some(SplitSize::Percent(percent)) => Dimension::percent(percent),
                    Some(SplitSize::Fixed(size)) => Dimension::fixed(size),
                    None => {
                        let free_percent =
                            if let Constraint::Percent(p) = space_to_split.rows.constraint {
                                p - sizes
                                    .iter()
                                    .map(|&s| {
                                        if let Some(SplitSize::Percent(ip)) = s {
                                            ip
                                        } else {
                                            0.0
                                        }
                                    })
                                    .sum::<f64>()
                            } else {
                                panic!("Implicit sizing within fixed-size panes is not supported");
                            };
                        Dimension::percent(free_percent / flex_parts as f64)
                    }
                };
                let mut cols = space_to_split.cols;
                cols.set_inner(
                    layout
                        .parts
                        .iter()
                        .map(|p| layout_size(Direction::Vertical, p))
                        .max()
                        .unwrap(),
                );
                split_parts.push(PaneGeom {
                    x: space_to_split.x,
                    y: current_y_position,
                    // FIXME: This is probably wrong
                    cols,
                    rows,
                });
                current_y_position += layout_size(Direction::Horizontal, part);
            }

            split_parts
        }
    };
    for (i, part) in layout.parts.iter().enumerate() {
        let part_position_and_size = split_parts.get(i).unwrap();
        if !part.parts.is_empty() {
            let mut part_positions = split_space(part_position_and_size, part);
            pane_positions.append(&mut part_positions);
        } else {
            pane_positions.push((part.clone(), *part_position_and_size));
        }
    }
    pane_positions
}
