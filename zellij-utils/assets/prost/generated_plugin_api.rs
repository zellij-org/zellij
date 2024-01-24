pub mod api {
    pub mod action {
        include!("api.action.rs");
    }
    pub mod command {
        include!("api.command.rs");
    }
    pub mod event {
        include!("api.event.rs");
    }
    pub mod file {
        include!("api.file.rs");
    }
    pub mod input_mode {
        include!("api.input_mode.rs");
    }
    pub mod key {
        include!("api.key.rs");
    }
    pub mod message {
        include!("api.message.rs");
    }
    pub mod pipe_message {
        include!("api.pipe_message.rs");
    }
    pub mod plugin_command {
        include!("api.plugin_command.rs");
    }
    pub mod plugin_ids {
        include!("api.plugin_ids.rs");
    }
    pub mod plugin_permission {
        include!("api.plugin_permission.rs");
    }
    pub mod resize {
        include!("api.resize.rs");
    }
    pub mod style {
        include!("api.style.rs");
    }
}
