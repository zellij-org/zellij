//! The zellij-tile crate acts as the Rust API for developing plugins for Zellij.
//!
//! To read more about Zellij plugins:
//! [https://zellij.dev/documentation/plugins](https://zellij.dev/documentation/plugins)
//!
//! ### Interesting things in this library:
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
use zellij_utils::data::{ClientId, Event, EventContext, PipeMessage, RenderResponse, Slot, SlotId};

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

#[allow(unused_variables)]
pub trait ZellijSharedPlugin: Default {
    fn load(&mut self, configuration: BTreeMap<String, String>) {}
    fn slot_added(&mut self, slot: Slot) {}
    fn slot_removed(&mut self, slot_id: SlotId) {}
    fn client_connected(&mut self, client_id: ClientId) {}
    fn client_disconnected(&mut self, client_id: ClientId) {}
    fn update(&mut self, event: Event, context: EventContext) -> RenderResponse {
        RenderResponse::Nothing
    }
    fn pipe(&mut self, pipe_message: PipeMessage, context: EventContext) -> RenderResponse {
        RenderResponse::Nothing
    }
    fn render(&mut self, rows: usize, cols: usize, slot_id: SlotId, client_id: ClientId) {}
}

#[doc(hidden)]
pub fn read_shared_context() -> EventContext {
    use prost::Message;
    use zellij_utils::plugin_api::shared_plugin::ProtobufEventContext;
    let protobuf_bytes: Vec<u8> = shim::protobuf_bytes_from_stdin().unwrap_or_default();
    ProtobufEventContext::decode(protobuf_bytes.as_slice())
        .map(EventContext::from)
        .unwrap_or_default()
}

#[doc(hidden)]
pub fn read_shared_slot() -> Slot {
    use prost::Message;
    use zellij_utils::plugin_api::shared_plugin::ProtobufSlot;
    let protobuf_bytes: Vec<u8> = shim::protobuf_bytes_from_stdin().unwrap();
    Slot::from(ProtobufSlot::decode(protobuf_bytes.as_slice()).unwrap())
}

#[doc(hidden)]
pub fn write_render_request(render: RenderResponse) -> i32 {
    use prost::Message;
    use zellij_utils::plugin_api::shared_plugin::ProtobufRenderRequest;
    match render {
        RenderResponse::Nothing => 0,
        RenderResponse::All => 1,
        render => {
            let request: ProtobufRenderRequest = render.into();
            shim::object_to_stdout(&request.encode_to_vec());
            2
        },
    }
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
    "An error occurred in a plugin while receiving an Event from zellij. This means
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

#[macro_export]
macro_rules! register_shared_plugin {
    ($t:ty) => {
        thread_local! {
            static STATE: std::cell::RefCell<$t> = std::cell::RefCell::new(Default::default());
        }

        fn main() {
            std::panic::set_hook(Box::new(|info| {
                $crate::shim::report_panic(info);
            }));
        }

        #[no_mangle]
        pub fn zellij_shared_plugin() {}

        #[no_mangle]
        fn load() {
            STATE.with(|state| {
                use std::collections::BTreeMap;
                use std::convert::TryFrom;
                use $crate::shim::plugin_api::action::ProtobufPluginConfiguration;
                use $crate::shim::prost::Message;
                let protobuf_bytes: Vec<u8> = $crate::shim::protobuf_bytes_from_stdin().unwrap();
                let protobuf_configuration: ProtobufPluginConfiguration =
                    ProtobufPluginConfiguration::decode(protobuf_bytes.as_slice()).unwrap();
                let plugin_configuration: BTreeMap<String, String> =
                    BTreeMap::try_from(&protobuf_configuration).unwrap();
                <$t as $crate::ZellijSharedPlugin>::load(
                    &mut *state.borrow_mut(),
                    plugin_configuration,
                );
            });
        }

        #[no_mangle]
        pub fn shared_update() -> i32 {
            let context = $crate::read_shared_context();
            let event = {
                let protobuf_bytes: Vec<u8> = $crate::shim::protobuf_bytes_from_stdin().unwrap();
                $crate::shim::plugin_api::event::event_from_protobuf_bytes(&protobuf_bytes).unwrap()
            };
            let render = STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::update(&mut *state.borrow_mut(), event, context)
            });
            $crate::write_render_request(render)
        }

        #[no_mangle]
        pub fn shared_pipe() -> i32 {
            use std::convert::TryInto;
            use $crate::shim::plugin_api::pipe_message::ProtobufPipeMessage;
            use $crate::shim::prost::Message;
            let context = $crate::read_shared_context();
            let protobuf_pipe_message: ProtobufPipeMessage = {
                let protobuf_bytes: Vec<u8> = $crate::shim::protobuf_bytes_from_stdin().unwrap();
                ProtobufPipeMessage::decode(protobuf_bytes.as_slice()).unwrap()
            };
            let pipe_message = protobuf_pipe_message.try_into().unwrap();
            let render = STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::pipe(
                    &mut *state.borrow_mut(),
                    pipe_message,
                    context,
                )
            });
            $crate::write_render_request(render)
        }

        #[no_mangle]
        pub fn shared_render(rows: i32, cols: i32, slot_id: i32, client_id: i32) {
            STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::render(
                    &mut *state.borrow_mut(),
                    rows as usize,
                    cols as usize,
                    slot_id as u32,
                    client_id as u32,
                );
            });
        }

        #[no_mangle]
        pub fn slot_added() {
            let slot = $crate::read_shared_slot();
            STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::slot_added(&mut *state.borrow_mut(), slot);
            });
        }

        #[no_mangle]
        pub fn slot_removed(slot_id: i32) {
            STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::slot_removed(
                    &mut *state.borrow_mut(),
                    slot_id as u32,
                );
            });
        }

        #[no_mangle]
        pub fn client_connected(client_id: i32) {
            STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::client_connected(
                    &mut *state.borrow_mut(),
                    client_id as u32,
                );
            });
        }

        #[no_mangle]
        pub fn client_disconnected(client_id: i32) {
            STATE.with(|state| {
                <$t as $crate::ZellijSharedPlugin>::client_disconnected(
                    &mut *state.borrow_mut(),
                    client_id as u32,
                );
            });
        }

        #[no_mangle]
        pub fn plugin_version() {
            println!("{}", $crate::prelude::VERSION);
        }
    };
}
