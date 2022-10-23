pub mod prelude;
pub mod shim;

use zellij_utils::data::Event;

#[allow(unused_variables)]
pub trait ZellijPlugin {
    fn load(&mut self) {}
    fn update(&mut self, event: Event) {}
    fn render(&mut self, rows: usize, cols: usize) {}
}

pub const PLUGIN_MISMATCH: &str =
"An error occured in a plugin while receiving an Event from zellij. This means
that your plugins aren't compatible with your zellij version.

The most likely explanation for this is that you're running either a
self-compiled zellij or plugin version. Please make sure that, while developing,
you also rebuild the plugins in order to pick up changes to the plugin code.

Please refer to the documentation for further information:
    https://github.com/zellij-org/zellij/blob/main/CONTRIBUTING.md#building
";

#[macro_export]
macro_rules! register_plugin {
    ($t:ty) => {
        thread_local! {
            static STATE: std::cell::RefCell<$t> = std::cell::RefCell::new(Default::default());
        }

        fn main() {
            // Register custom panic handler
            std::panic::set_hook(Box::new(|info| {
                report_panic(info);
            }));

            STATE.with(|state| {
                state.borrow_mut().load();
            });
        }

        #[no_mangle]
        pub fn update() {
            let object = $crate::shim::object_from_stdin()
                .context($crate::PLUGIN_MISMATCH)
                .to_stdout()
                .unwrap();

            STATE.with(|state| {
                state
                    .borrow_mut()
                    .update(object);
            });
        }

        #[no_mangle]
        pub fn render(rows: i32, cols: i32) {
            STATE.with(|state| {
                state.borrow_mut().render(rows as usize, cols as usize);
            });
        }

        #[no_mangle]
        pub fn plugin_version() {
            println!("{}", $crate::prelude::VERSION);
        }

    };
}

