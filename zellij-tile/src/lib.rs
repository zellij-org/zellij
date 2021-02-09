mod shim;

pub use shim::*;
#[allow(unused_variables)]
pub trait MosaicTile {
    fn init(&mut self) {}
    fn draw(&mut self, rows: usize, cols: usize) {}
    fn handle_key(&mut self, key: Key) {}
    fn handle_global_key(&mut self, key: Key) {}
}

#[macro_export]
macro_rules! register_tile {
    ($t:ty) => {
        thread_local! {
            static STATE: std::cell::RefCell<$t> = std::cell::RefCell::new(Default::default());
        }

        fn main() {
            STATE.with(|state| {
                state.borrow_mut().init();
            });
        }

        #[no_mangle]
        pub fn draw(rows: i32, cols: i32) {
            STATE.with(|state| {
                state.borrow_mut().draw(rows as usize, cols as usize);
            });
        }

        #[no_mangle]
        pub fn handle_key() {
            STATE.with(|state| {
                state.borrow_mut().handle_key($crate::get_key());
            });
        }

        #[no_mangle]
        pub fn handle_global_key() {
            STATE.with(|state| {
                state.borrow_mut().handle_global_key($crate::get_key());
            });
        }
    };
}
