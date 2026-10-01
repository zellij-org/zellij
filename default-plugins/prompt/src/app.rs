use std::time::{Duration, Instant};
use zellij_tile::prelude::*;
use zellij_utils::prompt::{EXIT_ANSWERED, EXIT_CANCELLED, EXIT_ERROR};

use crate::outcome::{default_answer, reply_for, Outcome};
use crate::request::{parse_request, split_lines, Request, Spec};
use crate::ui::{
    hints_text, hints_width, text_width, truncate, Hint, Screen, Step, MAX_WIDTH, MIN_WIDTH,
};

const FOOTER_ROWS: usize = 1;
const EAGER_RENDER_ITEMS: usize = 50;
const RENDER_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    BlockPipe(String),
    SetExitCode(String, i32),
    Output(String, String),
    UnblockPipe(String),
    SetPopupSize(usize, usize),
    SetTimeout(f64),
    ShowSelf,
    CloseSelf,
    FocusPane(PaneId),
    WatchNames,
}

#[derive(Default)]
pub struct App {
    pipe_id: Option<String>,
    request: Option<Request>,
    screen: Option<Screen>,
    deadline: Option<Instant>,
    finished: bool,
    released: bool,
    last_popup_size: Option<(usize, usize)>,
    last_input_render: Option<Instant>,
    effects: Vec<Effect>,
}

impl App {
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn handle_pipe(&mut self, pipe_message: PipeMessage, now: Instant) -> bool {
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(pipe_id) => pipe_id.clone(),
            _ => return false,
        };
        if self.finished {
            return false;
        }
        match &self.pipe_id {
            None => self.start(pipe_id, pipe_message, now),
            Some(current) if *current == pipe_id => self.continue_input(pipe_message, now),
            Some(_) => {
                self.effects.push(Effect::Output(
                    pipe_id.clone(),
                    "zellij prompt: this prompt is already answering another request\n".to_owned(),
                ));
                self.effects
                    .push(Effect::SetExitCode(pipe_id.clone(), EXIT_ERROR));
                self.effects.push(Effect::UnblockPipe(pipe_id));
                false
            },
        }
    }
    fn start(&mut self, pipe_id: String, pipe_message: PipeMessage, now: Instant) -> bool {
        self.pipe_id = Some(pipe_id.clone());
        let request = match parse_request(
            &pipe_message.name,
            &pipe_message.args,
            pipe_message.payload.as_deref(),
        ) {
            Ok(request) => request,
            Err(error) => {
                self.effects.push(Effect::BlockPipe(pipe_id));
                self.finish(Outcome::Error(error));
                return false;
            },
        };
        if matches!(request.spec, Spec::Notify { .. }) {
            return self.start_notice(pipe_id, request, now);
        }
        let is_streaming = matches!(
            request.spec,
            Spec::Choose {
                streaming: true,
                ..
            }
        );
        if !is_streaming {
            self.effects.push(Effect::BlockPipe(pipe_id.clone()));
        }
        self.effects
            .push(Effect::SetExitCode(pipe_id, EXIT_CANCELLED));
        if !request.common.in_popup {
            self.effects.push(Effect::ShowSelf);
        }
        if let Some(timeout) = request.common.timeout {
            self.deadline = Some(now + timeout);
            self.schedule_tick(now);
        }
        self.screen = Some(Screen::new(&request));
        self.request = Some(request);
        self.request_popup_size();
        true
    }
    fn start_notice(&mut self, pipe_id: String, request: Request, now: Instant) -> bool {
        self.effects
            .push(Effect::SetExitCode(pipe_id.clone(), EXIT_ANSWERED));
        self.effects.push(Effect::UnblockPipe(pipe_id));
        self.released = true;
        if !request.common.in_popup {
            self.effects.push(Effect::ShowSelf);
        }
        if let Some(timeout) = request.common.timeout {
            self.deadline = Some(now + timeout);
            self.schedule_tick(now);
        }
        let screen = Screen::new(&request);
        if screen.watches_names() {
            self.effects.push(Effect::WatchNames);
        }
        self.screen = Some(screen);
        self.request = Some(request);
        self.request_popup_size();
        true
    }
    pub fn handle_pane_update(&mut self, pane_manifest: &PaneManifest) -> bool {
        let changed = !self.finished
            && self
                .screen
                .as_mut()
                .map(|screen| screen.update_panes(pane_manifest))
                .unwrap_or(false);
        if changed {
            self.request_popup_size();
        }
        changed
    }
    pub fn handle_tab_update(&mut self, tabs: &[TabInfo]) -> bool {
        let changed = !self.finished
            && self
                .screen
                .as_mut()
                .map(|screen| screen.update_tabs(tabs))
                .unwrap_or(false);
        if changed {
            self.request_popup_size();
        }
        changed
    }
    fn close_notice(&mut self) {
        self.finished = true;
        self.effects.push(Effect::CloseSelf);
    }
    fn continue_input(&mut self, pipe_message: PipeMessage, now: Instant) -> bool {
        let Some(pipe_id) = self.pipe_id.clone() else {
            return false;
        };
        let (is_choose, null, labels) = match self.request.as_ref().map(|r| &r.spec) {
            Some(Spec::Choose { null, labels, .. }) => (true, *null, *labels),
            _ => (false, false, false),
        };
        match pipe_message.payload {
            Some(payload) if is_choose => {
                let count = match self.screen.as_mut() {
                    Some(Screen::Choose(screen)) => {
                        for item in split_lines(&payload, null, labels) {
                            screen.add_item(item);
                        }
                        screen.received_count()
                    },
                    _ => 0,
                };
                let render_is_due = self
                    .last_input_render
                    .map(|last| now.saturating_duration_since(last) >= RENDER_INTERVAL)
                    .unwrap_or(true);
                let should_render = count <= EAGER_RENDER_ITEMS || render_is_due;
                if should_render {
                    self.last_input_render = Some(now);
                    self.request_popup_size();
                }
                should_render
            },
            Some(_) => {
                self.effects.push(Effect::BlockPipe(pipe_id));
                false
            },
            None => {
                self.effects.push(Effect::BlockPipe(pipe_id));
                if let Some(screen) = self.screen.as_mut() {
                    screen.input_ended();
                }
                self.request_popup_size();
                true
            },
        }
    }
    fn schedule_tick(&mut self, now: Instant) {
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(now);
            let next = remaining.min(Duration::from_secs(1));
            self.effects
                .push(Effect::SetTimeout(next.as_secs_f64().max(0.01)));
        }
    }
    pub fn handle_timer(&mut self, now: Instant) -> bool {
        if self.finished {
            return false;
        }
        let mut changed = self
            .screen
            .as_mut()
            .map(|screen| screen.handle_timer())
            .unwrap_or(false);
        if let Some(deadline) = self.deadline {
            if now >= deadline && self.released {
                self.close_notice();
                return false;
            }
            if now >= deadline {
                let answer = self
                    .request
                    .as_ref()
                    .map(|request| default_answer(request))
                    .unwrap_or(Ok(None));
                match answer {
                    Ok(answer) => self.finish(Outcome::TimedOut(answer)),
                    Err(error) => self.finish(Outcome::Error(error)),
                }
                return false;
            }
            self.schedule_tick(now);
            changed = true;
        }
        changed
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.finished {
            return false;
        }
        let step = match self.screen.as_mut() {
            Some(screen) => screen.handle_key(&key),
            None => Step::Nothing,
        };
        self.apply_step(step)
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if self.finished {
            return false;
        }
        let step = match self.screen.as_mut() {
            Some(screen) => screen.handle_mouse(mouse),
            None => Step::Nothing,
        };
        self.apply_step(step)
    }
    fn apply_step(&mut self, step: Step) -> bool {
        match step {
            Step::Nothing => false,
            Step::Redraw => {
                self.request_popup_size();
                true
            },
            Step::Done(outcome) => {
                self.finish(outcome);
                false
            },
            Step::FocusPane(pane_id) => {
                self.effects.push(Effect::FocusPane(pane_id));
                self.close_notice();
                false
            },
        }
    }
    fn finish(&mut self, outcome: Outcome) {
        if self.released {
            self.close_notice();
            return;
        }
        let Some(pipe_id) = self.pipe_id.clone() else {
            return;
        };
        let (json, null) = match &self.request {
            Some(request) => (
                request.common.json,
                matches!(request.spec, Spec::Choose { null: true, .. }),
            ),
            None => (false, false),
        };
        let reply = reply_for(&outcome, json, null);
        if !reply.output.is_empty() {
            self.effects
                .push(Effect::Output(pipe_id.clone(), reply.output));
        }
        self.effects
            .push(Effect::SetExitCode(pipe_id.clone(), reply.exit_code));
        self.effects.push(Effect::UnblockPipe(pipe_id));
        self.finished = true;
        self.effects.push(Effect::CloseSelf);
    }
    fn has_confirm_footer(&self) -> bool {
        self.screen
            .as_ref()
            .map(|screen| screen.has_own_frame())
            .unwrap_or(false)
    }
    fn is_framed(&self) -> bool {
        let in_popup = self
            .request
            .as_ref()
            .map(|request| request.common.in_popup)
            .unwrap_or(false);
        let has_own_frame = self
            .screen
            .as_ref()
            .map(|screen| screen.has_own_frame())
            .unwrap_or(false);
        in_popup && !has_own_frame
    }
    fn header_title(&self) -> Option<&str> {
        let request = self.request.as_ref()?;
        let has_own_frame = self
            .screen
            .as_ref()
            .map(|screen| screen.has_own_frame())
            .unwrap_or(false);
        if self.is_framed() || has_own_frame {
            None
        } else {
            request.common.title.as_deref()
        }
    }
    fn frame_title(&self) -> Option<&str> {
        if self.is_framed() {
            self.request.as_ref()?.common.title.as_deref()
        } else {
            None
        }
    }
    fn hints(&self) -> &'static [Hint] {
        self.screen
            .as_ref()
            .map(|screen| screen.hints())
            .unwrap_or(&[])
    }
    fn status(&self, now: Instant) -> Option<String> {
        let mut parts = vec![];
        if let Some(status) = self.screen.as_ref().and_then(|screen| screen.status()) {
            parts.push(status);
        }
        if let Some(deadline) = self.deadline {
            let remaining = deadline.saturating_duration_since(now).as_secs_f64().ceil();
            parts.push(format!("{}s left", remaining as u64));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }
    fn footer_width(&self) -> usize {
        hints_width(self.hints(), self.status(Instant::now()).as_deref())
    }
    fn is_notice(&self) -> bool {
        self.screen
            .as_ref()
            .map(|screen| screen.is_notice())
            .unwrap_or(false)
    }
    pub fn desired_size(&self) -> Option<(usize, usize)> {
        let screen = self.screen.as_ref()?;
        if screen.is_notice() {
            return Some(screen.desired_size());
        }
        let (body_width, body_height) = screen.desired_size();
        let header_rows = if self.header_title().is_some() { 1 } else { 0 };
        let title = self
            .header_title()
            .or(self.frame_title())
            .map(|t| text_width(t) + 4)
            .unwrap_or(0);
        let footer = if self.has_confirm_footer() {
            self.footer_width() + 6
        } else {
            self.footer_width()
        };
        let width = body_width
            .max(title)
            .max(footer.min(MAX_WIDTH))
            .clamp(MIN_WIDTH, MAX_WIDTH);
        let footer_rows = if self.has_confirm_footer() {
            0
        } else {
            FOOTER_ROWS
        };
        let height = header_rows + body_height + footer_rows;
        if self.is_framed() {
            Some((width + 4, height + 2))
        } else {
            Some((width, height))
        }
    }
    fn request_popup_size(&mut self) {
        if let Some(size) = self.desired_size() {
            if self.last_popup_size != Some(size) {
                self.last_popup_size = Some(size);
                self.effects.push(Effect::SetPopupSize(size.0, size.1));
            }
        }
    }
    fn render_frame(&self, rows: usize, cols: usize) {
        if rows < 2 || cols < 2 {
            return;
        }
        let inner = cols - 2;
        let top = match self.frame_title() {
            Some(title) if inner >= 6 => {
                let title = truncate(title, inner - 4);
                let title_width = text_width(&title);
                let line = format!(
                    "╭─ {} {}╮",
                    title,
                    "─".repeat(inner.saturating_sub(title_width + 3))
                );
                Text::new(line).color_range(2, 3..3 + title_width)
            },
            _ => Text::new(format!("╭{}╮", "─".repeat(inner))),
        };
        print_text_with_coordinates(top, 0, 0, Some(cols), None);
        for row in 1..rows - 1 {
            print_text_with_coordinates(Text::new("│"), 0, row, Some(1), None);
            print_text_with_coordinates(Text::new("│"), cols - 1, row, Some(1), None);
        }
        print_text_with_coordinates(
            Text::new(format!("╰{}╯", "─".repeat(inner))),
            0,
            rows - 1,
            Some(cols),
            None,
        );
    }
    pub fn render(&mut self, rows: usize, cols: usize, now: Instant) {
        if self.finished || rows == 0 || cols == 0 {
            return;
        }
        if self.is_notice() {
            if let Some(screen) = self.screen.as_mut() {
                screen.render(0, 0, cols, rows);
            }
            return;
        }
        let (x, y, width, height) = if self.is_framed() && rows > 2 && cols > 4 {
            self.render_frame(rows, cols);
            (2, 1, cols - 4, rows - 2)
        } else {
            (0, 0, cols, rows)
        };
        let mut header_rows = 0;
        if let Some(title) = self.header_title() {
            let title = truncate(title, width);
            let length = title.chars().count();
            print_text_with_coordinates(
                Text::new(title).color_range(0, 0..length),
                x,
                y,
                Some(width),
                None,
            );
            header_rows = 1;
        }
        let status = self.status(now);
        if self.has_confirm_footer() {
            let body_height = height.saturating_sub(header_rows).max(1);
            if let Some(screen) = self.screen.as_mut() {
                screen.render(x, y + header_rows, width, body_height);
            }
            if let Some(area) = self
                .screen
                .as_ref()
                .and_then(|screen| screen.confirm_footer_area())
            {
                print_text_with_coordinates(
                    hints_text(self.hints(), status.as_deref(), area.width),
                    area.x,
                    area.y + area.height - 1,
                    Some(area.width),
                    None,
                );
            }
        } else {
            let body_height = height.saturating_sub(header_rows + FOOTER_ROWS).max(1);
            if let Some(screen) = self.screen.as_mut() {
                screen.render(x, y + header_rows, width, body_height);
            }
            if height > header_rows + 1 {
                print_text_with_coordinates(
                    hints_text(self.hints(), status.as_deref(), width),
                    x,
                    y + height - 1,
                    Some(width),
                    None,
                );
            }
        }
        if let Some(screen) = self.screen.as_mut() {
            screen.render_overlays(rows, cols);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn message(
        pipe_id: &str,
        name: &str,
        args: &[(&str, &str)],
        payload: Option<&str>,
    ) -> PipeMessage {
        PipeMessage {
            source: PipeSource::Cli(pipe_id.to_owned()),
            name: name.to_owned(),
            payload: payload.map(|p| p.to_owned()),
            args: args
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .chain(std::iter::once((
                    zellij_utils::prompt::CALLER_PANE_TITLE_ARG.to_owned(),
                    "shell".to_owned(),
                )))
                .collect::<BTreeMap<_, _>>(),
            is_private: true,
        }
    }

    fn pipe_effects(effects: &[Effect]) -> Vec<Effect> {
        effects
            .iter()
            .filter(|e| !matches!(e, Effect::SetPopupSize(..) | Effect::SetTimeout(..)))
            .cloned()
            .collect()
    }

    #[test]
    fn a_request_blocks_the_pipe_and_sets_exit_code_one_until_answered() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(message("p", "confirm", &[("message", "Go?")], None), now);
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::BlockPipe("p".to_owned()),
                Effect::SetExitCode("p".to_owned(), 1),
            ]
        );
        app.handle_key(KeyWithModifier::new(BareKey::Enter));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::SetExitCode("p".to_owned(), 0),
                Effect::UnblockPipe("p".to_owned()),
                Effect::CloseSelf,
            ]
        );
        assert!(app.is_finished());
    }

    #[test]
    fn a_notice_releases_the_pipe_with_exit_zero_at_once_and_stays_open() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(
            message("p", "notify", &[("message", "Build finished")], None),
            now,
        );
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::SetExitCode("p".to_owned(), 0),
                Effect::UnblockPipe("p".to_owned()),
            ]
        );
        assert!(!app.is_finished());
        app.handle_key(KeyWithModifier::new(BareKey::Esc));
        assert!(pipe_effects(&app.take_effects()).is_empty());
        assert!(!app.is_finished());
    }

    #[test]
    fn a_notice_closes_itself_when_its_timeout_runs_out() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(
            message(
                "p",
                "notify",
                &[("message", "Build finished"), ("timeout", "5s")],
                None,
            ),
            now,
        );
        app.take_effects();
        app.handle_timer(now + Duration::from_secs(1));
        assert!(pipe_effects(&app.take_effects()).is_empty());
        app.handle_timer(now + Duration::from_secs(6));
        assert_eq!(pipe_effects(&app.take_effects()), vec![Effect::CloseSelf]);
        assert!(app.is_finished());
    }

    #[test]
    fn a_click_on_the_notice_focuses_its_pane_and_closes_it() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(
            message(
                "p",
                "notify",
                &[("message", "Build finished"), ("_caller_pane_id", "terminal_7")],
                None,
            ),
            now,
        );
        app.take_effects();
        let (cols, rows) = app.desired_size().unwrap();
        app.render(rows, cols, now);
        app.handle_mouse(Mouse::LeftClick(1, 2));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![Effect::FocusPane(PaneId::Terminal(7)), Effect::CloseSelf]
        );
        assert!(app.is_finished());
    }

    #[test]
    fn a_click_on_the_close_mark_closes_the_notice_without_focusing() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(
            message(
                "p",
                "notify",
                &[("message", "Build finished"), ("_caller_pane_id", "terminal_7")],
                None,
            ),
            now,
        );
        app.take_effects();
        let (cols, rows) = app.desired_size().unwrap();
        app.render(rows, cols, now);
        app.handle_mouse(Mouse::LeftClick(0, cols - 3));
        assert_eq!(pipe_effects(&app.take_effects()), vec![Effect::CloseSelf]);
    }

    #[test]
    fn a_notice_watches_and_follows_the_names_of_its_pane_and_tab() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(
            message(
                "p",
                "notify",
                &[
                    ("message", "Build finished"),
                    ("_caller_pane_id", "terminal_7"),
                    ("_caller_tab_name", "Tab #1"),
                ],
                None,
            ),
            now,
        );
        assert!(app.take_effects().contains(&Effect::WatchNames));
        let panes = PaneManifest {
            panes: [(
                0,
                vec![PaneInfo {
                    id: 7,
                    title: "vim".to_owned(),
                    ..Default::default()
                }],
            )]
            .into_iter()
            .collect(),
        };
        assert!(app.handle_pane_update(&panes));
        assert!(app.handle_tab_update(&[TabInfo {
            position: 0,
            name: "Editor".to_owned(),
            ..Default::default()
        }]));
        assert!(!app.handle_pane_update(&panes));
        let without_names = {
            let mut app = App::default();
            app.handle_pipe(
                message(
                    "q",
                    "notify",
                    &[
                        ("message", "x"),
                        ("_caller_pane_id", "terminal_7"),
                        ("no_pane_name", "true"),
                        ("no_tab_name", "true"),
                    ],
                    None,
                ),
                now,
            );
            app.take_effects()
        };
        assert!(!without_names.contains(&Effect::WatchNames));
    }

    #[test]
    fn esc_cancels_with_exit_code_one() {
        let mut app = App::default();
        app.handle_pipe(message("p", "toggle", &[], None), Instant::now());
        app.take_effects();
        app.handle_key(KeyWithModifier::new(BareKey::Esc));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::SetExitCode("p".to_owned(), 1),
                Effect::UnblockPipe("p".to_owned()),
                Effect::CloseSelf,
            ]
        );
    }

    #[test]
    fn streamed_choose_lines_pass_through_until_input_ends() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(message("p", "choose", &[], Some("main\n")), now);
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![Effect::SetExitCode("p".to_owned(), 1)]
        );
        app.handle_pipe(message("p", "choose", &[], Some("dev\n")), now);
        assert!(pipe_effects(&app.take_effects()).is_empty());
        app.handle_pipe(message("p", "choose", &[], None), now);
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![Effect::BlockPipe("p".to_owned())]
        );
        app.handle_key(KeyWithModifier::new(BareKey::Down));
        app.handle_key(KeyWithModifier::new(BareKey::Enter));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::Output("p".to_owned(), "dev\n".to_owned()),
                Effect::SetExitCode("p".to_owned(), 0),
                Effect::UnblockPipe("p".to_owned()),
                Effect::CloseSelf,
            ]
        );
    }

    #[test]
    fn a_batch_of_streamed_lines_adds_every_line() {
        let mut app = App::default();
        let now = Instant::now();
        app.handle_pipe(message("p", "choose", &[], Some("a\nb\n")), now);
        app.handle_pipe(message("p", "choose", &[], Some("c\nd\ne\n")), now);
        app.handle_pipe(message("p", "choose", &[], None), now);
        app.take_effects();
        for _ in 0..4 {
            app.handle_key(KeyWithModifier::new(BareKey::Down));
        }
        app.handle_key(KeyWithModifier::new(BareKey::Enter));
        assert!(app
            .take_effects()
            .contains(&Effect::Output("p".to_owned(), "e\n".to_owned())));
    }

    #[test]
    fn timeout_without_a_default_exits_124_and_with_a_default_answers_it() {
        let now = Instant::now();
        let mut app = App::default();
        app.handle_pipe(message("p", "input", &[("timeout", "2s")], None), now);
        app.take_effects();
        assert!(app.handle_timer(now + Duration::from_secs(1)));
        app.handle_timer(now + Duration::from_secs(3));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::SetExitCode("p".to_owned(), 124),
                Effect::UnblockPipe("p".to_owned()),
                Effect::CloseSelf,
            ]
        );
        let mut app = App::default();
        app.handle_pipe(
            message("p", "input", &[("timeout", "2s"), ("default", "x")], None),
            now,
        );
        app.take_effects();
        app.handle_timer(now + Duration::from_secs(3));
        assert_eq!(
            pipe_effects(&app.take_effects()),
            vec![
                Effect::Output("p".to_owned(), "x\n".to_owned()),
                Effect::SetExitCode("p".to_owned(), 0),
                Effect::UnblockPipe("p".to_owned()),
                Effect::CloseSelf,
            ]
        );
    }

    #[test]
    fn a_bad_request_exits_two_with_the_error_on_the_output() {
        let mut app = App::default();
        app.handle_pipe(
            message("p", "number", &[("min", "x")], None),
            Instant::now(),
        );
        let effects = pipe_effects(&app.take_effects());
        assert!(matches!(&effects[1], Effect::Output(_, text) if text.contains("min")));
        assert_eq!(effects[2], Effect::SetExitCode("p".to_owned(), 2));
    }

    #[test]
    fn a_second_request_to_a_busy_prompt_is_refused() {
        let mut app = App::default();
        app.handle_pipe(message("p", "toggle", &[], None), Instant::now());
        app.take_effects();
        app.handle_pipe(message("q", "toggle", &[], None), Instant::now());
        let effects = pipe_effects(&app.take_effects());
        assert_eq!(effects[1], Effect::SetExitCode("q".to_owned(), 2));
        assert_eq!(effects[2], Effect::UnblockPipe("q".to_owned()));
        assert!(!app.is_finished());
    }

    #[test]
    fn json_output_reports_the_answer() {
        let mut app = App::default();
        app.handle_pipe(
            message(
                "p",
                "choose",
                &[("items", r#"["a","b"]"#), ("json", "true")],
                None,
            ),
            Instant::now(),
        );
        app.take_effects();
        app.handle_key(KeyWithModifier::new(BareKey::Enter));
        let effects = pipe_effects(&app.take_effects());
        assert_eq!(
            effects[0],
            Effect::Output(
                "p".to_owned(),
                "{\"result\":\"answered\",\"value\":\"a\"}\n".to_owned()
            )
        );
    }

    #[test]
    fn the_popup_is_sized_to_the_element() {
        let mut app = App::default();
        app.handle_pipe(
            message(
                "p",
                "menu",
                &[("items", r#"["Open","Rename","Delete"]"#)],
                None,
            ),
            Instant::now(),
        );
        let size = app
            .take_effects()
            .into_iter()
            .find_map(|e| match e {
                Effect::SetPopupSize(w, h) => Some((w, h)),
                _ => None,
            })
            .unwrap();
        assert!(size.0 >= MIN_WIDTH + 4);
        assert_eq!(size.1, 3 + 1 + 2);
    }

    #[test]
    fn a_prompt_outside_a_popup_shows_itself() {
        let mut app = App::default();
        let pipe_message = PipeMessage {
            source: PipeSource::Cli("p".to_owned()),
            name: "toggle".to_owned(),
            payload: None,
            args: BTreeMap::new(),
            is_private: true,
        };
        app.handle_pipe(pipe_message, Instant::now());
        assert!(app.take_effects().contains(&Effect::ShowSelf));
    }

    #[test]
    fn only_popups_without_their_own_frame_get_a_frame() {
        let mut app = App::default();
        app.handle_pipe(
            message("p", "choose", &[("items", r#"["a"]"#)], None),
            Instant::now(),
        );
        assert!(app.is_framed());
        let mut app = App::default();
        app.handle_pipe(message("p", "confirm", &[], None), Instant::now());
        assert!(!app.is_framed());
    }
}
