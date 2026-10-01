mod app;
mod form;
mod outcome;
mod request;
mod store;
mod ui;

use app::{App, Effect};
use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::*;

#[derive(Default)]
struct Prompt {
    app: App,
}

register_plugin!(Prompt);

#[cfg(target_family = "wasm")]
fn apply(effects: Vec<Effect>) {
    for effect in effects {
        match effect {
            Effect::BlockPipe(pipe_id) => block_cli_pipe_input(&pipe_id),
            Effect::SetExitCode(pipe_id, exit_code) => set_cli_pipe_exit_code(&pipe_id, exit_code),
            Effect::Output(pipe_id, output) => cli_pipe_output(&pipe_id, &output),
            Effect::UnblockPipe(pipe_id) => unblock_cli_pipe_input(&pipe_id),
            Effect::SetPopupSize(width, height) => set_popup_size(width, height),
            Effect::SetTimeout(seconds) => set_timeout(seconds),
            Effect::ShowSelf => show_self(true),
            Effect::CloseSelf => close_self(),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn apply(_effects: Vec<Effect>) {}

impl Prompt {
    fn flush(&mut self) {
        apply(self.app.take_effects());
    }
}

impl ZellijPlugin for Prompt {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[EventType::Key, EventType::Mouse, EventType::Timer]);
    }
    fn update(&mut self, event: Event) -> bool {
        let should_render = match event {
            Event::Key(key) => self.app.handle_key(key),
            Event::Mouse(mouse) => self.app.handle_mouse(mouse),
            Event::Timer(_) => self.app.handle_timer(Instant::now()),
            _ => false,
        };
        self.flush();
        should_render
    }
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        let should_render = self.app.handle_pipe(pipe_message, Instant::now());
        self.flush();
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.app.render(rows, cols, Instant::now());
    }
}
