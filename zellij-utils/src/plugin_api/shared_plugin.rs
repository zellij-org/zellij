pub use super::generated_api::api::{
    action::{
        NameAndValue as ProtobufNameAndValue, PluginConfiguration as ProtobufPluginConfiguration,
    },
    shared_plugin::{
        EventContext as ProtobufEventContext, RenderKind as ProtobufRenderKind,
        RenderRequest as ProtobufRenderRequest, Slot as ProtobufSlot, SlotKind as ProtobufSlotKind,
    },
};
use crate::data::{EventContext, RenderResponse, Slot, SlotKind};

use std::collections::BTreeMap;
use std::convert::TryFrom;

impl From<EventContext> for ProtobufEventContext {
    fn from(context: EventContext) -> Self {
        ProtobufEventContext {
            slot_id: context.slot_id,
            client_id: context.client_id,
        }
    }
}

impl From<ProtobufEventContext> for EventContext {
    fn from(context: ProtobufEventContext) -> Self {
        EventContext {
            slot_id: context.slot_id,
            client_id: context.client_id,
        }
    }
}

impl From<Slot> for ProtobufSlot {
    fn from(slot: Slot) -> Self {
        let kind = match slot.kind {
            SlotKind::Pane => ProtobufSlotKind::Pane,
            SlotKind::Background => ProtobufSlotKind::Background,
        };
        ProtobufSlot {
            id: slot.id,
            kind: kind as i32,
            configuration: Some(ProtobufPluginConfiguration {
                name_and_value: slot
                    .configuration
                    .into_iter()
                    .map(|(name, value)| ProtobufNameAndValue { name, value })
                    .collect(),
            }),
        }
    }
}

impl From<ProtobufSlot> for Slot {
    fn from(slot: ProtobufSlot) -> Self {
        let kind = match ProtobufSlotKind::try_from(slot.kind) {
            Ok(ProtobufSlotKind::Background) => SlotKind::Background,
            _ => SlotKind::Pane,
        };
        let configuration = slot
            .configuration
            .as_ref()
            .and_then(|c| BTreeMap::try_from(c).ok())
            .unwrap_or_default();
        Slot {
            id: slot.id,
            kind,
            configuration,
        }
    }
}

impl From<RenderResponse> for ProtobufRenderRequest {
    fn from(render: RenderResponse) -> Self {
        match render {
            RenderResponse::Nothing => ProtobufRenderRequest {
                kind: ProtobufRenderKind::Nothing as i32,
                client_id: None,
                slot_ids: vec![],
            },
            RenderResponse::All => ProtobufRenderRequest {
                kind: ProtobufRenderKind::All as i32,
                client_id: None,
                slot_ids: vec![],
            },
            RenderResponse::Client(client_id) => ProtobufRenderRequest {
                kind: ProtobufRenderKind::Client as i32,
                client_id: Some(client_id),
                slot_ids: vec![],
            },
            RenderResponse::Slots(slot_ids) => ProtobufRenderRequest {
                kind: ProtobufRenderKind::Slots as i32,
                client_id: None,
                slot_ids,
            },
        }
    }
}

impl From<ProtobufRenderRequest> for RenderResponse {
    fn from(request: ProtobufRenderRequest) -> Self {
        match ProtobufRenderKind::try_from(request.kind) {
            Ok(ProtobufRenderKind::All) => RenderResponse::All,
            Ok(ProtobufRenderKind::Client) => match request.client_id {
                Some(client_id) => RenderResponse::Client(client_id),
                None => RenderResponse::All,
            },
            Ok(ProtobufRenderKind::Slots) => RenderResponse::Slots(request.slot_ids),
            _ => RenderResponse::Nothing,
        }
    }
}
