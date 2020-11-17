mod keys;
mod shim;

pub use keys::*;
pub use shim::*;

pub trait MosaicPlugin {
    fn init(&mut self);
    fn draw(&mut self, rows: usize, cols: usize);
    fn handle_key(&mut self, key: KeyEvent);
}

#[macro_export]
macro_rules! register_plugin {
    ($t:ty) => {
        use mosaic_plugin::*;

        use std::cell::RefCell;
        thread_local! {
            static STATE: RefCell<$t> = RefCell::new(Default::default());
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
                state.borrow_mut().handle_key(get_key());
            });
        }
    };
}
