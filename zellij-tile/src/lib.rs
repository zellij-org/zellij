pub mod data;
pub mod prelude;
pub mod shim;

use data::*;
use serde::de::DeserializeOwned;

#[allow(unused_variables)]
pub trait ZellijPlugin {
    type Options: Default + DeserializeOwned;

    fn load(&mut self, _: Self::Options) {}
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
            let options = $crate::shim::object_from_stdin().map_err(|err| {
                eprintln!(
                    "Your plugins configuration had a derialization error: {:?}.",
                    err
                );
                err
            });

            STATE.with(|state| {
                state.borrow_mut().load(options.unwrap_or_default());
            });
        }

        #[no_mangle]
        pub fn update() {
            STATE.with(|state| {
                state
                    .borrow_mut()
                    .update($crate::shim::object_from_stdin().unwrap());
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
