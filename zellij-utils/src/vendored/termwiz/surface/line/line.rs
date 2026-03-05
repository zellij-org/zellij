use crate::vendored::termwiz::cell::{Cell, CellAttributes, SemanticType, UnicodeVersion};
use crate::vendored::termwiz::cellcluster::CellCluster;
use crate::vendored::termwiz::hyperlink::Rule;
use crate::vendored::termwiz::surface::line::cellref::CellRef;
use crate::vendored::termwiz::surface::line::clusterline::ClusteredLine;
use crate::vendored::termwiz::surface::line::linebits::LineBits;
use crate::vendored::termwiz::surface::line::storage::{CellStorage, VisibleCellIter};
use crate::vendored::termwiz::surface::line::vecstorage::{VecStorage, VecStorageIter};
use crate::vendored::termwiz::surface::{Change, SequenceNo, SEQ_ZERO};
use finl_unicode::grapheme_clusters::Graphemes;
#[cfg(feature = "use_serde")]
use serde::{Deserialize, Serialize};
use siphasher::sip128::{Hasher128, SipHasher};
use std::any::Any;
use std::borrow::Cow;
use std::hash::Hash;
use std::ops::Range;
use std::sync::{Arc, Mutex, Weak};
use wezterm_bidi::{Direction, ParagraphDirectionHint};

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneRange {
    pub semantic_type: SemanticType,
    pub range: Range<u16>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DoubleClickRange {
    Range(Range<usize>),
    RangeWithWrap(Range<usize>),
}

#[cfg_attr(feature = "use_serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct Line {
    pub(crate) cells: CellStorage,
    zones: Vec<ZoneRange>,
    seqno: SequenceNo,
    bits: LineBits,
    #[cfg_attr(feature = "use_serde", serde(skip))]
    appdata: Mutex<Option<Weak<dyn Any + Send + Sync>>>,
}

impl Clone for Line {
    fn clone(&self) -> Self {
        Self {
            cells: self.cells.clone(),
            zones: self.zones.clone(),
            seqno: self.seqno,
            bits: self.bits,
            appdata: Mutex::new(self.appdata.lock().unwrap().clone()),
        }
    }
}

impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.seqno == other.seqno && self.bits == other.bits && self.cells == other.cells
    }
}

impl Line {
    pub fn with_width_and_cell(width: usize, cell: Cell, seqno: SequenceNo) -> Self {
        let mut cells = Vec::with_capacity(width);
        cells.resize(width, cell.clone());
        let bits = LineBits::NONE;
        Self {
            bits,
            cells: CellStorage::V(VecStorage::new(cells)),
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    pub fn from_cells(cells: Vec<Cell>, seqno: SequenceNo) -> Self {
        let bits = LineBits::NONE;
        Self {
            bits,
            cells: CellStorage::V(VecStorage::new(cells)),
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    /// Create a new line using cluster storage, optimized for appending
    /// and lower memory utilization.
    /// The line will automatically switch to cell storage when necessary
    /// to apply edits.
    pub fn new(seqno: SequenceNo) -> Self {
        Self {
            bits: LineBits::NONE,
            cells: CellStorage::C(ClusteredLine::new()),
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    /// Computes a hash over the line that will change if the way that
    /// the line contents are shaped would change.
    /// This is independent of the seqno and is based purely on the
    /// content of the line.
    ///
    /// Line doesn't implement Hash in terms of this function as compute_shape_hash
    /// doesn't every possible bit of internal state, and we don't want to
    /// encourage using Line directly as a hash key.
    pub fn compute_shape_hash(&self) -> [u8; 16] {
        let mut hasher = SipHasher::new();
        self.bits.bits().hash(&mut hasher);
        for cell in self.visible_cells() {
            cell.compute_shape_hash(&mut hasher);
        }
        hasher.finish128().as_bytes()
    }

    pub fn with_width(width: usize, seqno: SequenceNo) -> Self {
        let mut cells = Vec::with_capacity(width);
        cells.resize_with(width, Cell::blank);
        let bits = LineBits::NONE;
        Self {
            bits,
            cells: CellStorage::V(VecStorage::new(cells)),
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    pub fn from_text(
        s: &str,
        attrs: &CellAttributes,
        seqno: SequenceNo,
        unicode_version: Option<UnicodeVersion>,
    ) -> Line {
        let mut cells = Vec::new();

        for sub in Graphemes::new(s) {
            let cell = Cell::new_grapheme(sub, attrs.clone(), unicode_version);
            let width = cell.width();
            cells.push(cell);
            for _ in 1..width {
                cells.push(Cell::new(' ', attrs.clone()));
            }
        }

        Line {
            cells: CellStorage::V(VecStorage::new(cells)),
            bits: LineBits::NONE,
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    pub fn from_text_with_wrapped_last_col(
        s: &str,
        attrs: &CellAttributes,
        seqno: SequenceNo,
    ) -> Line {
        let mut line = Self::from_text(s, attrs, seqno, None);
        line.cells_mut()
            .last_mut()
            .map(|cell| cell.attrs_mut().set_wrapped(true));
        line
    }

    pub fn resize_and_clear(
        &mut self,
        width: usize,
        seqno: SequenceNo,
        blank_attr: CellAttributes,
    ) {
        {
            let cells = self.coerce_vec_storage();
            for c in cells.iter_mut() {
                *c = Cell::blank_with_attrs(blank_attr.clone());
            }
            cells.resize_with(width, || Cell::blank_with_attrs(blank_attr.clone()));
            cells.shrink_to_fit();
        }
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
        self.bits = LineBits::NONE;
    }

    pub fn resize(&mut self, width: usize, seqno: SequenceNo) {
        self.coerce_vec_storage().resize_with(width, Cell::blank);
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    /// Wrap the line so that it fits within the provided width.
    /// Returns the list of resultant line(s)
    pub fn wrap(self, width: usize, seqno: SequenceNo) -> Vec<Self> {
        let mut cells: Vec<CellRef> = self.visible_cells().collect();
        if let Some(end_idx) = cells.iter().rposition(|c| c.str() != " ") {
            cells.truncate(end_idx + 1);

            let mut lines: Vec<Self> = vec![];
            let mut delta = 0;
            for cell in cells {
                let need_new_line = lines
                    .last_mut()
                    .map(|line| line.len() + cell.width() > width)
                    .unwrap_or(true);
                if need_new_line {
                    lines
                        .last_mut()
                        .map(|line| line.set_last_cell_was_wrapped(true, seqno));
                    lines.push(Line::new(seqno));
                    delta = cell.cell_index();
                }
                let line = lines.last_mut().unwrap();
                line.set_cell_grapheme(
                    cell.cell_index() - delta,
                    cell.str(),
                    cell.width(),
                    (*cell.attrs()).clone(),
                    seqno,
                );
            }

            lines
        } else {
            vec![self]
        }
    }

    /// Set arbitrary application specific data for the line.
    /// Only one piece of appdata can be tracked per line,
    /// so this is only suitable for the overall application
    /// and not for use by "middleware" crates.
    /// A Weak reference is stored.
    /// `get_appdata` is used to retrieve a previously stored reference.
    pub fn set_appdata<T: Any + Send + Sync>(&self, appdata: Arc<T>) {
        let appdata: Arc<dyn Any + Send + Sync> = appdata;
        self.appdata
            .lock()
            .unwrap()
            .replace(Arc::downgrade(&appdata));
    }

    pub fn clear_appdata(&self) {
        self.appdata.lock().unwrap().take();
    }

    /// Retrieve the appdata for the line, if any.
    /// This may return None in the case where the underlying data has
    /// been released: Line only stores a Weak reference to it.
    pub fn get_appdata(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        self.appdata
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|data| data.upgrade())
    }

    /// Returns true if the line's last changed seqno is more recent
    /// than the provided seqno parameter
    pub fn changed_since(&self, seqno: SequenceNo) -> bool {
        self.seqno == SEQ_ZERO || self.seqno > seqno
    }

    pub fn current_seqno(&self) -> SequenceNo {
        self.seqno
    }

    /// Annotate the line with the sequence number of a change.
    /// This can be used together with Line::changed_since to
    /// manage caching and rendering
    #[inline]
    pub fn update_last_change_seqno(&mut self, seqno: SequenceNo) {
        self.seqno = self.seqno.max(seqno);
    }

    /// Check whether the line is single-width.
    #[inline]
    pub fn is_single_width(&self) -> bool {
        (self.bits
            & (LineBits::DOUBLE_WIDTH
                | LineBits::DOUBLE_HEIGHT_TOP
                | LineBits::DOUBLE_HEIGHT_BOTTOM))
            == LineBits::NONE
    }

    /// Force single-width.  This also implicitly sets
    /// double-height-(top/bottom) and dirty.
    #[inline]
    pub fn set_single_width(&mut self, seqno: SequenceNo) {
        self.bits.remove(LineBits::DOUBLE_WIDTH_HEIGHT_MASK);
        self.update_last_change_seqno(seqno);
    }

    /// Check whether the line is double-width and not double-height.
    #[inline]
    pub fn is_double_width(&self) -> bool {
        (self.bits & LineBits::DOUBLE_WIDTH_HEIGHT_MASK) == LineBits::DOUBLE_WIDTH
    }

    /// Force double-width.  This also implicitly sets
    /// double-height-(top/bottom) and dirty.
    #[inline]
    pub fn set_double_width(&mut self, seqno: SequenceNo) {
        self.bits
            .remove(LineBits::DOUBLE_HEIGHT_TOP | LineBits::DOUBLE_HEIGHT_BOTTOM);
        self.bits.insert(LineBits::DOUBLE_WIDTH);
        self.update_last_change_seqno(seqno);
    }

    /// Check whether the line is double-height-top.
    #[inline]
    pub fn is_double_height_top(&self) -> bool {
        (self.bits & LineBits::DOUBLE_WIDTH_HEIGHT_MASK)
            == LineBits::DOUBLE_WIDTH | LineBits::DOUBLE_HEIGHT_TOP
    }

    /// Force double-height top-half.  This also implicitly sets
    /// double-width and dirty.
    #[inline]
    pub fn set_double_height_top(&mut self, seqno: SequenceNo) {
        self.bits.remove(LineBits::DOUBLE_HEIGHT_BOTTOM);
        self.bits
            .insert(LineBits::DOUBLE_WIDTH | LineBits::DOUBLE_HEIGHT_TOP);
        self.update_last_change_seqno(seqno);
    }

    /// Check whether the line is double-height-bottom.
    #[inline]
    pub fn is_double_height_bottom(&self) -> bool {
        (self.bits & LineBits::DOUBLE_WIDTH_HEIGHT_MASK)
            == LineBits::DOUBLE_WIDTH | LineBits::DOUBLE_HEIGHT_BOTTOM
    }

    /// Force double-height bottom-half.  This also implicitly sets
    /// double-width and dirty.
    #[inline]
    pub fn set_double_height_bottom(&mut self, seqno: SequenceNo) {
        self.bits.remove(LineBits::DOUBLE_HEIGHT_TOP);
        self.bits
            .insert(LineBits::DOUBLE_WIDTH | LineBits::DOUBLE_HEIGHT_BOTTOM);
        self.update_last_change_seqno(seqno);
    }

    /// Set a flag the indicate whether the line should have the bidi
    /// algorithm applied during rendering
    pub fn set_bidi_enabled(&mut self, enabled: bool, seqno: SequenceNo) {
        self.bits.set(LineBits::BIDI_ENABLED, enabled);
        self.update_last_change_seqno(seqno);
    }

    /// Set the bidi direction for the line.
    /// This affects both the bidi algorithm (if enabled via set_bidi_enabled)
    /// and the layout direction of the line.
    /// `auto_detect` specifies whether the direction should be auto-detected
    /// before falling back to the specified direction.
    pub fn set_direction(&mut self, direction: Direction, auto_detect: bool, seqno: SequenceNo) {
        self.bits
            .set(LineBits::RTL, direction == Direction::LeftToRight);
        self.bits.set(LineBits::AUTO_DETECT_DIRECTION, auto_detect);
        self.update_last_change_seqno(seqno);
    }

    pub fn set_bidi_info(
        &mut self,
        enabled: bool,
        direction: ParagraphDirectionHint,
        seqno: SequenceNo,
    ) {
        self.bits.set(LineBits::BIDI_ENABLED, enabled);
        let (auto, rtl) = match direction {
            ParagraphDirectionHint::AutoRightToLeft => (true, true),
            ParagraphDirectionHint::AutoLeftToRight => (true, false),
            ParagraphDirectionHint::LeftToRight => (false, false),
            ParagraphDirectionHint::RightToLeft => (false, true),
        };
        self.bits.set(LineBits::AUTO_DETECT_DIRECTION, auto);
        self.bits.set(LineBits::RTL, rtl);
        self.update_last_change_seqno(seqno);
    }

    /// Returns a tuple of (BIDI_ENABLED, Direction), indicating whether
    /// the line should have the bidi algorithm applied and its base
    /// direction, respectively.
    pub fn bidi_info(&self) -> (bool, ParagraphDirectionHint) {
        (
            self.bits.contains(LineBits::BIDI_ENABLED),
            match (
                self.bits.contains(LineBits::AUTO_DETECT_DIRECTION),
                self.bits.contains(LineBits::RTL),
            ) {
                (true, true) => ParagraphDirectionHint::AutoRightToLeft,
                (false, true) => ParagraphDirectionHint::RightToLeft,
                (true, false) => ParagraphDirectionHint::AutoLeftToRight,
                (false, false) => ParagraphDirectionHint::LeftToRight,
            },
        )
    }

    fn invalidate_zones(&mut self) {
        self.zones.clear();
    }

    fn compute_zones(&mut self) {
        let blank_cell = Cell::blank();
        let mut last_cell: Option<CellRef> = None;
        let mut current_zone: Option<ZoneRange> = None;
        let mut zones = vec![];

        // Rows may have trailing space+Output cells interleaved
        // with other zones as a result of clear-to-eol and
        // clear-to-end-of-screen sequences.  We don't want
        // those to affect the zones that we compute here
        let mut last_non_blank = self.len();
        for cell in self.visible_cells() {
            if cell.str() != blank_cell.str() || cell.attrs() != blank_cell.attrs() {
                last_non_blank = cell.cell_index();
            }
        }

        for cell in self.visible_cells() {
            if cell.cell_index() > last_non_blank {
                break;
            }
            let grapheme_idx = cell.cell_index() as u16;
            let semantic_type = cell.attrs().semantic_type();
            let new_zone = match last_cell {
                None => true,
                Some(ref c) => c.attrs().semantic_type() != semantic_type,
            };

            if new_zone {
                if let Some(zone) = current_zone.take() {
                    zones.push(zone);
                }

                current_zone.replace(ZoneRange {
                    range: grapheme_idx..grapheme_idx + 1,
                    semantic_type,
                });
            }

            if let Some(zone) = current_zone.as_mut() {
                zone.range.end = grapheme_idx;
            }

            last_cell.replace(cell);
        }

        if let Some(zone) = current_zone.take() {
            zones.push(zone);
        }
        self.zones = zones;
    }

    pub fn semantic_zone_ranges(&mut self) -> &[ZoneRange] {
        if self.zones.is_empty() {
            self.compute_zones();
        }
        &self.zones
    }

    /// If we have any cells with an implicit hyperlink, remove the hyperlink
    /// from the cell attributes but leave the remainder of the attributes alone.
    #[inline]
    pub fn invalidate_implicit_hyperlinks(&mut self, seqno: SequenceNo) {
        if (self.bits & (LineBits::SCANNED_IMPLICIT_HYPERLINKS | LineBits::HAS_IMPLICIT_HYPERLINKS))
            == LineBits::NONE
        {
            return;
        }

        self.bits &= !LineBits::SCANNED_IMPLICIT_HYPERLINKS;
        if (self.bits & LineBits::HAS_IMPLICIT_HYPERLINKS) == LineBits::NONE {
            return;
        }

        self.invalidate_implicit_hyperlinks_impl(seqno);
    }

    fn invalidate_implicit_hyperlinks_impl(&mut self, seqno: SequenceNo) {
        let cells = self.coerce_vec_storage();
        for cell in cells.iter_mut() {
            let replace = match cell.attrs().hyperlink() {
                Some(ref link) if link.is_implicit() => Some(Cell::new_grapheme(
                    cell.str(),
                    cell.attrs().clone().set_hyperlink(None).clone(),
                    None,
                )),
                _ => None,
            };
            if let Some(replace) = replace {
                *cell = replace;
            }
        }

        self.bits &= !LineBits::HAS_IMPLICIT_HYPERLINKS;
        self.update_last_change_seqno(seqno);
    }

    /// Scan through the line and look for sequences that match the provided
    /// rules.  Matching sequences are considered to be implicit hyperlinks
    /// and will have a hyperlink attribute associated with them.
    /// This function will only make changes if the line has been invalidated
    /// since the last time this function was called.
    /// This function does not remember the values of the `rules` slice, so it
    /// is the responsibility of the caller to call `invalidate_implicit_hyperlinks`
    /// if it wishes to call this function with different `rules`.
    pub fn scan_and_create_hyperlinks(&mut self, rules: &[Rule]) {
        if (self.bits & LineBits::SCANNED_IMPLICIT_HYPERLINKS)
            == LineBits::SCANNED_IMPLICIT_HYPERLINKS
        {
            // Has not changed since last time we scanned
            return;
        }

        // FIXME: let's build a string and a byte-to-cell map here, and
        // use this as an opportunity to rebuild HAS_HYPERLINK, skip matching
        // cells with existing non-implicit hyperlinks, and avoid matching
        // text with zero-width cells.
        self.bits |= LineBits::SCANNED_IMPLICIT_HYPERLINKS;
        self.bits &= !LineBits::HAS_IMPLICIT_HYPERLINKS;
        let line = self.as_str();

        let matches = Rule::match_hyperlinks(&line, rules);
        if matches.is_empty() {
            return;
        }

        let line = line.into_owned();
        let cells = self.coerce_vec_storage();
        if cells.scan_and_create_hyperlinks(&line, matches) {
            self.bits |= LineBits::HAS_IMPLICIT_HYPERLINKS;
        }
    }

    /// Scan through a logical line that is comprised of an array of
    /// physical lines and look for sequences that match the provided
    /// rules.  Matching sequences are considered to be implicit hyperlinks
    /// and will have a hyperlink attribute associated with them.
    /// This function will only make changes if the line has been invalidated
    /// since the last time this function was called.
    /// This function does not remember the values of the `rules` slice, so it
    /// is the responsibility of the caller to call `invalidate_implicit_hyperlinks`
    /// if it wishes to call this function with different `rules`.
    ///
    /// This function will call Line::clear_appdata on lines where
    /// hyperlinks are adjusted.
    pub fn apply_hyperlink_rules(rules: &[Rule], logical_line: &mut [&mut Line]) {
        if rules.is_empty() || logical_line.is_empty() {
            return;
        }

        let mut need_scan = false;
        for line in logical_line.iter() {
            if !line.bits.contains(LineBits::SCANNED_IMPLICIT_HYPERLINKS) {
                need_scan = true;
                break;
            }
        }
        if !need_scan {
            return;
        }

        let mut logical = logical_line[0].clone();
        for line in &logical_line[1..] {
            let seqno = logical.current_seqno().max(line.current_seqno());
            logical.append_line((**line).clone(), seqno);
        }
        let seq = logical.current_seqno();

        logical.invalidate_implicit_hyperlinks(seq);
        logical.scan_and_create_hyperlinks(rules);

        if !logical.has_hyperlink() {
            for line in logical_line.iter_mut() {
                line.bits.set(LineBits::SCANNED_IMPLICIT_HYPERLINKS, true);
                line.clear_appdata();
            }
            return;
        }

        // Re-compute the physical lines that comprise this logical line
        for phys in logical_line.iter_mut() {
            let wrapped = phys.last_cell_was_wrapped();
            let is_cluster = matches!(&phys.cells, CellStorage::C(_));
            let len = phys.len();
            let remainder = logical.split_off(len, seq);
            **phys = logical;
            logical = remainder;
            phys.set_last_cell_was_wrapped(wrapped, seq);
            phys.clear_appdata();
            if is_cluster {
                phys.compress_for_scrollback();
            }
        }
    }

    /// Returns true if the line contains a hyperlink
    #[inline]
    pub fn has_hyperlink(&self) -> bool {
        (self.bits & (LineBits::HAS_HYPERLINK | LineBits::HAS_IMPLICIT_HYPERLINKS))
            != LineBits::NONE
    }

    /// Recompose line into the corresponding utf8 string.
    pub fn as_str(&self) -> Cow<str> {
        match &self.cells {
            CellStorage::V(_) => {
                let mut s = String::new();
                for cell in self.visible_cells() {
                    s.push_str(cell.str());
                }
                Cow::Owned(s)
            },
            CellStorage::C(cl) => Cow::Borrowed(&cl.text),
        }
    }

    pub fn split_off(&mut self, idx: usize, seqno: SequenceNo) -> Self {
        let my_cells = self.coerce_vec_storage();
        // Clamp to avoid out of bounds panic if the line is shorter
        // than the requested split point
        // <https://github.com/wezterm/wezterm/issues/2355>
        let idx = idx.min(my_cells.len());
        let cells = my_cells.split_off(idx);
        Self {
            bits: self.bits,
            cells: CellStorage::V(VecStorage::new(cells)),
            seqno,
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    pub fn compute_double_click_range<F: Fn(&str) -> bool>(
        &self,
        click_col: usize,
        is_word: F,
    ) -> DoubleClickRange {
        let len = self.len();

        if click_col >= len {
            return DoubleClickRange::Range(click_col..click_col);
        }

        let mut lower = click_col;
        let mut upper = click_col;

        // TODO: look back and look ahead for cells that are hidden by
        // a preceding multi-wide cell
        let cells = self.visible_cells().collect::<Vec<_>>();
        for cell in &cells {
            if cell.cell_index() < click_col {
                continue;
            }
            if !is_word(cell.str()) {
                break;
            }
            upper = cell.cell_index() + 1;
        }
        for cell in cells.iter().rev() {
            if cell.cell_index() > click_col {
                continue;
            }
            if !is_word(cell.str()) {
                break;
            }
            lower = cell.cell_index();
        }

        if upper > lower
            && upper >= len
            && cells
                .last()
                .map(|cell| cell.attrs().wrapped())
                .unwrap_or(false)
        {
            DoubleClickRange::RangeWithWrap(lower..upper)
        } else {
            DoubleClickRange::Range(lower..upper)
        }
    }

    /// Returns a substring from the line.
    pub fn columns_as_str(&self, range: Range<usize>) -> String {
        let mut s = String::new();
        for c in self.visible_cells() {
            if c.cell_index() < range.start {
                continue;
            }
            if c.cell_index() >= range.end {
                break;
            }
            s.push_str(c.str());
        }
        s
    }

    pub fn columns_as_line(&self, range: Range<usize>) -> Self {
        let mut cells = vec![];
        for c in self.visible_cells() {
            if c.cell_index() < range.start {
                continue;
            }
            if c.cell_index() >= range.end {
                break;
            }
            cells.push(c.as_cell());
        }
        Self {
            bits: LineBits::NONE,
            cells: CellStorage::V(VecStorage::new(cells)),
            seqno: self.current_seqno(),
            zones: vec![],
            appdata: Mutex::new(None),
        }
    }

    /// If we're about to modify a cell obscured by a double-width
    /// character ahead of that cell, we need to nerf that sequence
    /// of cells to avoid partial rendering concerns.
    /// Similarly, when we assign a cell, we need to blank out those
    /// occluded successor cells.
    pub fn set_cell(&mut self, idx: usize, cell: Cell, seqno: SequenceNo) {
        self.set_cell_impl(idx, cell, false, seqno);
    }

    /// Assign a cell using grapheme text with a known width and attributes.
    /// This is a micro-optimization over first constructing a Cell from
    /// the grapheme info. If assigning this particular cell can be optimized
    /// to an append to the interal clustered storage then the cost of
    /// constructing and dropping the Cell can be avoided.
    pub fn set_cell_grapheme(
        &mut self,
        idx: usize,
        text: &str,
        width: usize,
        attr: CellAttributes,
        seqno: SequenceNo,
    ) {
        if attr.hyperlink().is_some() {
            self.bits |= LineBits::HAS_HYPERLINK;
        }

        if let CellStorage::C(cl) = &mut self.cells {
            if idx > cl.len() && text == " " && attr == CellAttributes::blank() {
                // Appending blank beyond end of line; is already
                // implicitly blank
                return;
            }
            while cl.len() < idx {
                // Fill out any implied blanks until we can append
                // their intended cell content
                cl.append_grapheme(" ", 1, CellAttributes::blank());
            }
            if idx == cl.len() {
                cl.append_grapheme(text, width, attr);
                self.invalidate_implicit_hyperlinks(seqno);
                self.invalidate_zones();
                self.update_last_change_seqno(seqno);
                return;
            }
        }

        self.set_cell(idx, Cell::new_grapheme_with_width(text, width, attr), seqno);
    }

    pub fn set_cell_clearing_image_placements(
        &mut self,
        idx: usize,
        cell: Cell,
        seqno: SequenceNo,
    ) {
        self.set_cell_impl(idx, cell, true, seqno)
    }

    fn raw_set_cell(&mut self, idx: usize, cell: Cell, clear: bool) {
        let cells = self.coerce_vec_storage();
        cells.set_cell(idx, cell, clear);
    }

    fn set_cell_impl(&mut self, idx: usize, cell: Cell, clear: bool, seqno: SequenceNo) {
        // The .max(1) stuff is here in case we get called with a
        // zero-width cell.  That shouldn't happen: those sequences
        // should get filtered out in the terminal parsing layer,
        // but in case one does sneak through, we need to ensure that
        // we grow the cells array to hold this bogus entry.
        // https://github.com/wezterm/wezterm/issues/768
        let width = cell.width().max(1);

        self.invalidate_implicit_hyperlinks(seqno);
        self.invalidate_zones();
        self.update_last_change_seqno(seqno);
        if cell.attrs().hyperlink().is_some() {
            self.bits |= LineBits::HAS_HYPERLINK;
        }

        if let CellStorage::C(cl) = &mut self.cells {
            if idx > cl.len() && cell == Cell::blank() {
                // Appending blank beyond end of line; is already
                // implicitly blank
                return;
            }
            while cl.len() < idx {
                // Fill out any implied blanks until we can append
                // their intended cell content
                cl.append_grapheme(" ", 1, CellAttributes::blank());
            }
            if idx == cl.len() {
                cl.append(cell);
                return;
            }
            /*
            log::info!(
                "cannot append {cell:?} to {:?} as idx={idx} and cl.len is {}",
                cl,
                cl.len
            );
            */
        }

        // if the line isn't wide enough, pad it out with the default attributes.
        {
            let cells = self.coerce_vec_storage();
            if idx + width > cells.len() {
                cells.resize_with(idx + width, Cell::blank);
            }
        }

        self.invalidate_grapheme_at_or_before(idx);

        // For double-wide or wider chars, ensure that the cells that
        // are overlapped by this one are blanked out.
        for i in 1..=width.saturating_sub(1) {
            self.raw_set_cell(idx + i, Cell::blank_with_attrs(cell.attrs().clone()), clear);
        }

        self.raw_set_cell(idx, cell, clear);
    }

    /// Place text starting at the specified column index.
    /// Each grapheme of the text run has the same attributes.
    pub fn overlay_text_with_attribute(
        &mut self,
        mut start_idx: usize,
        text: &str,
        attr: CellAttributes,
        seqno: SequenceNo,
    ) {
        for (i, c) in Graphemes::new(text).enumerate() {
            let cell = Cell::new_grapheme(c, attr.clone(), None);
            let width = cell.width();
            self.set_cell(i + start_idx, cell, seqno);

            // Compensate for required spacing/placement of
            // double width characters
            start_idx += width.saturating_sub(1);
        }
    }

    fn invalidate_grapheme_at_or_before(&mut self, idx: usize) {
        // Assumption: that the width of a grapheme is never > 2.
        // This constrains the amount of look-back that we need to do here.
        if idx > 0 {
            let prior = idx - 1;
            let cells = self.coerce_vec_storage();
            let width = cells[prior].width();
            if width > 1 {
                let attrs = cells[prior].attrs().clone();
                for nerf in prior..prior + width {
                    cells[nerf] = Cell::blank_with_attrs(attrs.clone());
                }
            }
        }
    }

    pub fn insert_cell(&mut self, x: usize, cell: Cell, right_margin: usize, seqno: SequenceNo) {
        self.invalidate_implicit_hyperlinks(seqno);

        let cells = self.coerce_vec_storage();
        if right_margin <= cells.len() {
            cells.remove(right_margin - 1);
        }

        if x >= cells.len() {
            cells.resize_with(x, Cell::blank);
        }

        // If we're inserting a wide cell, we should also insert the overlapped cells.
        // We insert them first so that the grapheme winds up left-most.
        let width = cell.width();
        for _ in 1..=width.saturating_sub(1) {
            cells.insert(x, Cell::blank_with_attrs(cell.attrs().clone()));
        }

        cells.insert(x, cell);
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    pub fn erase_cell(&mut self, x: usize, seqno: SequenceNo) {
        if x >= self.len() {
            // Already implicitly erased
            return;
        }
        self.invalidate_implicit_hyperlinks(seqno);
        self.invalidate_grapheme_at_or_before(x);
        {
            let cells = self.coerce_vec_storage();
            cells.remove(x);
            cells.push(Cell::default());
        }
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    pub fn remove_cell(&mut self, x: usize, seqno: SequenceNo) {
        if x >= self.len() {
            // Already implicitly removed
            return;
        }
        self.invalidate_implicit_hyperlinks(seqno);
        self.invalidate_grapheme_at_or_before(x);
        self.coerce_vec_storage().remove(x);
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    pub fn erase_cell_with_margin(
        &mut self,
        x: usize,
        right_margin: usize,
        seqno: SequenceNo,
        blank_attr: CellAttributes,
    ) {
        self.invalidate_implicit_hyperlinks(seqno);
        if x < self.len() {
            self.invalidate_grapheme_at_or_before(x);
            self.coerce_vec_storage().remove(x);
        }
        if right_margin <= self.len() + 1
        /* we just removed one */
        {
            self.coerce_vec_storage()
                .insert(right_margin - 1, Cell::blank_with_attrs(blank_attr));
        }
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    pub fn prune_trailing_blanks(&mut self, seqno: SequenceNo) {
        if let CellStorage::C(cl) = &mut self.cells {
            if cl.prune_trailing_blanks() {
                self.update_last_change_seqno(seqno);
                self.invalidate_zones();
            }
            return;
        }

        let def_attr = CellAttributes::blank();
        let cells = self.coerce_vec_storage();
        if let Some(end_idx) = cells
            .iter()
            .rposition(|c| c.str() != " " || c.attrs() != &def_attr)
        {
            cells.resize_with(end_idx + 1, Cell::blank);
            self.update_last_change_seqno(seqno);
            self.invalidate_zones();
        }
    }

    pub fn fill_range(&mut self, cols: Range<usize>, cell: &Cell, seqno: SequenceNo) {
        if self.len() == 0 && *cell == Cell::blank() {
            // We would be filling it with blanks only to prune
            // them all away again before we return; NOP
            return;
        }
        for x in cols {
            // FIXME: we can skip the look-back for second and subsequent iterations
            self.set_cell_impl(x, cell.clone(), true, seqno);
        }
        self.prune_trailing_blanks(seqno);
    }

    pub fn len(&self) -> usize {
        match &self.cells {
            CellStorage::V(cells) => cells.len(),
            CellStorage::C(cl) => cl.len(),
        }
    }

    /// Iterates the visible cells, respecting the width of the cell.
    /// For instance, a double-width cell overlaps the following (blank)
    /// cell, so that blank cell is omitted from the iterator results.
    /// The iterator yields (column_index, Cell).  Column index is the
    /// index into Self::cells, and due to the possibility of skipping
    /// the characters that follow wide characters, the column index may
    /// skip some positions.  It is returned as a convenience to the consumer
    /// as using .enumerate() on this iterator wouldn't be as useful.
    pub fn visible_cells<'a>(&'a self) -> impl Iterator<Item = CellRef<'a>> {
        match &self.cells {
            CellStorage::V(cells) => VisibleCellIter::V(VecStorageIter {
                cells: cells.iter(),
                idx: 0,
                skip_width: 0,
            }),
            CellStorage::C(cl) => VisibleCellIter::C(cl.iter()),
        }
    }

    pub fn get_cell(&self, cell_index: usize) -> Option<CellRef> {
        self.visible_cells()
            .find(|cell| cell.cell_index() == cell_index)
    }

    pub fn cluster(&self, bidi_hint: Option<ParagraphDirectionHint>) -> Vec<CellCluster> {
        CellCluster::make_cluster(self.len(), self.visible_cells(), bidi_hint)
    }

    fn make_cells(&mut self) {
        let cells = match &self.cells {
            CellStorage::V(_) => return,
            CellStorage::C(cl) => cl.to_cell_vec(),
        };
        // log::info!("make_cells\n{:?}", backtrace::Backtrace::new());
        self.cells = CellStorage::V(VecStorage::new(cells));
    }

    pub(crate) fn coerce_vec_storage(&mut self) -> &mut VecStorage {
        self.make_cells();

        match &mut self.cells {
            CellStorage::V(c) => return c,
            CellStorage::C(_) => unreachable!(),
        }
    }

    /// Adjusts the internal storage so that it occupies less
    /// space. Subsequent mutations will incur some overhead to
    /// re-materialize the storage in a form that is suitable
    /// for mutation.
    pub fn compress_for_scrollback(&mut self) {
        let cv = match &self.cells {
            CellStorage::V(v) => ClusteredLine::from_cell_vec(v.len(), self.visible_cells()),
            CellStorage::C(_) => return,
        };
        self.cells = CellStorage::C(cv);
    }

    pub fn cells_mut(&mut self) -> &mut [Cell] {
        self.coerce_vec_storage().as_mut_slice()
    }

    /// Return true if the line consists solely of whitespace cells
    pub fn is_whitespace(&self) -> bool {
        self.visible_cells().all(|c| c.str() == " ")
    }

    /// Return true if the last cell in the line has the wrapped attribute,
    /// indicating that the following line is logically a part of this one.
    pub fn last_cell_was_wrapped(&self) -> bool {
        self.visible_cells()
            .last()
            .map(|c| c.attrs().wrapped())
            .unwrap_or(false)
    }

    /// Adjust the value of the wrapped attribute on the last cell of this
    /// line.
    pub fn set_last_cell_was_wrapped(&mut self, wrapped: bool, seqno: SequenceNo) {
        self.update_last_change_seqno(seqno);
        if let CellStorage::C(cl) = &mut self.cells {
            if cl.len() == 0 {
                // Need to mark that implicit space as wrapped, so
                // explicitly add it
                cl.append(Cell::blank());
            }
            cl.set_last_cell_was_wrapped(wrapped);
            return;
        }

        let cells = self.coerce_vec_storage();
        if let Some(cell) = cells.last_mut() {
            cell.attrs_mut().set_wrapped(wrapped);
        }
    }

    /// Concatenate the cells from other with this line, appending them
    /// to this line.
    /// This function is used by rewrapping logic when joining wrapped
    /// lines back together.
    pub fn append_line(&mut self, other: Line, seqno: SequenceNo) {
        match &mut self.cells {
            CellStorage::V(cells) => {
                for cell in other.visible_cells() {
                    cells.push(cell.as_cell());
                    for _ in 1..cell.width() {
                        cells.push(Cell::new(' ', cell.attrs().clone()));
                    }
                }
            },
            CellStorage::C(cl) => {
                for cell in other.visible_cells() {
                    cl.append(cell.as_cell());
                }
            },
        }
        self.update_last_change_seqno(seqno);
        self.invalidate_zones();
    }

    /// mutable access the cell data, but the caller must take care
    /// to only mutate attributes rather than the cell textual content.
    /// Use set_cell if you need to modify the textual content of the
    /// cell, so that important invariants are upheld.
    pub fn cells_mut_for_attr_changes_only(&mut self) -> &mut [Cell] {
        self.coerce_vec_storage().as_mut_slice()
    }

    /// Given a starting attribute value, produce a series of Change
    /// entries to recreate the current line
    pub fn changes(&self, start_attr: &CellAttributes) -> Vec<Change> {
        let mut result = Vec::new();
        let mut attr = start_attr.clone();
        let mut text_run = String::new();

        for cell in self.visible_cells() {
            if *cell.attrs() == attr {
                text_run.push_str(cell.str());
            } else {
                // flush out the current text run
                if !text_run.is_empty() {
                    result.push(Change::Text(text_run.clone()));
                    text_run.clear();
                }

                attr = cell.attrs().clone();
                result.push(Change::AllAttributes(attr.clone()));
                text_run.push_str(cell.str());
            }
        }

        // flush out any remaining text run
        if !text_run.is_empty() {
            // if this is just spaces then it is likely cheaper
            // to emit ClearToEndOfLine instead.
            if attr
                == CellAttributes::default()
                    .set_background(attr.background())
                    .clone()
            {
                let left = text_run.trim_end_matches(' ').to_string();
                let num_trailing_spaces = text_run.len() - left.len();

                if num_trailing_spaces > 0 {
                    if !left.is_empty() {
                        result.push(Change::Text(left));
                    } else if result.len() == 1 {
                        // if the only queued result prior to clearing
                        // to the end of the line is an attribute change,
                        // we can prune it out and return just the line
                        // clearing operation
                        if let Change::AllAttributes(_) = result[0] {
                            result.clear()
                        }
                    }

                    // Since this function is only called in the full repaint
                    // case, and we always emit a clear screen with the default
                    // background color, we don't need to emit an instruction
                    // to clear the remainder of the line unless it has a different
                    // background color.
                    if attr.background() != Default::default() {
                        result.push(Change::ClearToEndOfLine(attr.background()));
                    }
                } else {
                    result.push(Change::Text(text_run));
                }
            } else {
                result.push(Change::Text(text_run));
            }
        }

        result
    }
}

impl<'a> From<&'a str> for Line {
    fn from(s: &str) -> Line {
        Line::from_text(s, &CellAttributes::default(), SEQ_ZERO, None)
    }
}
