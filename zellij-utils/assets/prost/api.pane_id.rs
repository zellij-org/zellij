#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneId {
    #[prost(oneof = "pane_id::Id", tags = "1, 2")]
    pub id: ::core::option::Option<pane_id::Id>,
}
/// Nested message and enum types in `PaneId`.
pub mod pane_id {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Id {
        #[prost(uint32, tag = "1")]
        Terminal(u32),
        #[prost(uint32, tag = "2")]
        Plugin(u32),
    }
}
