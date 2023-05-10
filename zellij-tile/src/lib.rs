pub mod prelude;
pub mod shim;

use zellij_utils::data::Event;
use serde::{Serialize, Deserialize};

#[allow(unused_variables)]
pub trait ZellijPlugin {
    fn load(&mut self) {}
    fn update(&mut self, event: Event) -> bool {
        false
    } // return true if it should render
    fn render(&mut self, rows: usize, cols: usize) {}
}

#[allow(unused_variables)]
// TODO: can we get rid of the lifetime? maybe with generics?
pub trait ZellijWorker<'de>: Default + Serialize + Deserialize<'de> {
    fn on_message(&mut self, message: String, payload: String) {}
}

pub const PLUGIN_MISMATCH: &str =
    "An error occured in a plugin while receiving an Event from zellij. This means
that the plugins aren't compatible with the current zellij version.

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
        pub fn update() -> bool {
            let object = $crate::shim::object_from_stdin()
                .context($crate::PLUGIN_MISMATCH)
                .to_stdout()
                .unwrap();

            STATE.with(|state| state.borrow_mut().update(object))
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

#[macro_export]
macro_rules! register_worker {
    ($worker:ty, $worker_name:ident) => {
        #[no_mangle]
        pub fn $worker_name() {
            let worker_display_name = std::stringify!($worker_name);

            // read message from STDIN
            let (message, payload): (String, String) = $crate::shim::object_from_stdin().unwrap_or_else(|e| {
                eprintln!("Failed to deserialize message to worker \"{}\": {:?}", worker_display_name, e);
                Default::default()
            });

            // read previous worker state from HD if it exists
            let mut worker_instance = match std::fs::read(&format!("/data/{}", worker_display_name))
                .map_err(|e| format!("Failed to read file: {:?}", e))
                .and_then(|s| {
                serde_json::from_str::<$worker>(&String::from_utf8_lossy(&s)).map_err(|e| format!("Failed to deserialize: {:?}", e))
            }) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to read existing state ({:?}), creating new state for worker", e);
                    <$worker>::default()
                }
            };

            // invoke worker
            worker_instance.on_message(message, payload);

            // persist worker state to HD for next run
            match serde_json::to_string(&worker_instance)
                .map_err(|e| format!("Failed to serialize worker state"))
                .and_then(|serialized_state| std::fs::write(&format!("/data/{}", worker_display_name), serialized_state.as_bytes())
                .map_err(|e| format!("Failed to persist state to HD"))) {
                    Ok(()) => {},
                    Err(e) => eprintln!("Failed to serialize and persist worker state to hd: {:?}", e),
            }
        }
    }
}
