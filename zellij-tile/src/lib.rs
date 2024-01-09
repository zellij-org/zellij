//! The zellij-tile crate acts as the Rust API for developing plugins for Zellij.
//!
//! To read more about Zellij plugins:
//! [https://zellij.dev/documentation/plugins](https://zellij.dev/documentation/plugins)
//!
//! ### Interesting things in this libary:
//! - The [`ZellijPlugin`] trait for implementing plugins combined with the
//! [`register_plugin!`](register_plugin) macro to register them.
//! - The list of [commands](shim) representing what a plugin can do.
//! - The list of [`Events`](prelude::Event) a plugin can subscribe to
//! - The [`ZellijWorker`] trait for implementing background workers combined with the
//! [`register_worker!`](register_worker) macro to register them
//!
//! ### Full Example and Development Environment
//! For a working plugin example as well as a development environment, please see:
//! [https://github.com/zellij-org/rust-plugin-example](https://github.com/zellij-org/rust-plugin-example)
//!
pub mod prelude;
pub mod shim;
pub mod ui_components;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use zellij_utils::data::{Event, PipeMessage};

// use zellij_tile::shim::plugin_api::event::ProtobufEvent;

/// This trait should be implemented - once per plugin - on a struct (normally representing the
/// plugin state). This struct should then be registered with the
/// [`register_plugin!`](register_plugin) macro.
#[allow(unused_variables)]
pub trait ZellijPlugin: Default {
    /// Will be called when the plugin is loaded, this is a good place to [`subscribe`](shim::subscribe) to events that are interesting for this plugin.
    fn load(&mut self, configuration: BTreeMap<String, String>) {}
    /// Will be called with an [`Event`](prelude::Event) if the plugin is subscribed to said event.
    /// If the plugin returns `true` from this function, Zellij will know it should be rendered and call its `render` function.
    fn update(&mut self, event: Event) -> bool {
        false
    } // return true if it should render
    /// Will be called when data is being piped to the plugin, a PipeMessage.payload of None signifies the pipe
    /// has ended
    /// If the plugin returns `true` from this function, Zellij will know it should be rendered and call its `render` function.
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        false
    } // return true if it should render
    /// Will be called either after an `update` that requested it, or when the plugin otherwise needs to be re-rendered (eg. on startup, or when the plugin is resized).
    /// The `rows` and `cols` values represent the "content size" of the plugin (this will not include its surrounding frame if the user has pane frames enabled).
    fn render(&mut self, rows: usize, cols: usize) {}
}

/// This trait is used to create workers. Workers can be used by plugins to run longer running
/// background tasks without blocking their own rendering (eg. and showing some sort of loading
/// indication in part of the UI as needed while waiting for the task to complete).
///
/// ## Starting workers on plugin load
/// Implement this trait on a struct (typically representing the worker state) and register it with
/// the [`register_worker!`](register_worker) macro.
///
/// ## Sending messages to workers and back to the plugin
/// Send messages to workers with the [`post_message_to`](shim::post_message_to) method.
/// Send messages from workers back to plugins with the
/// [`post_message_to_plugin`](shim::post_message_to_plugin) method (but be sure the plugin has
/// [`subscribe`](shim::subscribe)d to the [`CustomMessage`](prelude::Event::CustomMessage)) event
/// first!
#[allow(unused_variables)]
pub trait ZellijWorker<'de>: Default + Serialize + Deserialize<'de> {
    /// Triggered whenever the plugin sends the worker a message using the
    /// [`post_message_to`](shim::post_message_to) method.
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

/// Used to register a plugin implementing the [`ZellijPlugin`] trait.
///
/// eg.
/// ```rust
/// use zellij_tile::prelude::*;
///
/// #[derive(Default)]
/// pub struct MyPlugin {}
///
/// impl ZellijPlugin for MyPlugin {
///    // ...
/// }
///
/// register_plugin!(MyPlugin);
/// ```
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
        }

        #[no_mangle]
        fn load() {
            STATE.with(|state| {
                use std::collections::BTreeMap;
                use std::convert::TryFrom;
                use std::convert::TryInto;
                use zellij_tile::shim::plugin_api::action::ProtobufPluginConfiguration;
                use zellij_tile::shim::prost::Message;
                let protobuf_bytes: Vec<u8> = $crate::shim::object_from_stdin().unwrap();
                let protobuf_configuration: ProtobufPluginConfiguration =
                    ProtobufPluginConfiguration::decode(protobuf_bytes.as_slice()).unwrap();
                let plugin_configuration: BTreeMap<String, String> =
                    BTreeMap::try_from(&protobuf_configuration).unwrap();
                state.borrow_mut().load(plugin_configuration);
            });
        }

        #[no_mangle]
        pub fn update() -> bool {
            let err_context = "Failed to deserialize event";
            use std::convert::TryInto;
            use zellij_tile::shim::plugin_api::event::ProtobufEvent;
            use zellij_tile::shim::prost::Message;
            STATE.with(|state| {
                let protobuf_bytes: Vec<u8> = $crate::shim::object_from_stdin().unwrap();
                let protobuf_event: ProtobufEvent =
                    ProtobufEvent::decode(protobuf_bytes.as_slice()).unwrap();
                let event = protobuf_event.try_into().unwrap();
                state.borrow_mut().update(event)
            })
        }

        #[no_mangle]
        pub fn pipe() -> bool {
            let err_context = "Failed to deserialize pipe message";
            use std::convert::TryInto;
            use zellij_tile::shim::plugin_api::pipe_message::ProtobufPipeMessage;
            use zellij_tile::shim::prost::Message;
            STATE.with(|state| {
                let protobuf_bytes: Vec<u8> = $crate::shim::object_from_stdin().unwrap();
                let protobuf_pipe_message: ProtobufPipeMessage =
                    ProtobufPipeMessage::decode(protobuf_bytes.as_slice()).unwrap();
                let pipe_message = protobuf_pipe_message.try_into().unwrap();
                state.borrow_mut().pipe(pipe_message)
            })
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

/// Used to register a plugin worker implementing the [`ZellijWorker`] trait.
///
/// eg.
/// ```rust
/// use zellij_tile::prelude::*;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Default, Serialize, Deserialize)]
/// pub struct FileSearchWorker {}
///
/// impl ZellijWorker<'_> for FileSearchWorker {
///     fn on_message(&mut self, message: String, payload: String) {
///         // ...
///     }
/// }
///
/// register_worker!(
///     FileSearchWorker,
///     file_search_worker, // registers the worker as the namespace "file_search"
///     FILE_SEARCH_WORKER  // expanded to a static variable in which the worker state it held
/// );
/// ```
#[macro_export]
macro_rules! register_worker {
    ($worker:ty, $worker_name:ident, $worker_static_name:ident) => {
        // persist worker state in memory in a static variable
        thread_local! {
            static $worker_static_name: std::cell::RefCell<$worker> = std::cell::RefCell::new(Default::default());
        }
        #[no_mangle]
        pub fn $worker_name() {
            use zellij_tile::shim::plugin_api::message::ProtobufMessage;
            use zellij_tile::shim::prost::Message;
            let worker_display_name = std::stringify!($worker_name);
            let protobuf_bytes: Vec<u8> = $crate::shim::object_from_stdin()
                .unwrap();
            let protobuf_message: ProtobufMessage = ProtobufMessage::decode(protobuf_bytes.as_slice())
                .unwrap();
            let message = protobuf_message.name;
            let payload = protobuf_message.payload;
            $worker_static_name.with(|worker_instance| {
                let mut worker_instance = worker_instance.borrow_mut();
                worker_instance.on_message(message, payload);
            });
         }
    };
}
