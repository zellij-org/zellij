pub mod data;
pub mod prelude;
pub mod shim;

use data::*;

#[allow(unused_variables)]
pub trait ZellijTile {
    fn load(&mut self) {}
    fn update(&mut self, event: Event) {}
    fn render(&mut self, rows: usize, cols: usize) {}
    // FIXME: Everything below this line should be purged
    fn handle_tab_rename_keypress(&mut self, key: Key) {}
}

#[macro_export]
macro_rules! register_tile {
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
                state
                    .borrow_mut()
                    .update($crate::shim::deserialize_from_stdin().unwrap());
            });
        }

        #[no_mangle]
        pub fn render(rows: i32, cols: i32) {
            STATE.with(|state| {
                state.borrow_mut().render(rows as usize, cols as usize);
            });
        }

        #[no_mangle]
        pub fn handle_tab_rename_keypress() {
            STATE.with(|state| {
                state
                    .borrow_mut()
                    .handle_tab_rename_keypress($crate::shim::get_key());
            })
        }
    };
}
