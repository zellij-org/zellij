use crate::output::{CharacterChunk, KittyImageChunk, SixelImageChunk};
use crate::panes::kitty_graphics::{
    InterceptorResult, KittyApcInterceptor, KittyHostSupport, KittyImageStore,
};
use crate::panes::sixel::SixelImageStore;
use crate::panes::LinkHandler;
use crate::panes::{
    grid::{Grid, PendingNotification},
    nested_session_modal::GuestModalShortcuts,
    terminal_character::{render_first_run_banner, TerminalCharacter, EMPTY_TERMINAL_CHARACTER},
};
use crate::pty::VteBytes;
use crate::route::NotificationEnd;
use crate::tab::{AdjustedInput, GuestChoiceIndicator, Pane};
use crate::vt_parser::{self, Handler as _};
use crate::ClientId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::rc::Rc;
use std::time::{self, Instant};
use zellij_utils::data::PaneContents;
use zellij_utils::input::command::RunCommand;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::pane_size::Offset;
use zellij_utils::{
    data::{
        BareKey, BorderStyleOverride, InputMode, KeyWithModifier, Palette, PaletteColor,
        PaneId as ZellijUtilsPaneId, RegexHighlight, Style, Styling,
    },
    errors::prelude::*,
    input::layout::Run,
    nested_session::NestedSessionMessage,
    pane_size::PaneGeom,
    pane_size::SizeInPixels,
    position::Position,
    shared::make_terminal_title,
};

use crate::ui::pane_boundaries_frame::{FrameParams, PaneFrame};

pub const SELECTION_SCROLL_INTERVAL_MS: u64 = 10;

/// Most output a pane may have buffered behind an unanswered host
/// query. Past this the server stops waiting for the host reply and
/// resumes the pane as if the reply had timed out.
#[cfg(not(test))]
pub const MAX_PENDING_PTY_BYTES_WHILE_FORWARD_PAUSED: usize = 4 * 1024 * 1024;
// Smaller under test so that exceeding it does not mean pushing megabytes
// through the unoptimized grid.
#[cfg(test)]
pub const MAX_PENDING_PTY_BYTES_WHILE_FORWARD_PAUSED: usize = 256 * 1024;

/// Most unparsed output one pane may retain behind a semantic pause (an
/// unfinished repeat) or a host-forward pause. Past this, the oldest
/// unprocessed bytes are dropped with a warning, matching the scrolled
/// backlog's explicit loss policy.
// Leave headroom above the forward-pause resume threshold: the pane must be
// able to observe one chunk past that threshold before it resumes, while the
// hard ceiling still bounds a repeat-paused producer.
#[cfg(not(test))]
pub const MAX_PENDING_PTY_BYTES: usize = MAX_PENDING_PTY_BYTES_WHILE_FORWARD_PAUSED + 1024 * 1024;
#[cfg(test)]
pub const MAX_PENDING_PTY_BYTES: usize = MAX_PENDING_PTY_BYTES_WHILE_FORWARD_PAUSED + 64 * 1024;

/// Pending PTY bytes are scanned for kitty graphics APCs in blocks of
/// this size, so that resuming after a query does not rescan the whole
/// buffered remainder.
pub(crate) const PTY_SCAN_BLOCK_SIZE: usize = 16 * 1024;

// Some keys in different formats but are used in the code
const LEFT_ARROW: &[u8] = &[27, 91, 68];
const RIGHT_ARROW: &[u8] = &[27, 91, 67];
const UP_ARROW: &[u8] = &[27, 91, 65];
const DOWN_ARROW: &[u8] = &[27, 91, 66];
const HOME_KEY: &[u8] = &[27, 91, 72];
const END_KEY: &[u8] = &[27, 91, 70];
pub const BRACKETED_PASTE_BEGIN: &[u8] = &[27, 91, 50, 48, 48, 126];
pub const BRACKETED_PASTE_END: &[u8] = &[27, 91, 50, 48, 49, 126];
const ENTER_NEWLINE: &[u8] = &[10];
const ESC: &[u8] = &[27];
const ENTER_CARRIAGE_RETURN: &[u8] = &[13];
const SPACE: &[u8] = &[32];
const CTRL_C: &[u8] = &[3]; // TODO: check this to be sure it fits all types of CTRL_C (with mac, etc)
const TERMINATING_STRING: &str = "\0";
const DELETE_KEY: &str = "\u{007F}";
const BACKSPACE_KEY: &str = "\u{0008}";

/// The ansi encoding of some keys
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AnsiEncoding {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl AnsiEncoding {
    /// Returns the ANSI representation of the entries.
    /// NOTE: There is an ANSI escape code (27) at the beginning of the string,
    ///       some editors will not show this
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Left => "OD".as_bytes(),
            Self::Right => "OC".as_bytes(),
            Self::Up => "OA".as_bytes(),
            Self::Down => "OB".as_bytes(),
            Self::Home => &[27, 79, 72], // ESC O H
            Self::End => &[27, 79, 70],  // ESC O F
        }
    }

    pub fn as_vec_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Debug)]
pub enum PaneId {
    Terminal(u32),
    Plugin(u32), // FIXME: Drop the trait object, make this a wrapper for the struct?
}

// because crate architecture and reasons...
impl From<ZellijUtilsPaneId> for PaneId {
    fn from(zellij_utils_pane_id: ZellijUtilsPaneId) -> Self {
        match zellij_utils_pane_id {
            ZellijUtilsPaneId::Terminal(id) => PaneId::Terminal(id),
            ZellijUtilsPaneId::Plugin(id) => PaneId::Plugin(id),
        }
    }
}

impl Into<ZellijUtilsPaneId> for PaneId {
    fn into(self) -> ZellijUtilsPaneId {
        match self {
            PaneId::Terminal(id) => ZellijUtilsPaneId::Terminal(id),
            PaneId::Plugin(id) => ZellijUtilsPaneId::Plugin(id),
        }
    }
}

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneId::Terminal(id) => write!(f, "terminal_{}", id),
            PaneId::Plugin(id) => write!(f, "plugin_{}", id),
        }
    }
}

type IsFirstRun = bool;

// FIXME: This should hold an os_api handle so that terminal panes can set their own size via FD in
// their `reflow_lines()` method. Drop a Box<dyn ServerOsApi> in here somewhere.
#[allow(clippy::too_many_arguments)]
pub struct TerminalPane {
    pub grid: Grid,
    program_resized_at: Option<Instant>,
    pub pid: u32,
    pub selectable: bool,
    pub geom: PaneGeom,
    pub geom_override: Option<PaneGeom>,
    pub active_at: Instant,
    pub style: Style,
    vte_parser: vt_parser::Parser,
    selection_scrolled_at: time::Instant,
    content_offset: Offset,
    pane_title: String,
    pane_name: String,
    prev_pane_name: String,
    frame: HashMap<ClientId, PaneFrame>,
    borderless: bool,
    border_style_override: BorderStyleOverride,
    exclude_from_sync: bool,
    fake_cursor_locations: HashSet<(usize, usize)>, // (x, y) - these hold a record of previous fake cursors which we need to clear on render
    search_term: String,
    is_held: Option<(Option<i32>, IsFirstRun, RunCommand)>, // a "held" pane means that its command has either exited and the pane is waiting for a
    // possible user instruction to be re-run, or that the command has not yet been run
    banner: Option<String>, // a banner to be rendered inside this TerminalPane, used for panes
    // held on startup and can possibly be used to display some errors
    pane_frame_color_override: Option<(PaletteColor, Option<String>)>,
    has_bell_notification: bool,
    attention_marker: Option<crate::attention::FrameMarker>,
    attention_markers_enabled: bool,
    initial_attention_flags: Option<(bool, bool)>,
    invoked_with: Option<Run>,
    arrow_fonts: bool,
    notification_end: Option<NotificationEnd>,
    /// `true` while a host-terminal forward initiated by this pane is
    /// outstanding. While set, processing of `pending_pty_input` is
    /// suspended so that the async host reply lands on the pane's
    /// stdin in the same stream position the original query occupied.
    /// Cleared by Tab when the reply (or 500 ms cache-fallback) lands.
    forward_paused: bool,
    nested_guest: bool,
    guest_modal: HashMap<ClientId, usize>,
    guest_modal_hover: HashMap<ClientId, usize>,
    guest_choice_indicators: HashMap<ClientId, GuestChoiceIndicator>,
    guest_session_name: Option<String>,
    guest_modal_shortcuts: GuestModalShortcuts,
    /// PTY bytes that have not yet been fed to vte, starting at
    /// `pending_pty_offset` (everything before it was already consumed).
    /// While the pane is forward-paused `handle_pty_bytes` appends here;
    /// processing feeds the bytes front to back, advancing the offset
    /// past what was consumed, and stops as soon as Grid produces a
    /// forward-bound query. The remainder stays in place, so a resume
    /// continues where processing stopped instead of re-copying it.
    pending_pty_input: Vec<u8>,
    pending_pty_offset: usize,
    /// Total bytes copied into `pending_pty_input`, to let tests check
    /// that resuming does not re-copy the buffered remainder.
    #[cfg(test)]
    pending_bytes_appended: usize,
    held_escape: bool,
    kitty_interceptor: KittyApcInterceptor,
    mirror_pinned_size: Option<zellij_utils::pane_size::Size>,
    /// When this pane is a fixed one-liner in a classic stack, keep giving its
    /// program the expanded stack content size so startup ioctl is never 0×0.
    collapsed_stack_program_size: Option<zellij_utils::pane_size::Size>,
    /// User asked to scroll to an end. One quantum runs per screen turn.
    viewport_goal: Option<crate::tab::ViewportGoal>,
    /// Output is being applied at the live bottom while the user's view
    /// is held and restored in quanta. Renders stay locked until it ends.
    output_catchup: Option<OutputCatchup>,
    /// Bytes from before this pane existed were dropped. Shown in the title,
    /// never written into the PTY stream.
    output_truncated: bool,
}

struct OutputCatchup {
    anchor: crate::panes::grid::ViewportAnchor,
    phase: CatchupPhase,
    /// The held window was within one row quantum of the live bottom
    /// before this parse. Restore it before the call returns. The extra
    /// rows are ones this quantum appended, so the restore stays with the
    /// quantum instead of leaving the live tail on screen.
    restore_in_turn: bool,
}

#[derive(Clone, Copy)]
enum CatchupPhase {
    ToBottom,
    Draining,
    Restore,
}

impl Pane for TerminalPane {
    fn x(&self) -> usize {
        self.get_x()
    }
    fn y(&self) -> usize {
        self.get_y()
    }
    fn rows(&self) -> usize {
        self.get_rows()
    }
    fn cols(&self) -> usize {
        self.get_columns()
    }
    fn get_content_x(&self) -> usize {
        self.get_x() + self.content_offset.left
    }
    fn get_content_y(&self) -> usize {
        self.get_y() + self.content_offset.top
    }
    fn get_content_columns(&self) -> usize {
        // content columns might differ from the pane's columns if the pane has a frame
        // in that case they would be 2 less
        self.get_columns()
            .saturating_sub(self.content_offset.left + self.content_offset.right)
    }
    fn get_content_rows(&self) -> usize {
        // content rows might differ from the pane's rows if the pane has a frame
        // in that case they would be 2 less
        self.get_rows()
            .saturating_sub(self.content_offset.top + self.content_offset.bottom)
    }
    fn reset_size_and_position_override(&mut self) {
        self.geom_override = None;
        self.reflow_lines();
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        let is_pinned = self.geom.is_pinned;
        self.geom = position_and_size;
        self.geom.is_pinned = is_pinned;
        self.reflow_lines();
        self.render_full_viewport();
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
        self.reflow_lines();
    }
    fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        self.set_should_render(true);
        if bytes.is_empty() && (self.grid.rep_pending() || !self.has_pending_pty_input()) {
            // An empty resume with nothing buffered must not complete a
            // half-received escape with a synthetic ESC, and a paused repeat
            // is advanced by deferred work rather than by this path.
            return;
        }
        let bytes = if std::mem::take(&mut self.held_escape) {
            let mut joined = Vec::with_capacity(bytes.len() + 1);
            joined.push(0x1b);
            joined.extend_from_slice(&bytes);
            joined
        } else {
            bytes
        };
        if self.forward_paused {
            // A host-forward initiated by this pane is outstanding.
            // Buffer the bytes; Tab resumes processing them in order.
            self.append_pending_pty_input(&bytes);
            return;
        }
        self.with_live_terminal(&bytes, |pane| pane.feed_terminal_bytes(&bytes));
    }
    fn cursor_coordinates(&self, client_id: Option<ClientId>) -> Option<(usize, usize, bool)> {
        // (x, y, is_visible)
        if let Some(client_id) = client_id {
            if self.guest_modal.contains_key(&client_id) {
                return None;
            }
        }
        if self.get_content_rows() < 1 || self.get_content_columns() < 1 {
            // do not render cursor if there's no room for it
            return None;
        }
        let Offset { top, left, .. } = self.content_offset;
        self.grid
            .cursor_coordinates()
            .filter(|(x, y, _)| self.cursor_is_in_the_visible_area(*x, *y))
            .map(|(x, y, is_visible)| (x + left, y + top, is_visible))
    }
    fn is_mid_frame(&self) -> bool {
        self.grid.is_mid_frame()
    }
    fn adjust_input_to_terminal(
        &mut self,
        key_with_modifier: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
        client_id: Option<ClientId>,
    ) -> Option<AdjustedInput> {
        // there are some cases in which the terminal state means that input sent to it
        // needs to be adjusted.
        // here we match against those cases - if need be, we adjust the input and if not
        // we send back the original input

        self.reset_selection(client_id);
        if !self.grid.bracketed_paste_mode {
            // Zellij itself operates in bracketed paste mode, so the terminal sends these
            // instructions (bracketed paste start and bracketed paste end respectively)
            // when pasting input. We only need to make sure not to send them to terminal
            // panes who do not work in this mode
            match raw_input_bytes.as_slice() {
                BRACKETED_PASTE_BEGIN | BRACKETED_PASTE_END => {
                    return Some(AdjustedInput::WriteBytesToTerminal(vec![]))
                },
                _ => {},
            }
        }

        if let Some(selection) = client_id.and_then(|c| self.guest_modal.get(&c).copied()) {
            let client_id = client_id.expect("guest modal selection requires a client id");
            let is_up = key_with_modifier
                .as_ref()
                .map(|k| {
                    k.is_key_without_modifier(BareKey::Up)
                        || k.is_key_without_modifier(BareKey::Char('k'))
                })
                .unwrap_or(false)
                || raw_input_bytes.as_slice() == UP_ARROW;
            let is_down = key_with_modifier
                .as_ref()
                .map(|k| {
                    k.is_key_without_modifier(BareKey::Down)
                        || k.is_key_without_modifier(BareKey::Char('j'))
                })
                .unwrap_or(false)
                || raw_input_bytes.as_slice() == DOWN_ARROW;
            let is_enter = key_with_modifier
                .as_ref()
                .map(|k| k.is_key_without_modifier(BareKey::Enter))
                .unwrap_or(false)
                || matches!(
                    raw_input_bytes.as_slice(),
                    ENTER_CARRIAGE_RETURN | ENTER_NEWLINE
                );
            let is_esc = key_with_modifier
                .as_ref()
                .map(|k| k.is_key_without_modifier(BareKey::Esc))
                .unwrap_or(false)
                || raw_input_bytes.as_slice() == ESC;
            let digit = key_with_modifier
                .as_ref()
                .and_then(|k| match k.bare_key {
                    BareKey::Char(c @ '1'..='2') if k.key_modifiers.is_empty() => Some(c),
                    _ => None,
                })
                .or_else(|| match raw_input_bytes.as_slice() {
                    b"1" => Some('1'),
                    b"2" => Some('2'),
                    _ => None,
                });
            if is_up {
                self.guest_modal.insert(client_id, (selection + 1) % 2);
                self.set_should_render(true);
                Some(AdjustedInput::GuestModalSelectionChanged)
            } else if is_down {
                self.guest_modal.insert(client_id, (selection + 1) % 2);
                self.set_should_render(true);
                Some(AdjustedInput::GuestModalSelectionChanged)
            } else if let Some(digit) = digit {
                match digit {
                    '1' => Some(AdjustedInput::GuestModalZoom),
                    _ => Some(AdjustedInput::GuestModalDescend),
                }
            } else if is_enter {
                match selection {
                    0 => Some(AdjustedInput::GuestModalZoom),
                    _ => Some(AdjustedInput::GuestModalDescend),
                }
            } else if is_esc {
                Some(AdjustedInput::GuestModalDescend)
            } else {
                None
            }
        } else if self.is_held.is_some() {
            if key_with_modifier
                .as_ref()
                .map(|k| k.is_key_without_modifier(BareKey::Enter))
                .unwrap_or(false)
            {
                self.handle_held_run()
            } else if key_with_modifier
                .as_ref()
                .map(|k| k.is_key_without_modifier(BareKey::Esc))
                .unwrap_or(false)
            {
                self.handle_held_drop_to_shell()
            } else if key_with_modifier
                .as_ref()
                .map(|k| k.is_key_with_ctrl_modifier(BareKey::Char('c')))
                .unwrap_or(false)
            {
                Some(AdjustedInput::CloseThisPane)
            } else {
                match raw_input_bytes.as_slice() {
                    ENTER_CARRIAGE_RETURN | ENTER_NEWLINE | SPACE => self.handle_held_run(),
                    ESC => self.handle_held_drop_to_shell(),
                    CTRL_C => Some(AdjustedInput::CloseThisPane),
                    _ => None,
                }
            }
        } else {
            if self.grid.supports_kitty_keyboard_protocol {
                self.adjust_input_to_terminal_with_kitty_keyboard_protocol(
                    key_with_modifier,
                    raw_input_bytes,
                    raw_input_bytes_are_kitty,
                )
            } else {
                self.adjust_input_to_terminal_without_kitty_keyboard_protocol(
                    key_with_modifier,
                    raw_input_bytes,
                    raw_input_bytes_are_kitty,
                )
            }
        }
    }
    fn position_and_size(&self) -> PaneGeom {
        self.geom
    }
    fn current_geom(&self) -> PaneGeom {
        self.geom_override.unwrap_or(self.geom)
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.geom_override
    }
    fn should_render(&self) -> bool {
        self.grid.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
        if should_render {
            self.grid.note_change();
        }
    }
    fn render_full_viewport(&mut self) {
        // this marks the pane for a full re-render, rather than just rendering the
        // diff as it usually does with the OutputBuffer
        self.frame.clear();
        self.grid.render_full_viewport();
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn set_pane_default_colors(&mut self, fg: Option<String>, bg: Option<String>) {
        self.grid.set_pane_default_colors(fg, bg);
        self.set_should_render(true);
    }
    fn get_pane_default_colors(&self) -> (Option<String>, Option<String>) {
        self.grid.get_pane_default_color_strings()
    }
    fn render(
        &mut self,
        _client_id: Option<ClientId>,
    ) -> Result<
        Option<(
            Vec<CharacterChunk>,
            Option<String>,
            Vec<SixelImageChunk>,
            Vec<KittyImageChunk>,
        )>,
    > {
        if self.should_render() {
            let content_x = self.get_content_x();
            let content_y = self.get_content_y();
            let rows = self.get_content_rows();
            let columns = self.get_content_columns();
            if rows < 1 || columns < 1 {
                return Ok(None);
            }
            let full_redraw = self.grid.will_render_all_lines();
            match self.grid.render(content_x, content_y, &self.style) {
                Ok(rendered_assets) => {
                    if rendered_assets.is_some() {
                        self.set_should_render(false);
                    }
                    if self.mirror_pinned_size.is_some() {
                        return Ok(rendered_assets.map(
                            |(chunks, raw_vte_output, sixel_chunks, kitty_chunks)| {
                                (
                                    self.crop_pinned_chunks(chunks, full_redraw),
                                    raw_vte_output,
                                    sixel_chunks,
                                    kitty_chunks,
                                )
                            },
                        ));
                    }
                    return Ok(rendered_assets);
                },
                e => return e,
            }
        } else {
            Ok(None)
        }
    }
    fn render_frame(
        &mut self,
        client_id: ClientId,
        mut frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>> {
        let err_context = || format!("failed to render frame for client {client_id}");
        frame_params.omit_title = frame_params.omit_title
            && !(input_mode == InputMode::RenamePane && frame_params.is_main_client);
        // TODO: remove the cursor stuff from here
        let normal_title = if self.pane_name.is_empty()
            && input_mode == InputMode::RenamePane
            && frame_params.is_main_client
        {
            String::from("Enter name...")
        } else if input_mode == InputMode::EnterSearch
            && frame_params.is_main_client
            && self.search_term.is_empty()
        {
            String::from("Enter search...")
        } else if (input_mode == InputMode::EnterSearch || input_mode == InputMode::Search)
            && !self.search_term.is_empty()
        {
            let mut modifier_text = String::new();
            if self.grid.search_results.has_modifiers_set() {
                let mut modifiers = Vec::new();
                modifier_text.push_str(" [");
                if self.grid.search_results.case_insensitive {
                    modifiers.push("c")
                }
                if self.grid.search_results.whole_word_only {
                    modifiers.push("o")
                }
                if self.grid.search_results.wrap_search {
                    modifiers.push("w")
                }
                modifier_text.push_str(&modifiers.join(", "));
                modifier_text.push(']');
            }
            format!("SEARCHING: {}{}", self.search_term, modifier_text)
        } else {
            self.current_title()
        };
        let frame_params_blank_title = frame_params.blank_title;
        let pane_title = if frame_params.blank_title {
            String::new()
        } else if let Some(pinned_size) = self
            .mirror_pinned_size
            .filter(|_| self.pane_frame_color_override.is_none())
        {
            format!(
                "{} (mirrored {}x{})",
                self.title_with_bell_indicator(normal_title),
                pinned_size.cols,
                pinned_size.rows
            )
        } else if let Some(text_color_override) = self
            .pane_frame_color_override
            .as_ref()
            .and_then(|(_color, text)| text.as_ref())
        {
            text_color_override.into()
        } else {
            self.title_with_bell_indicator(normal_title)
        };

        let frame_geom = frame_params
            .frame_geom_override
            .unwrap_or_else(|| self.current_geom());
        let is_pinned = frame_geom.is_pinned;
        let mut frame = PaneFrame::new(
            frame_geom.into(),
            self.grid.scrollback_position_and_length(),
            pane_title,
            frame_params,
        )
        .is_pinned(is_pinned);
        if !frame_params_blank_title {
            frame.set_attention_marker(self.frame_attention_marker());
        }
        if let Some((exit_status, is_first_run, _run_command)) = &self.is_held {
            if *is_first_run {
                frame.indicate_first_run();
            } else {
                frame.add_exit_status(exit_status.as_ref().copied());
            }
        }
        if let Some((frame_color_override, _text)) = self.pane_frame_color_override.as_ref() {
            frame.override_color(*frame_color_override);
        }

        let res = match self.frame.get(&client_id) {
            // TODO: use and_then or something?
            Some(last_frame) => {
                if &frame != last_frame {
                    if frame.omits_title() && !last_frame.omits_title() {
                        self.grid.update_line_for_rendering(0);
                        self.set_should_render(true);
                    }
                    if !self.borderless {
                        let frame_output = frame.render().with_context(err_context)?;
                        self.frame.insert(client_id, frame);
                        Some(frame_output)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            None => {
                if !self.borderless {
                    let frame_output = frame.render().with_context(err_context)?;
                    self.frame.insert(client_id, frame);
                    Some(frame_output)
                } else {
                    None
                }
            },
        };
        Ok(res)
    }
    fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<crate::output::CharacterChunk> {
        let Some((cursor_x, cursor_y, true)) = self.cursor_coordinates() else {
            return None;
        };
        let mut character_under_cursor = self
            .grid
            .get_character_under_cursor()
            .unwrap_or(EMPTY_TERMINAL_CHARACTER);
        character_under_cursor.styles.update(|styles| {
            styles.background = Some(cursor_color.into());
            styles.foreground = Some(text_color.into());
        });
        self.fake_cursor_locations.insert((cursor_y, cursor_x));
        let mut chunk = CharacterChunk::new(
            vec![character_under_cursor],
            self.get_content_x() + cursor_x,
            self.get_content_y() + cursor_y,
        );
        // the cell under the cursor may carry one of this pane's link ids
        chunk.link_handler = Some(self.grid.link_handler.clone());
        Some(chunk)
    }
    fn render_terminal_title(&mut self, input_mode: InputMode) -> String {
        let pane_title = if self.pane_name.is_empty() && input_mode == InputMode::RenamePane {
            "Enter name..."
        } else if self.pane_name.is_empty() {
            self.grid.title.as_deref().unwrap_or("")
        } else {
            &self.pane_name
        };
        make_terminal_title(pane_title)
    }
    fn update_name(&mut self, name: &str) {
        match name {
            TERMINATING_STRING => {
                self.pane_name = String::new();
            },
            DELETE_KEY | BACKSPACE_KEY => {
                zellij_utils::graphemes::pop_cluster(&mut self.pane_name);
            },
            c => {
                self.pane_name.push_str(c);
            },
        }
        self.set_should_render(true);
    }
    fn pid(&self) -> PaneId {
        PaneId::Terminal(self.pid)
    }
    fn reduce_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows.set_percent(p - percent);
            self.set_should_render(true);
        }
    }
    fn increase_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows.set_percent(p + percent);
            self.set_should_render(true);
        }
    }
    fn reduce_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols.set_percent(p - percent);
            self.set_should_render(true);
        }
    }
    fn increase_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols.set_percent(p + percent);
            self.set_should_render(true);
        }
    }
    fn push_down(&mut self, count: usize) {
        self.geom.y += count;
        self.reflow_lines();
    }
    fn push_right(&mut self, count: usize) {
        self.geom.x += count;
        self.reflow_lines();
    }
    fn pull_left(&mut self, count: usize) {
        self.geom.x -= count;
        self.reflow_lines();
    }
    fn pull_up(&mut self, count: usize) {
        self.geom.y -= count;
        self.reflow_lines();
    }
    fn dump_screen(&self, full: bool, _client_id: Option<ClientId>) -> String {
        self.grid.dump_screen(full)
    }
    fn dump_screen_with_ansi(&self, full: bool, _client_id: Option<ClientId>) -> String {
        self.grid.dump_screen_with_ansi(full)
    }
    fn preview_with_anchor(
        &self,
        anchor: zellij_utils::data::PreviewAnchor,
        max_scrollback: usize,
    ) -> (String, Option<usize>) {
        self.grid.preview_with_anchor(anchor, max_scrollback)
    }
    fn program_resized_at(&self) -> Option<Instant> {
        self.program_resized_at
    }
    fn mark_unseen_output_start(&mut self) {
        self.grid.mark_unseen_output_start();
    }
    fn clear_screen(&mut self) {
        self.reset_grid_state();
        self.grid.mark_for_rerender();
    }
    fn scroll_up(&mut self, count: usize, _client_id: ClientId) {
        if self.nudge_catchup_anchor(-(count as isize)) {
            return;
        }
        self.grid.move_viewport_up(count);
        self.set_should_render(true);
    }
    fn scroll_down(&mut self, count: usize, _client_id: ClientId) {
        if self.nudge_catchup_anchor(count as isize) {
            return;
        }
        self.grid.move_viewport_down(count);
        self.set_should_render(true);
    }
    fn scroll_left(&mut self, _count: usize, _client_id: ClientId) {}
    fn scroll_right(&mut self, _count: usize, _client_id: ClientId) {}
    fn scroll_to_previous_prompt(&mut self, _client_id: ClientId) {
        if self.grid.scroll_to_previous_prompt() {
            self.set_should_render(true);
        }
    }
    fn scroll_to_next_prompt(&mut self, _client_id: ClientId) {
        if self.grid.scroll_to_next_prompt() {
            self.set_should_render(true);
        }
    }
    fn select_command_at_scroll_position(&mut self, _client_id: ClientId) {
        if self.grid.select_command_at_scroll_position() {
            self.set_should_render(true);
        }
    }
    fn copy_last_command_output(&mut self) -> Option<String> {
        let (output, start, end) = self.grid.last_completed_command_output()?;
        self.grid.set_command_output_flash(start, end);
        self.set_should_render(true);
        Some(output)
    }
    fn clear_command_output_flash(&mut self) {
        if self.grid.clear_command_output_flash() {
            self.set_should_render(true);
        }
    }
    fn clear_scroll(&mut self) {
        self.cancel_output_catchup();
        self.viewport_goal = None;
        self.grid.pending_search = None;
        self.grid.reset_viewport();
        self.set_should_render(true);
    }
    fn is_scrolled(&self) -> bool {
        self.grid.is_scrolled
    }
    fn begin_viewport_goal(&mut self, goal: crate::tab::ViewportGoal) {
        self.cancel_output_catchup();
        self.viewport_goal = Some(goal);
        let _ = self.advance_viewport_goal();
    }
    fn advance_viewport_goal(&mut self) -> bool {
        let Some(goal) = self.viewport_goal else {
            return false;
        };
        let done = match goal {
            crate::tab::ViewportGoal::Bottom => self
                .grid
                .advance_to_bottom(crate::panes::grid::SCREEN_ROW_QUANTUM),
            crate::tab::ViewportGoal::Top => self
                .grid
                .advance_toward_top(crate::panes::grid::SCREEN_ROW_QUANTUM),
        };
        self.set_should_render(true);
        if done {
            self.viewport_goal = None;
        }
        !done
    }
    fn viewport_goal_pending(&self) -> bool {
        self.viewport_goal.is_some()
    }
    fn has_deferred_pane_work(&self) -> bool {
        self.grid.rep_pending()
            || (!self.forward_paused && self.has_pending_pty_input())
            || self.viewport_goal.is_some()
            || self.output_catchup.is_some()
            || self.grid.pending_search.is_some()
            || self.grid.display_recount_pending()
    }
    fn advance_deferred_pane_work(&mut self) {
        if self.grid.rep_pending() {
            self.with_live_terminal(&[], |pane| {
                let done = pane.grid.continue_pending_rep();
                if done {
                    // Resume the bytes the parser left behind the repeat.
                    pane.process_pending_pty_input();
                }
            });
            self.set_should_render(true);
            return;
        }
        if !self.forward_paused && self.has_pending_pty_input() {
            self.with_live_terminal(&[], |pane| pane.process_pending_pty_input());
            self.set_should_render(true);
            return;
        }
        if self.viewport_goal.is_some() {
            let _ = self.advance_viewport_goal();
            return;
        }
        if self.output_catchup.is_some() {
            if !self.forward_paused && !self.has_pending_pty_input() {
                self.scrolled_backlog_idle();
            } else {
                let _ = self.prepare_scrolled_output(false);
            }
            return;
        }
        if self.grid.pending_search.is_some() {
            let _ = self.grid.continue_pending_search();
            self.set_should_render(true);
            return;
        }
        if self.grid.display_recount_pending() {
            let _ = self.grid.continue_display_row_recount();
        }
    }
    fn prepare_scrolled_output(&mut self, has_queued_bytes: bool) -> bool {
        if self.output_catchup.is_none() {
            if !has_queued_bytes || !self.grid.is_scrolled {
                return true;
            }
            self.grid.lock_renders();
            let near_bottom =
                self.grid.scrolled_row_distance() <= crate::panes::grid::SCREEN_ROW_QUANTUM;
            self.output_catchup = Some(OutputCatchup {
                anchor: self.grid.viewport_anchor(),
                phase: CatchupPhase::ToBottom,
                // A short user scroll stays on those rows while output is
                // applied. The gap after a parse is the rows that parse
                // appended, and the next quantum has to cross it in this
                // turn or the backlog stops draining.
                restore_in_turn: near_bottom,
            });
        }
        let phase = match self.output_catchup.as_ref().map(|catchup| &catchup.phase) {
            Some(phase) => match phase {
                CatchupPhase::ToBottom => CatchupPhase::ToBottom,
                CatchupPhase::Draining => CatchupPhase::Draining,
                CatchupPhase::Restore => CatchupPhase::Restore,
            },
            None => return true,
        };
        match phase {
            CatchupPhase::ToBottom => self.advance_catchup_to_bottom(),
            CatchupPhase::Draining => {
                if self.grid.is_scrolled {
                    self.advance_catchup_to_bottom()
                } else {
                    true
                }
            },
            CatchupPhase::Restore => {
                if has_queued_bytes {
                    if let Some(catchup) = self.output_catchup.as_mut() {
                        catchup.phase = CatchupPhase::ToBottom;
                    }
                    false
                } else {
                    self.advance_catchup_restore();
                    false
                }
            },
        }
    }
    fn scrolled_backlog_idle(&mut self) {
        let Some(phase) = self.output_catchup.as_ref().map(|catchup| catchup.phase) else {
            return;
        };
        match phase {
            CatchupPhase::ToBottom => {
                if !self.advance_catchup_to_bottom() {
                    return;
                }
            },
            CatchupPhase::Draining => {},
            CatchupPhase::Restore => {
                self.advance_catchup_restore();
                return;
            },
        }
        if let Some(catchup) = self.output_catchup.as_mut() {
            catchup.phase = CatchupPhase::Restore;
        }
        self.advance_catchup_restore();
    }
    fn restore_short_output_catchup(&mut self) {
        if self.grid.is_live_bottom_parked() {
            // The held rows never left. Putting them back is O(viewport).
            self.grid.unpark_live_bottom();
            self.set_should_render(true);
            return;
        }
        let Some(catchup) = self.output_catchup.as_ref() else {
            return;
        };
        if !catchup.restore_in_turn {
            return;
        }
        let anchor = catchup.anchor;
        // Keep the catch-up. The backlog is not idle, and the next quantum
        // has to know this view started within one row quantum of the bottom.
        let mut guard = 0;
        while self.grid.rows_to_restore_anchor(anchor).unwrap_or(0) > 0 {
            let rows = self.grid.rows_to_restore_anchor(anchor).unwrap_or(0);
            if self.grid.advance_toward_anchor(anchor, rows.max(1)) {
                break;
            }
            guard += 1;
            if guard > 8 {
                break;
            }
        }
        self.set_should_render(true);
    }

    fn active_at(&self) -> Instant {
        self.active_at
    }

    fn set_active_at(&mut self, time: Instant) {
        self.active_at = time;
    }
    fn cursor_shape_csi(&self) -> String {
        self.grid.cursor_shape().get_csi_str().to_string()
    }
    fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        self.grid.pending_messages_to_pty.drain(..).collect()
    }

    fn drain_forwarded_queries(&mut self) -> Vec<crate::host_query::HostQuery> {
        self.grid.clear_forward_hold();
        self.grid.pending_forwarded_queries.drain(..).collect()
    }

    fn drain_nested_session_messages(&mut self) -> Vec<NestedSessionMessage> {
        self.grid
            .pending_nested_session_messages
            .drain(..)
            .collect()
    }

    fn is_nested_guest(&self) -> bool {
        self.nested_guest
    }

    fn set_is_nested_guest(&mut self, is_nested_guest: bool) {
        self.nested_guest = is_nested_guest;
    }

    fn set_guest_modal(&mut self, client_ids: &[ClientId]) {
        for client_id in client_ids {
            self.guest_modal.insert(*client_id, 0);
        }
        self.set_should_render(true);
    }

    fn clear_guest_modal(&mut self, client_id: ClientId) {
        self.guest_modal_hover.remove(&client_id);
        if self.guest_modal.remove(&client_id).is_some() {
            self.render_full_viewport();
            self.set_should_render(true);
        }
    }

    fn clear_all_guest_modals(&mut self) {
        self.guest_modal_hover.clear();
        if !self.guest_modal.is_empty() {
            self.guest_modal.clear();
            self.render_full_viewport();
            self.set_should_render(true);
        }
    }

    fn guest_modal_selection(&self, client_id: ClientId) -> Option<usize> {
        self.guest_modal.get(&client_id).copied()
    }

    fn guest_modal_hover(&self, client_id: ClientId) -> Option<usize> {
        self.guest_modal_hover.get(&client_id).copied()
    }

    fn set_guest_modal_hover(&mut self, client_id: ClientId, option: Option<usize>) -> bool {
        let option = option.filter(|_| self.guest_modal.contains_key(&client_id));
        if self.guest_modal_hover.get(&client_id).copied() == option {
            return false;
        }
        match option {
            Some(option) => {
                self.guest_modal_hover.insert(client_id, option);
            },
            None => {
                self.guest_modal_hover.remove(&client_id);
            },
        }
        self.set_should_render(true);
        true
    }

    fn has_guest_modal_for_any_client(&self) -> bool {
        !self.guest_modal.is_empty()
    }

    fn set_guest_choice_indicator(
        &mut self,
        client_id: ClientId,
        indicator: Option<GuestChoiceIndicator>,
    ) {
        let previous = self.guest_choice_indicators.get(&client_id).copied();
        if previous == indicator {
            return;
        }
        match indicator {
            Some(indicator) => {
                self.guest_choice_indicators.insert(client_id, indicator);
            },
            None => {
                self.guest_choice_indicators.remove(&client_id);
            },
        }
        self.set_should_render(true);
    }

    fn guest_choice_indicator(&self, client_id: ClientId) -> Option<GuestChoiceIndicator> {
        self.guest_choice_indicators.get(&client_id).copied()
    }

    fn clear_all_guest_choice_indicators(&mut self) {
        if !self.guest_choice_indicators.is_empty() {
            self.guest_choice_indicators.clear();
            self.set_should_render(true);
        }
    }

    fn set_guest_session_name(&mut self, session_name: Option<String>) {
        self.guest_session_name = session_name;
    }

    fn guest_session_name(&self) -> Option<String> {
        self.guest_session_name.clone()
    }

    fn set_guest_modal_shortcuts(&mut self, shortcuts: GuestModalShortcuts) {
        self.guest_modal_shortcuts = shortcuts;
    }

    fn guest_modal_shortcuts(&self) -> GuestModalShortcuts {
        self.guest_modal_shortcuts.clone()
    }

    fn arm_forward_pause(&mut self) {
        self.forward_paused = true;
    }

    fn clear_forward_pause(&mut self) -> bool {
        let was_paused = self.forward_paused;
        self.forward_paused = false;
        was_paused
    }

    fn drain_pending_pty_input(&mut self) -> Vec<u8> {
        let mut pending = std::mem::take(&mut self.pending_pty_input);
        pending.drain(..self.pending_pty_offset);
        self.pending_pty_offset = 0;
        pending
    }

    fn has_pending_pty_input(&self) -> bool {
        self.pending_pty_offset < self.pending_pty_input.len()
    }

    fn has_pending_output(&self) -> bool {
        self.has_pending_pty_input() || self.grid.rep_pending()
    }

    fn can_accept_pty_bytes(&self) -> bool {
        // A forward pause appends into the bounded pending buffer, so the
        // backlog still drains. An unfinished repeat or an already-buffered
        // suffix must run first, otherwise the backlog would only grow.
        self.forward_paused || (!self.has_pending_pty_input() && !self.grid.rep_pending())
    }

    fn pending_pty_input_exceeds_forward_limit(&self) -> bool {
        self.forward_paused
            && self
                .pending_pty_input
                .len()
                .saturating_sub(self.pending_pty_offset)
                > MAX_PENDING_PTY_BYTES_WHILE_FORWARD_PAUSED
    }

    fn is_forward_paused(&self) -> bool {
        self.forward_paused
    }

    fn push_color_palette_dsr(&mut self, mode: zellij_utils::data::HostTerminalThemeMode) {
        self.grid.push_color_palette_dsr(mode);
    }

    fn drain_clipboard_update(&mut self) -> Option<String> {
        self.grid.pending_clipboard_update.take()
    }

    fn drain_desktop_notifications(&mut self) -> Vec<PendingNotification> {
        self.grid.pending_desktop_notifications.drain(..).collect()
    }

    fn drain_osc7_cwd(&mut self) -> Option<std::path::PathBuf> {
        self.grid.pending_osc7_cwd.take()
    }

    fn drain_prompt_return(&mut self) -> bool {
        std::mem::take(&mut self.grid.pending_prompt_return)
    }
    fn reset_input_modes_left_by_lost_guest(&mut self) {
        self.grid.reset_input_modes_left_by_lost_guest();
    }

    fn set_selection_options(&mut self, osc133_command_selection: bool, word_separators: &str) {
        self.grid
            .set_selection_options(osc133_command_selection, word_separators);
    }

    fn start_selection(&mut self, start: &Position, _client_id: ClientId) {
        self.grid.start_selection(start);
        self.set_should_render(true);
    }

    fn update_selection(&mut self, to: &Position, _client_id: ClientId) {
        let should_scroll = self.selection_scrolled_at.elapsed()
            >= time::Duration::from_millis(SELECTION_SCROLL_INTERVAL_MS);
        let cursor_at_the_bottom = to.line.0 < 0 && should_scroll;
        let cursor_at_the_top = to.line.0 as usize >= self.grid.height && should_scroll;
        let cursor_in_the_middle = to.line.0 >= 0 && (to.line.0 as usize) < self.grid.height;

        // TODO: check how far up/down mouse is relative to pane, to increase scroll lines?
        if cursor_at_the_bottom {
            self.grid.scroll_up_one_line();
            self.selection_scrolled_at = time::Instant::now();
            self.set_should_render(true);
        } else if cursor_at_the_top {
            self.grid.scroll_down_one_line();
            self.selection_scrolled_at = time::Instant::now();
            self.set_should_render(true);
        } else if cursor_in_the_middle {
            // here we'll only render if the selection was updated, and that'll be handled by the
            // grid
            self.grid.update_selection(to);
        }
    }

    fn end_selection(&mut self, end: &Position, _client_id: ClientId) {
        self.grid.end_selection(end);
        self.set_should_render(true);
    }

    fn reset_selection(&mut self, _client_id: Option<ClientId>) {
        self.grid.reset_selection();
    }

    fn get_selected_text(&self, _client_id: ClientId) -> Option<String> {
        self.grid.get_selected_text()
    }

    fn set_frame(&mut self, _frame: bool) {
        self.frame.clear();
    }

    fn set_content_offset(&mut self, offset: Offset) {
        self.content_offset = offset;
        self.reflow_lines();
    }

    fn get_content_offset(&self) -> Offset {
        self.content_offset
    }

    fn store_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.prev_pane_name = self.pane_name.clone()
        }
    }
    fn load_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.pane_name = self.prev_pane_name.clone()
        }
    }

    fn borderless(&self) -> bool {
        self.borderless
    }
    fn set_borderless(&mut self, should_be_borderless: bool) {
        self.borderless = should_be_borderless;
        if should_be_borderless {
            self.set_content_offset(Offset::default());
        } else {
            self.set_content_offset(Offset::frame(1));
        }
    }
    fn set_border_style_override(&mut self, border_style_override: BorderStyleOverride) {
        self.border_style_override = border_style_override;
        self.frame.clear();
        self.set_should_render(true);
    }
    fn border_style_override(&self) -> BorderStyleOverride {
        self.border_style_override
    }

    fn set_exclude_from_sync(&mut self, exclude_from_sync: bool) {
        self.exclude_from_sync = exclude_from_sync;
    }

    fn exclude_from_sync(&self) -> bool {
        self.exclude_from_sync
    }

    fn mouse_event(&self, event: &MouseEvent, _client_id: ClientId) -> Option<String> {
        self.grid.mouse_event_signal(event)
    }

    fn mouse_left_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_left_click_signal(position, is_held)
    }
    fn mouse_left_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_left_click_release_signal(position)
    }
    fn mouse_right_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_right_click_signal(position, is_held)
    }
    fn mouse_right_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_right_click_release_signal(position)
    }
    fn mouse_middle_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_middle_click_signal(position, is_held)
    }
    fn mouse_middle_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_middle_click_release_signal(position)
    }
    fn mouse_scroll_up(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_up_signal(position)
    }
    fn mouse_scroll_down(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_down_signal(position)
    }
    fn mouse_scroll_left(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_left_signal(position)
    }
    fn mouse_scroll_right(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_right_signal(position)
    }
    fn focus_event(&self) -> Option<String> {
        self.grid.focus_event()
    }
    fn unfocus_event(&self) -> Option<String> {
        self.grid.unfocus_event()
    }
    fn get_line_number(&self) -> Option<usize> {
        // + 1 because the absolute position in the scrollback is 0 indexed and this should be 1 indexed
        Some(self.grid.absolute_position_in_scrollback() + 1)
    }

    fn update_search_term(&mut self, needle: &str) {
        match needle {
            TERMINATING_STRING => {
                self.search_term = String::new();
            },
            DELETE_KEY | BACKSPACE_KEY => {
                zellij_utils::graphemes::pop_cluster(&mut self.search_term);
            },
            c => {
                self.search_term.push_str(c);
            },
        }
        self.grid.clear_search();
        if !self.search_term.is_empty() {
            self.grid.set_search_string(&self.search_term);
        }
        self.set_should_render(true);
    }
    fn search_down(&mut self) {
        if self.search_term.is_empty() {
            return; // No-op
        }
        self.grid.search_down();
        self.set_should_render(true);
    }
    fn search_up(&mut self) {
        if self.search_term.is_empty() {
            return; // No-op
        }
        self.grid.search_up();
        self.set_should_render(true);
    }
    fn toggle_search_case_sensitivity(&mut self) {
        self.grid.toggle_search_case_sensitivity();
        self.set_should_render(true);
    }
    fn toggle_search_whole_words(&mut self) {
        self.grid.toggle_search_whole_words();
        self.set_should_render(true);
    }
    fn toggle_search_wrap(&mut self) {
        self.grid.toggle_search_wrap();
    }
    fn clear_search(&mut self) {
        self.grid.clear_search();
        self.search_term.clear();
    }
    fn is_alternate_mode_active(&self) -> bool {
        self.grid.is_alternate_mode_active()
    }
    fn hold(&mut self, exit_status: Option<i32>, is_first_run: bool, run_command: RunCommand) {
        self.invoked_with = Some(Run::Command(run_command.clone()));
        self.is_held = Some((exit_status, is_first_run, run_command));
        if let Some(notification_end) = self.notification_end.as_mut() {
            if let Some(exit_status) = exit_status {
                notification_end.set_exit_status(exit_status);

                // Check if unblock condition is met
                if let Some(condition) = notification_end.unblock_condition() {
                    if condition.is_met(exit_status) {
                        // Condition met - drop the NotificationEnd now to unblock
                        drop(self.notification_end.take());
                    }
                }
            }
        }
        if is_first_run {
            self.render_first_run_banner();
        }
        self.set_should_render(true);
    }
    fn has_bell(&self) -> bool {
        self.grid.ring_bell
    }
    fn consume_bell(&mut self) {
        self.grid.ring_bell = false;
    }
    fn osc7_payload(&self) -> Option<&str> {
        self.grid.osc7_payload()
    }
    fn set_bell_notification(&mut self, val: bool) {
        self.has_bell_notification = val;
    }
    fn get_bell_notification(&self) -> bool {
        self.has_bell_notification
    }
    fn add_red_pane_frame_color_override(&mut self, error_text: Option<String>) -> bool {
        let new_override = Some((self.style.colors.exit_code_error.base, error_text));
        let changed = self.pane_frame_color_override != new_override;
        self.pane_frame_color_override = new_override;
        changed
    }
    fn add_highlight_pane_frame_color_override(
        &mut self,
        text: Option<String>,
        _client_id: Option<ClientId>,
    ) -> bool {
        // TODO: if we have a client_id, we should only highlight the frame for this client
        let new_override = Some((self.style.colors.frame_highlight.emphasis_0, text));
        let changed = self.pane_frame_color_override != new_override;
        self.pane_frame_color_override = new_override;
        changed
    }
    fn clear_pane_frame_color_override(&mut self, _client_id: Option<ClientId>) -> bool {
        // TODO: if we have a client_id, we should only clear the highlight for this client
        let changed = self.pane_frame_color_override.is_some();
        self.pane_frame_color_override = None;
        changed
    }
    fn frame_color_override(&self) -> Option<PaletteColor> {
        self.pane_frame_color_override
            .as_ref()
            .map(|(color, _text)| *color)
    }
    fn invoked_with(&self) -> &Option<Run> {
        &self.invoked_with
    }
    fn take_notification_end(&mut self) -> Option<NotificationEnd> {
        self.notification_end.take()
    }
    fn attach_exit_waiter(&mut self, mut waiter: NotificationEnd) -> Option<NotificationEnd> {
        if self.exited() {
            if let Some(exit_status) = self.exit_status() {
                waiter.set_exit_status(exit_status);
            }
            drop(waiter);
            return None;
        }
        if self.notification_end.is_some() {
            return Some(waiter);
        }
        self.notification_end = Some(waiter);
        None
    }
    fn set_title(&mut self, title: String) {
        self.pane_title = title;
    }
    fn current_title(&self) -> String {
        let mut title = if self.pane_name.is_empty() {
            self.grid
                .title
                .as_deref()
                .unwrap_or(&self.pane_title)
                .to_owned()
        } else {
            self.pane_name.to_owned()
        };
        if self.output_truncated {
            title.push_str(" [output cut off]");
        }
        if self.grid.history_was_truncated() {
            title.push_str(" [history cut off]");
        }
        title
    }
    fn note_output_truncated(&mut self) {
        if !self.output_truncated {
            self.output_truncated = true;
            self.set_should_render(true);
        }
    }
    fn discard_unfinished_input(&mut self) {
        self.reset_unfinished_input_state();
        // Bytes the prefix left queued must not be fed next to the output
        // that follows the gap.
        self.pending_pty_input.clear();
        self.pending_pty_offset = 0;
    }
    fn reject_unparsed_input(&mut self) {
        self.reset_unfinished_input_state();
        self.pending_pty_input.clear();
        self.pending_pty_offset = 0;
        self.note_output_truncated();
        self.scrolled_backlog_idle();
    }
    fn stack_list_entry_label(&self) -> String {
        match self.attention_marker_text() {
            Some(marker) => format!("{} {}", self.current_title(), marker),
            None => self.title_with_bell_indicator(self.current_title()),
        }
    }
    fn set_attention_marker(
        &mut self,
        marker: Option<crate::attention::FrameMarker>,
        enabled: bool,
    ) {
        if self.attention_marker != marker || self.attention_markers_enabled != enabled {
            self.attention_marker = marker;
            self.attention_markers_enabled = enabled;
            self.set_should_render(true);
        }
    }
    fn drain_pane_signals(&mut self) -> Vec<crate::attention::PaneSignal> {
        self.grid.shell_reports.drain()
    }
    fn set_initial_attention_flags(&mut self, muted: bool, ignore_output: bool) {
        self.initial_attention_flags = Some((muted, ignore_output));
    }
    fn take_initial_attention_flags(&mut self) -> Option<(bool, bool)> {
        self.initial_attention_flags.take()
    }
    fn custom_title(&self) -> Option<String> {
        if self.pane_name.is_empty() {
            None
        } else {
            Some(self.pane_name.clone())
        }
    }
    fn has_explicit_title(&self) -> bool {
        !self.pane_name.is_empty() || self.grid.title.is_some()
    }
    fn scroll_position(&self) -> (usize, usize) {
        self.grid.scrollback_position_and_length()
    }
    fn viewport_scroll_state(&self) -> Option<(usize, u64)> {
        self.grid.viewport_scroll_state()
    }
    fn exit_status(&self) -> Option<i32> {
        self.is_held
            .as_ref()
            .and_then(|(exit_status, _, _)| *exit_status)
    }
    fn is_held(&self) -> bool {
        self.is_held.is_some()
    }
    fn exited(&self) -> bool {
        match self.is_held {
            Some((_, is_first_run, _)) => !is_first_run,
            None => false,
        }
    }
    fn rename(&mut self, buf: Vec<u8>) {
        self.pane_name = zellij_utils::shared::clean_string_from_control_and_linebreak(
            &String::from_utf8_lossy(&buf),
        );
        self.set_should_render(true);
    }
    fn serialize(&self, scrollback_lines_to_serialize: Option<usize>) -> Option<String> {
        self.grid.serialize(scrollback_lines_to_serialize)
    }
    fn rerun(&mut self) -> Option<RunCommand> {
        // if this is a command pane that has exited or is waiting to be rerun, will return its
        // RunCommand, otherwise it is safe to assume this is not the right sort of pane or that it
        // is not in the right sort of state
        self.is_held.take().map(|(_, _, run_command)| {
            self.is_held = None;
            self.reset_grid_state();
            self.set_should_render(true);
            self.remove_banner();
            run_command.clone()
        })
    }
    fn update_theme(&mut self, theme: Styling) {
        self.style.colors = theme.clone();
        self.grid.update_theme(theme);
        if self.banner.is_some() {
            // we do this so that the banner will be updated with the new theme colors
            self.render_first_run_banner();
        }
    }
    fn update_arrow_fonts(&mut self, should_support_arrow_fonts: bool) {
        self.arrow_fonts = should_support_arrow_fonts;
        self.grid.update_arrow_fonts(should_support_arrow_fonts);
    }
    fn update_kitty_host_support(&mut self, supported: KittyHostSupport) {
        self.grid.update_kitty_host_support(supported);
    }
    fn update_sixel_host_support(&mut self, supported: bool) {
        self.grid.update_sixel_host_support(supported);
    }
    fn update_rounded_corners(&mut self, rounded_corners: bool) {
        self.style.rounded_corners = rounded_corners;
        self.frame.clear();
    }
    fn invalidate_frame_cache(&mut self) {
        self.frame.clear();
        self.set_should_render(true);
    }
    fn drain_fake_cursors(&mut self) -> Option<HashSet<(usize, usize)>> {
        if !self.fake_cursor_locations.is_empty() {
            for (y, _x) in &self.fake_cursor_locations {
                // we do this because once these fake_cursor_locations
                // have been drained, we have to make sure to render the line
                // they appeared on so that whatever clears their location
                // won't leave a hole
                self.grid.update_line_for_rendering(*y);
            }
            Some(self.fake_cursor_locations.drain().collect())
        } else {
            None
        }
    }
    fn toggle_pinned(&mut self) {
        self.geom.is_pinned = !self.geom.is_pinned;
    }
    fn set_pinned(&mut self, should_be_pinned: bool) {
        self.geom.is_pinned = should_be_pinned;
    }
    fn intercept_left_mouse_click(&mut self, position: &Position, client_id: ClientId) -> bool {
        if self.position_is_on_frame(position) {
            let relative_position = self.relative_position(position);
            if let Some(client_frame) = self.frame.get_mut(&client_id) {
                if client_frame.clicked_on_pinned(relative_position) {
                    self.toggle_pinned();
                    return true;
                }
            }
        }
        false
    }
    fn position_is_on_pin_button(&self, position: &Position, client_id: ClientId) -> bool {
        if self.position_is_on_frame(position) {
            let relative_position = self.relative_position(position);
            if let Some(client_frame) = self.frame.get(&client_id) {
                return client_frame.clicked_on_pinned(relative_position);
            }
        }
        false
    }
    fn intercept_mouse_event_on_frame(&mut self, event: &MouseEvent, client_id: ClientId) -> bool {
        if self.position_is_on_frame(&event.position) {
            let relative_position = self.relative_position(&event.position);
            if let MouseEventType::Press = event.event_type {
                if let Some(client_frame) = self.frame.get_mut(&client_id) {
                    if client_frame.clicked_on_pinned(relative_position) {
                        self.toggle_pinned();
                        return true;
                    }
                }
            }
        }
        false
    }
    fn reset_logical_position(&mut self) {
        self.geom.logical_position = None;
    }
    fn pane_contents(
        &self,
        _client_id: Option<ClientId>,
        get_full_scrollback: bool,
        max_scrollback_lines: Option<usize>,
    ) -> PaneContents {
        self.grid
            .pane_contents(get_full_scrollback, max_scrollback_lines)
    }
    fn pane_contents_with_ansi(
        &self,
        _client_id: Option<ClientId>,
        get_full_scrollback: bool,
        max_scrollback_lines: Option<usize>,
    ) -> PaneContents {
        self.grid
            .pane_contents_with_ansi(get_full_scrollback, max_scrollback_lines)
    }
    fn update_exit_status(&mut self, exit_status: i32) {
        if let Some(notification_end) = self.notification_end.as_mut() {
            notification_end.set_exit_status(exit_status);
            // Check if unblock condition is met
            if let Some(condition) = notification_end.unblock_condition() {
                if condition.is_met(exit_status) {
                    // Condition met - drop the NotificationEnd now to unblock
                    drop(self.notification_end.take());
                }
            }
        }
    }
    fn set_plugin_regex_highlights(
        &mut self,
        plugin_id: u32,
        highlights: Vec<RegexHighlight>,
        style: &Style,
    ) {
        self.grid
            .set_plugin_regex_highlights(plugin_id, highlights, style);
        self.set_should_render(true);
    }
    fn clear_plugin_highlights(&mut self, plugin_id: u32) {
        self.grid.clear_plugin_highlights(plugin_id);
        self.set_should_render(true);
    }
    fn set_hover_position(&mut self, position: Option<Position>) -> bool {
        let changed = self.grid.set_hover_position(position);
        if changed {
            self.set_should_render(true);
        }
        changed
    }
    fn cached_hover_tooltip(&self) -> Option<String> {
        self.grid.cached_hover_tooltip.clone()
    }
    fn plugin_highlight_at(
        &self,
        position: &Position,
    ) -> Option<(
        u32,
        String,
        String,
        std::collections::BTreeMap<String, String>,
    )> {
        self.grid.plugin_highlight_at(position)
    }
    fn terminal_emulator_wants_mouse(&self) -> bool {
        self.grid.mouse_tracking != crate::panes::grid::MouseTracking::Off
    }
    fn mirror_screen(&mut self) -> Option<zellij_utils::pane_mirror::MirrorScreen> {
        self.grid.link_urls_in_changed_lines();
        Some(self.grid.mirror_screen())
    }
    fn mouse_event_for_program(&self, event: &MouseEvent) -> Option<String> {
        self.grid.mouse_event_signal(event)
    }
    fn program_columns(&self) -> usize {
        self.mirror_pinned_size
            .map(|size| size.cols)
            .or_else(|| self.collapsed_stack_program_size.map(|size| size.cols))
            .unwrap_or_else(|| self.get_content_columns())
    }
    fn program_rows(&self) -> usize {
        self.mirror_pinned_size
            .map(|size| size.rows)
            .or_else(|| self.collapsed_stack_program_size.map(|size| size.rows))
            .unwrap_or_else(|| self.get_content_rows())
    }
    fn mirror_pinned_size(&self) -> Option<zellij_utils::pane_size::Size> {
        self.mirror_pinned_size
    }
    fn set_mirror_pinned_size(&mut self, size: Option<zellij_utils::pane_size::Size>) -> bool {
        if self.mirror_pinned_size == size {
            return false;
        }
        self.mirror_pinned_size = size;
        self.reflow_lines();
        self.render_full_viewport();
        true
    }
    fn set_collapsed_stack_program_size(&mut self, size: Option<zellij_utils::pane_size::Size>) {
        self.collapsed_stack_program_size = size;
    }
    fn mirror_change_counter(&self) -> u64 {
        self.grid.change_counter
    }
}

impl TerminalPane {
    /// Apply terminal operations against the live model even when the user
    /// is looking at history. The held view is restored afterwards, so a
    /// deferred REP quantum and the bytes behind it cannot land in history.
    fn reset_unfinished_input_state(&mut self) {
        self.held_escape = false;
        self.kitty_interceptor.reset();
        self.vte_parser.discard_unfinished();
        self.grid.abandon_unfinished_control_sequence();
    }

    fn bytes_may_place_image(bytes: &[u8]) -> bool {
        const INTRODUCERS: [&[u8]; 3] = [b"\x1bP", b"\x1b_", b"\x1b]1337;"];
        INTRODUCERS
            .iter()
            .any(|introducer| memchr::memmem::find(bytes, introducer).is_some())
    }

    fn input_is_parkable(&self, bytes: &[u8]) -> bool {
        let queued = &self.pending_pty_input[self.pending_pty_offset..];
        // An introducer can straddle the queue/new-chunk boundary.
        const BOUNDARY: usize = 6;
        let queued_len = queued.len().min(BOUNDARY);
        let new_len = bytes.len().min(BOUNDARY);
        let mut joined = [0u8; BOUNDARY * 2];
        joined[..queued_len].copy_from_slice(&queued[queued.len() - queued_len..]);
        joined[queued_len..queued_len + new_len].copy_from_slice(&bytes[..new_len]);
        self.kitty_interceptor.is_ground()
            && self.vte_parser.is_ground()
            && !self.held_escape
            && !Self::bytes_may_place_image(bytes)
            && !Self::bytes_may_place_image(queued)
            && !Self::bytes_may_place_image(&joined[..queued_len + new_len])
    }

    fn with_live_terminal<R>(
        &mut self,
        input_bytes: &[u8],
        operation: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let allow_park = self.input_is_parkable(input_bytes);
        if !self.grid.is_scrolled && !self.grid.is_live_bottom_parked() {
            return operation(self);
        }
        if allow_park {
            if self.grid.is_live_bottom_parked() || self.grid.park_at_live_bottom() {
                let result = operation(self);
                if self.grid.is_live_bottom_parked() {
                    self.grid.unpark_live_bottom();
                }
                return result;
            }
        } else if self.grid.is_live_bottom_parked() {
            // A park created by an earlier text quantum is not permission to
            // parse an image-capable suffix against the historical window.
            self.grid.unpark_live_bottom();
        }
        let alternate_before = self.grid.is_alternate_mode_active();
        let viewport_anchor = self.grid.viewport_anchor();
        self.grid.reset_viewport();
        let result = operation(self);
        if self.grid.is_alternate_mode_active() == alternate_before {
            self.grid.restore_viewport_anchor(viewport_anchor);
        }
        result
    }

    fn feed_terminal_bytes(&mut self, bytes: &[u8]) {
        self.grid.begin_terminal_input_turn();
        if self.pending_pty_offset == self.pending_pty_input.len() {
            // Nothing is queued ahead of these bytes: feed them straight
            // from the caller's buffer and keep only what was not
            // consumed.
            let (consumed, replay) = self.feed_pty_bytes(bytes);
            if !replay.is_empty() {
                self.append_pending_pty_input(&replay);
            }
            if consumed < bytes.len() {
                self.append_pending_pty_input(&bytes[consumed..]);
            }
        } else {
            self.append_pending_pty_input(bytes);
            self.process_pending_pty_input();
        }
    }

    fn cancel_output_catchup(&mut self) {
        if self.grid.is_live_bottom_parked() {
            self.grid.unpark_live_bottom();
        }
        if self.output_catchup.take().is_some() {
            self.grid.unlock_renders();
        }
    }

    fn reset_grid_state(&mut self) {
        self.cancel_output_catchup();
        self.viewport_goal = None;
        self.forward_paused = false;
        self.frame.clear();
        self.fake_cursor_locations.clear();
        self.discard_unfinished_input();
        self.grid.clear_forward_hold();
        self.grid.pending_forwarded_queries.clear();
        self.grid.pending_messages_to_pty.clear();
        self.grid.pending_clipboard_update = None;
        self.grid.pending_osc7_cwd = None;
        self.grid.pending_prompt_return = false;
        self.grid.pending_desktop_notifications.clear();
        self.grid.pending_nested_session_messages.clear();
        self.grid.unlock_renders();
        self.grid.reset_terminal_state();
        #[cfg(test)]
        {
            self.pending_bytes_appended = 0;
        }
    }

    /// Record a user scroll against the held anchor. Returns true when a
    /// catch-up owns the grid and the scroll was not applied to it.
    fn nudge_catchup_anchor(&mut self, rows: isize) -> bool {
        let Some(catchup) = self.output_catchup.as_mut() else {
            return false;
        };
        catchup.anchor = catchup.anchor.nudge(rows);
        true
    }

    fn advance_catchup_to_bottom(&mut self) -> bool {
        if self.grid.is_live_bottom_parked() || self.grid.park_at_live_bottom() {
            if let Some(catchup) = self.output_catchup.as_mut() {
                catchup.phase = CatchupPhase::Draining;
            }
            return true;
        }
        let restore_in_turn = self
            .output_catchup
            .as_ref()
            .is_some_and(|catchup| catchup.restore_in_turn);
        // Parking was refused (an image still tracks scrollback coordinates).
        // A deep scroll still moves one row quantum. A catch-up that started
        // near the bottom has to cross the rows this parse will append, or
        // the backlog stops draining.
        let budget = if restore_in_turn {
            usize::MAX
        } else {
            crate::panes::grid::SCREEN_ROW_QUANTUM
        };
        let at_bottom = self.grid.advance_to_bottom(budget);
        if at_bottom {
            if let Some(catchup) = self.output_catchup.as_mut() {
                catchup.phase = CatchupPhase::Draining;
            }
        }
        at_bottom
    }
    fn advance_catchup_restore(&mut self) {
        if self.grid.is_live_bottom_parked() {
            self.grid.unpark_live_bottom();
        }
        let Some(anchor) = self.output_catchup.as_ref().and_then(|catchup| {
            matches!(catchup.phase, CatchupPhase::Restore).then_some(catchup.anchor)
        }) else {
            return;
        };
        // The parked path left the user's rows in place. Finish when that
        // window is the anchor. A nudge moves one quantum and does not walk
        // the rows under the viewport.
        if !self.grid.lines_below.is_empty() {
            if self
                .grid
                .nudge_toward_anchor(anchor, crate::panes::grid::SCREEN_ROW_QUANTUM)
            {
                self.grid.unlock_renders();
                self.output_catchup = None;
            }
            self.set_should_render(true);
            return;
        }
        let restore_in_turn = self
            .output_catchup
            .as_ref()
            .is_some_and(|catchup| catchup.restore_in_turn);
        let budget = if restore_in_turn {
            self.grid.rows_to_restore_anchor(anchor).unwrap_or(0).max(1)
        } else {
            crate::panes::grid::SCREEN_ROW_QUANTUM
        };
        if self.grid.advance_toward_anchor(anchor, budget) {
            self.grid.unlock_renders();
            self.output_catchup = None;
        }
        self.set_should_render(true);
    }

    #[cfg(test)]
    pub(crate) fn pending_bytes_appended(&self) -> usize {
        self.pending_bytes_appended
    }

    #[cfg(test)]
    pub(crate) fn handle_pty_bytes_for_test(&mut self, bytes: Vec<u8>) {
        self.handle_pty_bytes(bytes);
    }

    fn append_pending_pty_input(&mut self, bytes: &[u8]) {
        #[cfg(test)]
        {
            self.pending_bytes_appended += bytes.len();
        }
        let unparsed = self
            .pending_pty_input
            .len()
            .saturating_sub(self.pending_pty_offset);
        if unparsed.saturating_add(bytes.len()) > MAX_PENDING_PTY_BYTES {
            // Keep the decoder and the bytes it sees on the same side of the
            // gap. A suffix of a control string is not independent output.
            let dropped = unparsed.saturating_add(bytes.len());
            self.reject_unparsed_input();
            log::warn!(
                "pane rejected {dropped} bytes of buffered output at the \
                 {MAX_PENDING_PTY_BYTES} byte ceiling and reset the parser"
            );
            return;
        }
        self.pending_pty_input.extend_from_slice(bytes);
    }

    /// Feed the queued PTY bytes to vte front to back. Only the bytes
    /// that were consumed are skipped, so the unprocessed remainder is
    /// never copied: each byte is fed once, however many forward-bound
    /// queries interrupt the stream.
    fn process_pending_pty_input(&mut self) {
        self.grid.begin_terminal_input_turn();
        let mut buffer = std::mem::take(&mut self.pending_pty_input);
        let start = self.pending_pty_offset;
        let (consumed, replay) = self.feed_pty_bytes(&buffer[start..]);
        let mut offset = start + consumed;
        if !replay.is_empty() {
            buffer.splice(offset..offset, replay);
        }
        if offset == buffer.len() {
            buffer.clear();
            offset = 0;
            if buffer.capacity() > PTY_SCAN_BLOCK_SIZE * 4 {
                // Don't keep a multi-megabyte allocation per pane after a
                // burst of buffered output.
                buffer = Vec::new();
            }
        } else if offset >= buffer.len() / 2 {
            // Reclaim the consumed prefix once it is at least as large as
            // what remains, so the cost is paid for by the consumed bytes.
            buffer.drain(..offset);
            offset = 0;
        }
        self.pending_pty_input = buffer;
        self.pending_pty_offset = offset;
    }

    /// Feed `bytes` through the kitty graphics interceptor and the vte
    /// parser. Parsing continues through text and further queries that can
    /// share one host round trip, and stops before a sequence that would
    /// write to the pane's stdin or that needs its own reply. Returns how
    /// many leading bytes were consumed. If stopping in the middle of bytes
    /// the interceptor was already holding from an earlier call, those bytes
    /// are returned as well; they have to be put back in front of the
    /// unconsumed remainder.
    fn feed_pty_bytes(&mut self, bytes: &[u8]) -> (usize, Vec<u8>) {
        let mut pos = 0;
        while pos < bytes.len() {
            if self.grid.paused() {
                return (pos, Vec::new());
            }
            let end = (pos + PTY_SCAN_BLOCK_SIZE).min(bytes.len());
            let block = &bytes[pos..end];
            if self.kitty_interceptor.is_ground() && memchr::memmem::find(block, b"\x1b_").is_none()
            {
                let ends_in_escape = block.last() == Some(&0x1b);
                let head = if ends_in_escape {
                    &block[..block.len() - 1]
                } else {
                    block
                };
                let consumed = self.vte_parser.feed_until_paused(&mut self.grid, head);
                pos += consumed;
                if consumed < head.len() {
                    if self.grid.paused() || !self.grid.batch_is_open() {
                        return (pos, Vec::new());
                    }
                    // The batch stopped on an escape this block does not
                    // finish. The slow path below sees the rest of `bytes`.
                } else {
                    if ends_in_escape && end == bytes.len() {
                        self.held_escape = true;
                        pos = end;
                    }
                    continue;
                }
            }
            let mut index = pos;
            while index < end {
                if self.grid.paused() {
                    return (index, Vec::new());
                }
                if self.kitty_interceptor.is_ground() {
                    let rest = &bytes[index..end];
                    let run = rest
                        .iter()
                        .position(|byte| *byte == 0x1b)
                        .unwrap_or(rest.len());
                    if run > 0 {
                        let consumed = self
                            .vte_parser
                            .feed_until_paused(&mut self.grid, &rest[..run]);
                        index += consumed;
                        if consumed < run {
                            return (index, Vec::new());
                        }
                        continue;
                    }
                }
                if self.kitty_interceptor.is_ground()
                    && self.grid.hold_open_batch_before(&bytes[index..])
                {
                    return (index, Vec::new());
                }
                let byte = bytes[index];
                index += 1;
                match self.kitty_interceptor.advance(byte) {
                    InterceptorResult::Forward(forwarded) => {
                        let forwarded = forwarded.as_slice();
                        let consumed = self.vte_parser.feed_until_paused(&mut self.grid, forwarded);
                        if consumed < forwarded.len() {
                            // Paused part way through bytes the interceptor
                            // had been holding. Replay the rest from a
                            // clean interceptor state, including the
                            // current byte if the interceptor kept it.
                            let holds_current_byte =
                                byte == 0x1b && !self.kitty_interceptor.is_ground();
                            let mut replay = forwarded[consumed..].to_vec();
                            if holds_current_byte {
                                replay.push(0x1b);
                            }
                            self.kitty_interceptor.reset();
                            return if index >= replay.len() {
                                (index - replay.len(), Vec::new())
                            } else {
                                (index, replay)
                            };
                        }
                    },
                    InterceptorResult::Swallow => {},
                    InterceptorResult::Captured(cmd) => {
                        self.grid.handle_kitty_apc(&cmd);
                    },
                }
            }
            pos = index;
        }
        (bytes.len(), Vec::new())
    }

    fn title_with_bell_indicator(&self, title: String) -> String {
        if self.has_bell_notification && !self.attention_markers_enabled {
            format!("{} [!]", title)
        } else {
            title
        }
    }
    fn attention_marker_text(&self) -> Option<String> {
        let marker = self.attention_marker.as_ref()?;
        let symbol = if marker.muted {
            if self.arrow_fonts {
                "⊘"
            } else {
                "-"
            }
        } else {
            marker.status.marker(!self.arrow_fonts)
        };
        if symbol.is_empty() {
            None
        } else {
            Some(symbol.to_owned())
        }
    }
    fn frame_attention_marker(
        &self,
    ) -> Option<crate::ui::pane_boundaries_frame::FrameAttentionMarker> {
        use zellij_utils::attention::AttentionStatus;
        let text = self.attention_marker_text()?;
        let marker = self.attention_marker.as_ref()?;
        let colors = &self.style.colors;
        let (color, dim) = if marker.muted {
            (colors.text_unselected.base, true)
        } else {
            match marker.status {
                AttentionStatus::NeedsYou => (colors.text_unselected.emphasis_0, false),
                AttentionStatus::Failed => (colors.exit_code_error.base, false),
                AttentionStatus::Done => (colors.exit_code_success.base, false),
                AttentionStatus::Working => (colors.text_unselected.emphasis_1, false),
                _ => (colors.text_unselected.base, true),
            }
        };
        Some(crate::ui::pane_boundaries_frame::FrameAttentionMarker {
            text,
            color: Some(color),
            dim,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pid: u32,
        position_and_size: PaneGeom,
        style: Style,
        pane_index: usize,
        pane_name: String,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        kitty_image_store: Rc<RefCell<KittyImageStore>>,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        debug: bool,
        arrow_fonts: bool,
        styled_underlines: bool,
        osc8_hyperlinks: bool,
        explicitly_disable_keyboard_protocol: bool,
        mut notification_end: Option<NotificationEnd>,
    ) -> TerminalPane {
        let initial_pane_title =
            initial_pane_title.unwrap_or_else(|| format!("Pane #{}", pane_index));
        let grid = Grid::new(
            position_and_size.rows.as_usize(),
            position_and_size.cols.as_usize(),
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            Rc::new(RefCell::new(LinkHandler::new())),
            character_cell_size,
            sixel_image_store,
            kitty_image_store,
            style.clone(),
            debug,
            arrow_fonts,
            styled_underlines,
            osc8_hyperlinks,
            explicitly_disable_keyboard_protocol,
        );
        if let Some(notification_end) = notification_end.as_mut() {
            notification_end.set_affected_pane_id(PaneId::Terminal(pid));
        }
        TerminalPane {
            frame: HashMap::new(),
            content_offset: Offset::default(),
            pid,
            grid,
            selectable: true,
            geom: position_and_size,
            geom_override: None,
            vte_parser: vt_parser::Parser::new(),
            active_at: Instant::now(),
            style,
            selection_scrolled_at: time::Instant::now(),
            pane_title: initial_pane_title,
            pane_name: pane_name.clone(),
            prev_pane_name: pane_name,
            borderless: false,
            border_style_override: BorderStyleOverride::default(),
            exclude_from_sync: false,
            fake_cursor_locations: HashSet::new(),
            search_term: String::new(),
            is_held: None,
            banner: None,
            pane_frame_color_override: None,
            has_bell_notification: false,
            attention_marker: None,
            attention_markers_enabled: true,
            initial_attention_flags: None,
            invoked_with,
            arrow_fonts,
            notification_end,
            forward_paused: false,
            nested_guest: false,
            guest_modal: HashMap::new(),
            guest_modal_hover: HashMap::new(),
            guest_choice_indicators: HashMap::new(),
            guest_session_name: None,
            guest_modal_shortcuts: GuestModalShortcuts::default(),
            pending_pty_input: Vec::new(),
            pending_pty_offset: 0,
            #[cfg(test)]
            pending_bytes_appended: 0,
            held_escape: false,
            kitty_interceptor: KittyApcInterceptor::new(),
            mirror_pinned_size: None,
            collapsed_stack_program_size: None,
            viewport_goal: None,
            output_catchup: None,
            output_truncated: false,
            program_resized_at: None,
        }
    }
    pub fn get_x(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.x,
            None => self.geom.x,
        }
    }
    pub fn get_y(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.y,
            None => self.geom.y,
        }
    }
    pub fn get_columns(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.cols.as_usize(),
            None => self.geom.cols.as_usize(),
        }
    }
    pub fn get_rows(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.rows.as_usize(),
            None => self.geom.rows.as_usize(),
        }
    }
    fn reflow_lines(&mut self) {
        let rows = self.program_rows();
        let cols = self.program_columns();
        if (self.grid.height, self.grid.width) != (rows, cols) {
            self.program_resized_at = Some(Instant::now());
        }
        // One recount quantum in this call. The screen loop continues the
        // rest, so a resize does not scan the whole scrollback before the
        // next pane can run. `Grid::change_size` still finishes, for callers
        // that read the length immediately.
        self.grid.resize_keeping_recount_open(rows, cols);
        if self.banner.is_some() {
            self.reset_grid_state();
            self.render_first_run_banner();
        }
        self.set_should_render(true);
    }
    pub fn with_grapheme_clusters(mut self, grapheme_clusters: bool) -> Self {
        self.grid.set_grapheme_mode_by_default(grapheme_clusters);
        self
    }
    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.grid.as_character_lines()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize, bool)> {
        // (x, y, is_visible)
        if self.get_content_rows() < 1 || self.get_content_columns() < 1 {
            // do not render cursor if there's no room for it
            return None;
        }
        self.grid
            .cursor_coordinates()
            .filter(|(x, y, _)| self.cursor_is_in_the_visible_area(*x, *y))
    }
    fn cursor_is_in_the_visible_area(&self, x: usize, y: usize) -> bool {
        self.mirror_pinned_size.is_none()
            || (x < self.get_content_columns() && y < self.get_content_rows())
    }
    fn crop_pinned_chunks(
        &self,
        chunks: Vec<CharacterChunk>,
        full_redraw: bool,
    ) -> Vec<CharacterChunk> {
        crop_and_pad_chunks(
            chunks,
            self.get_content_x(),
            self.get_content_y(),
            self.get_content_columns(),
            self.get_content_rows(),
            self.grid.width,
            self.grid.height,
            full_redraw,
        )
    }
    fn render_first_run_banner(&mut self) {
        let columns = self.get_content_columns();
        let rows = self.get_content_rows();
        let banner = match &self.is_held {
            Some((_exit_status, _is_first_run, run_command)) => {
                render_first_run_banner(columns, rows, &self.style, Some(run_command))
            },
            None => render_first_run_banner(columns, rows, &self.style, None),
        };
        self.banner = Some(banner.clone());
        self.handle_pty_bytes(banner.as_bytes().to_vec());
    }
    fn remove_banner(&mut self) {
        if self.banner.is_some() {
            self.reset_grid_state();
            self.set_should_render(true);
            self.banner = None;
        }
    }
    fn adjust_input_to_terminal_with_kitty_keyboard_protocol(
        &self,
        key: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
    ) -> Option<AdjustedInput> {
        if raw_input_bytes_are_kitty || key.is_none() {
            Some(AdjustedInput::WriteBytesToTerminal(raw_input_bytes))
        } else {
            // here what happens is that the host terminal is operating in non "kitty keys" mode, but
            // this terminal pane *is* operating in "kitty keys" mode - so we need to serialize the "non kitty"
            // key to a "kitty key"
            key.as_ref()
                .and_then(|k| k.serialize_kitty())
                .map(|s| AdjustedInput::WriteBytesToTerminal(s.as_bytes().to_vec()))
        }
    }
    fn adjust_input_to_terminal_without_kitty_keyboard_protocol(
        &self,
        key: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
    ) -> Option<AdjustedInput> {
        if self.grid.new_line_mode {
            let key_is_enter = raw_input_bytes.as_slice() == &[13]
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Enter))
                    .unwrap_or(false);
            if key_is_enter {
                // LNM - carriage return is followed by linefeed
                return Some(AdjustedInput::WriteBytesToTerminal(
                    "\u{0d}\u{0a}".as_bytes().to_vec(),
                ));
            };
        }
        if self.grid.cursor_key_mode {
            let key_is_left_arrow = raw_input_bytes.as_slice() == LEFT_ARROW
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Left))
                    .unwrap_or(false);
            let key_is_right_arrow = raw_input_bytes.as_slice() == RIGHT_ARROW
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Right))
                    .unwrap_or(false);
            let key_is_up_arrow = raw_input_bytes.as_slice() == UP_ARROW
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Up))
                    .unwrap_or(false);
            let key_is_down_arrow = raw_input_bytes.as_slice() == DOWN_ARROW
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Down))
                    .unwrap_or(false);
            let key_is_home_key = raw_input_bytes.as_slice() == HOME_KEY
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::Home))
                    .unwrap_or(false);
            let key_is_end_key = raw_input_bytes.as_slice() == END_KEY
                || key
                    .as_ref()
                    .map(|k| k.is_key_without_modifier(BareKey::End))
                    .unwrap_or(false);
            if key_is_left_arrow {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::Left.as_vec_bytes(),
                ));
            } else if key_is_right_arrow {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::Right.as_vec_bytes(),
                ));
            } else if key_is_up_arrow {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::Up.as_vec_bytes(),
                ));
            } else if key_is_down_arrow {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::Down.as_vec_bytes(),
                ));
            } else if key_is_home_key {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::Home.as_vec_bytes(),
                ));
            } else if key_is_end_key {
                return Some(AdjustedInput::WriteBytesToTerminal(
                    AnsiEncoding::End.as_vec_bytes(),
                ));
            }
        }
        if raw_input_bytes_are_kitty {
            // here what happens is that the host terminal is operating in "kitty keys" mode, but
            // this terminal pane is not - so we need to serialize the kitty key to "non kitty" if
            // possible - if not possible (eg. with multiple modifiers), we'll return a None here
            // and write nothing to the terminal pane
            key.as_ref()
                .and_then(|k| k.serialize_non_kitty())
                .map(|s| AdjustedInput::WriteBytesToTerminal(s.as_bytes().to_vec()))
        } else {
            Some(AdjustedInput::WriteBytesToTerminal(raw_input_bytes))
        }
    }
    fn handle_held_run(&mut self) -> Option<AdjustedInput> {
        self.is_held.take().map(|(_, _, run_command)| {
            self.is_held = None;
            self.reset_grid_state();
            self.set_should_render(true);
            self.remove_banner();
            AdjustedInput::ReRunCommandInThisPane(run_command.clone())
        })
    }
    fn handle_held_drop_to_shell(&mut self) -> Option<AdjustedInput> {
        self.is_held.take().map(|(_, _, run_command)| {
            // Drop to shell in the same working directory as the command was run
            let working_dir = run_command.cwd.clone();
            self.is_held = None;
            self.reset_grid_state();
            self.set_should_render(true);
            self.remove_banner();
            AdjustedInput::DropToShellInThisPane { working_dir }
        })
    }
}

pub(crate) fn crop_and_pad_chunks(
    chunks: Vec<CharacterChunk>,
    x: usize,
    y: usize,
    cols: usize,
    rows: usize,
    grid_cols: usize,
    grid_rows: usize,
    full_redraw: bool,
) -> Vec<CharacterChunk> {
    let mut cropped = Vec::with_capacity(chunks.len());
    for mut chunk in chunks {
        if chunk.y < y || chunk.y >= y + rows || chunk.x >= x + cols {
            continue;
        }
        let available = x + cols - chunk.x;
        let mut kept_width = 0;
        let mut kept = 0;
        for character in &chunk.terminal_characters {
            if kept_width + character.width() > available {
                break;
            }
            kept_width += character.width();
            kept += 1;
        }
        if kept < chunk.terminal_characters.len() {
            chunk.terminal_characters.truncate(kept);
            for _ in kept_width..available {
                chunk.terminal_characters.push(EMPTY_TERMINAL_CHARACTER);
            }
        }
        cropped.push(chunk);
    }
    if full_redraw {
        if grid_cols < cols {
            for row in 0..grid_rows.min(rows) {
                cropped.push(CharacterChunk::new(
                    vec![EMPTY_TERMINAL_CHARACTER; cols - grid_cols],
                    x + grid_cols,
                    y + row,
                ));
            }
        }
        for row in grid_rows.min(rows)..rows {
            cropped.push(CharacterChunk::new(
                vec![EMPTY_TERMINAL_CHARACTER; cols],
                x,
                y + row,
            ));
        }
    }
    cropped
}

#[cfg(test)]
#[path = "./unit/hostile_output_tests.rs"]
mod hostile_output_tests;

#[cfg(test)]
#[path = "./unit/terminal_pane_tests.rs"]
mod grid_tests;

#[cfg(test)]
#[path = "./unit/search_in_pane_tests.rs"]
mod search_tests;
