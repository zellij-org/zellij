pub mod grid_state;
pub mod interceptor;
pub mod parser;
pub mod placeholders;
pub mod replies;
pub mod store;
pub use grid_state::*;
pub use interceptor::*;
pub use parser::*;
pub use placeholders::*;
pub use replies::*;
pub use store::*;

/// The kitty graphics protocol's sentinel for "no placement id was given" -
/// both on the wire (an absent `p=` key) and in the underline-color channel
/// placeholder cells carry it in (an unset/default underline color decodes
/// to this).
pub const NO_PLACEMENT_ID: u32 = 0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KittyHostSupport {
    Supported,
    Unsupported,
    ProtocolDisabled,
}

impl KittyHostSupport {
    pub fn from_host_capability(supported: bool) -> Self {
        if supported {
            KittyHostSupport::Supported
        } else {
            KittyHostSupport::Unsupported
        }
    }
    pub fn protocol_is_enabled(&self) -> bool {
        *self != KittyHostSupport::ProtocolDisabled
    }
    pub fn host_supports_graphics(&self) -> bool {
        *self == KittyHostSupport::Supported
    }
}
