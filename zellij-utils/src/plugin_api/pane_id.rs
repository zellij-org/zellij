pub use super::generated_api::api::pane_id::{
    PaneId as ProtobufPaneId, PaneType as ProtobufPaneType,
};
use crate::data::PaneId;

use std::convert::TryFrom;

impl TryFrom<ProtobufPaneId> for PaneId {
    type Error = &'static str;
    fn try_from(protobuf_pane_id: ProtobufPaneId) -> Result<Self, &'static str> {
        match ProtobufPaneType::from_i32(protobuf_pane_id.pane_type) {
            Some(ProtobufPaneType::Terminal) => Ok(PaneId::Terminal(protobuf_pane_id.id)),
            Some(ProtobufPaneType::Plugin) => Ok(PaneId::Plugin(protobuf_pane_id.id)),
            None => Err("Failed to convert PaneId"),
        }
    }
}

impl TryFrom<PaneId> for ProtobufPaneId {
    type Error = &'static str;
    fn try_from(pane_id: PaneId) -> Result<Self, Self::Error> {
        Ok(match pane_id {
            PaneId::Terminal(id) => ProtobufPaneId {
                pane_type: ProtobufPaneType::Terminal as i32,
                id,
            },
            PaneId::Plugin(id) => ProtobufPaneId {
                pane_type: ProtobufPaneType::Plugin as i32,
                id,
            },
        })
    }
}
