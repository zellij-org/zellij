pub mod data;
pub mod prelude;
pub mod shim;

use data::*;

#[allow(unused_variables)]
pub trait ZellijPlugin {
    fn load(&mut self) {}
    fn update(&mut self, event: Event) {}
    fn render(&mut self, rows: usize, cols: usize) {}
}

#[macro_export]
macro_rules! register_plugin {
    ($t:ty) => {
        thread_local! {
            static STATE: std::cell::RefCell<$t> = std::cell::RefCell::new(Default::default());
        }

        fn main() {
            STATE.with(|state| {
                state.borrow_mut().load();
            });
        }

        #[no_mangle]
        pub fn update() {
            STATE.with(|state| {
                state.borrow_mut().update($crate::shim::object_from_stdin());
            });
        }

        #[no_mangle]
        pub fn render(rows: i32, cols: i32) {
            STATE.with(|state| {
                state.borrow_mut().render(rows as usize, cols as usize);
            });
        }
    };
}

#[macro_export]
macro_rules! rgb {
    ($a:expr) => {
        ansi_term::Color::RGB($a.0, $a.1, $a.2)
    };
}

#[macro_export]
macro_rules! style {
    ($a:expr, $b:expr) => {
        ansi_term::Style::new()
            .fg(ansi_term::Color::RGB($a.0, $a.1, $a.2))
            .on(ansi_term::Color::RGB($b.0, $b.1, $b.2))
    };
}
