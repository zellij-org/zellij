use crate::vendored::termwiz::cell::Cell;
use crate::vendored::termwiz::surface::line::cellref::CellRef;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VecStorage {
    cells: Vec<Cell>,
}

impl VecStorage {
    pub(crate) fn new(cells: Vec<Cell>) -> Self {
        Self { cells }
    }

    pub(crate) fn set_cell(&mut self, idx: usize, mut cell: Cell, clear_image_placement: bool) {
        if !clear_image_placement {
            if let Some(images) = self.cells[idx].attrs().images() {
                for image in images {
                    if image.has_placement_id() {
                        cell.attrs_mut().attach_image(Box::new(image));
                    }
                }
            }
        }
        self.cells[idx] = cell;
    }

    pub(crate) fn scan_and_create_hyperlinks(
        &mut self,
        line: &str,
        matches: Vec<crate::vendored::termwiz::hyperlink::RuleMatch>,
    ) -> bool {
        // The capture range is measured in bytes but we need to translate
        // that to the index of the column.  This is complicated a bit further
        // because double wide sequences have a blank column cell after them
        // in the cells array, but the string we match against excludes that
        // string.
        let mut cell_idx = 0;
        let mut has_implicit_hyperlinks = false;
        for (byte_idx, _grapheme) in line.grapheme_indices(true) {
            let cell = &mut self.cells[cell_idx];
            let mut matched = false;
            for m in &matches {
                if m.range.contains(&byte_idx) {
                    let attrs = cell.attrs_mut();
                    // Don't replace existing links
                    if attrs.hyperlink().is_none() {
                        attrs.set_hyperlink(Some(Arc::clone(&m.link)));
                        matched = true;
                    }
                }
            }
            cell_idx += cell.width();
            if matched {
                has_implicit_hyperlinks = true;
            }
        }

        has_implicit_hyperlinks
    }
}

impl std::ops::Deref for VecStorage {
    type Target = Vec<Cell>;

    fn deref(&self) -> &Vec<Cell> {
        &self.cells
    }
}

impl std::ops::DerefMut for VecStorage {
    fn deref_mut(&mut self) -> &mut Vec<Cell> {
        &mut self.cells
    }
}

/// Iterates over a slice of Cell, yielding only visible cells
pub(crate) struct VecStorageIter<'a> {
    pub cells: std::slice::Iter<'a, Cell>,
    pub idx: usize,
    pub skip_width: usize,
}

impl<'a> Iterator for VecStorageIter<'a> {
    type Item = CellRef<'a>;

    fn next(&mut self) -> Option<CellRef<'a>> {
        while self.skip_width > 0 {
            self.skip_width -= 1;
            let _ = self.cells.next()?;
            self.idx += 1;
        }
        let cell = self.cells.next()?;
        let cell_index = self.idx;
        self.idx += 1;
        self.skip_width = cell.width().saturating_sub(1);
        Some(CellRef::CellRef { cell_index, cell })
    }
}
