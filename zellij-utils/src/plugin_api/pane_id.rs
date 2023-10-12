use super::generated_api::api::pane_id::pane_id::Id;
pub use super::generated_api::api::pane_id::PaneId as ProtobufPaneId;
use crate::data::PaneId;

use std::convert::TryFrom;

impl TryFrom<ProtobufPaneId> for PaneId {
    type Error = &'static str;
    fn try_from(pane_id: ProtobufPaneId) -> Result<Self, &'static str> {
        match pane_id.id {
            Some(id) => match id {
                Id::Terminal(id) => Ok(PaneId::Terminal(id)),
                Id::Plugin(id) => Ok(PaneId::Plugin(id)),
            },
            None => Err("No id in PaneId"),
        }
    }
}

impl TryFrom<PaneId> for ProtobufPaneId {
    type Error = &'static str;
    fn try_from(pane_id: PaneId) -> Result<Self, &'static str> {
        match pane_id {
            PaneId::Plugin(id) => {
                let pane_id = ProtobufPaneId {
                    id: Some(Id::Plugin(id)),
                };
                Ok(pane_id)
            },
            PaneId::Terminal(id) => {
                let pane_id = ProtobufPaneId {
                    id: Some(Id::Terminal(id)),
                };
                Ok(pane_id)
            },
        }
    }
}
