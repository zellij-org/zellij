use super::generated_api::api::{
    action::{action::OptionalPayload, Action as ProtobufAction, ActionName},
    event::{event::Payload, Event as ProtobufEvent, MouseEventName},
    input_mode::InputMode,
    pipe_message::{PipeMessage as ProtobufPipeMessage, PipeSource},
};

pub const CONTRACT_MARKER_EXPORT: &str = "zellij_plugin_contract";

const LAST_ACTION_NAME_OF_0_45: ActionName = ActionName::CopyLastCommandOutput;
const ACTION_NAMES_0_45_CANNOT_DECODE: [ActionName; 6] = [
    ActionName::CliPipe,
    ActionName::SwitchSession,
    ActionName::SetPaneBorderless,
    ActionName::GoToTabById,
    ActionName::CloseTabById,
    ActionName::RenameTabById,
];
const LAST_INPUT_MODE_OF_0_45: InputMode = InputMode::Tmux;
const LAST_MOUSE_EVENT_NAME_OF_0_45: MouseEventName = MouseEventName::MouseScrollRight;
const LAST_PIPE_SOURCE_OF_0_45: PipeSource = PipeSource::Keybind;

fn input_mode_known_to_0_45(input_mode: i32) -> bool {
    (0..=LAST_INPUT_MODE_OF_0_45 as i32).contains(&input_mode)
}

fn input_mode_for_0_45(input_mode: i32) -> i32 {
    if input_mode == InputMode::RenameSession as i32 {
        InputMode::RenameTab as i32
    } else {
        input_mode
    }
}

pub fn action_decodable_by_0_45(action: &ProtobufAction) -> bool {
    let known_name = (0..=LAST_ACTION_NAME_OF_0_45 as i32).contains(&action.name)
        && !ACTION_NAMES_0_45_CANNOT_DECODE
            .iter()
            .any(|name| *name as i32 == action.name);
    if !known_name {
        return false;
    }
    match &action.optional_payload {
        Some(OptionalPayload::SwitchToModePayload(payload))
        | Some(OptionalPayload::SwitchModeForAllClientsPayload(payload)) => {
            input_mode_known_to_0_45(payload.input_mode)
        },
        _ => true,
    }
}

pub fn event_for_0_45_plugin(mut event: ProtobufEvent) -> Option<ProtobufEvent> {
    match event.payload.as_mut() {
        Some(Payload::ModeUpdatePayload(payload)) => {
            payload.current_mode = input_mode_for_0_45(payload.current_mode);
            Some(event)
        },
        Some(Payload::MouseEventPayload(payload))
            if payload.mouse_event_name > LAST_MOUSE_EVENT_NAME_OF_0_45 as i32 =>
        {
            None
        },
        Some(Payload::UserActionPayload(payload)) => match &payload.action {
            Some(action) if action_decodable_by_0_45(action) => Some(event),
            _ => None,
        },
        _ => Some(event),
    }
}

pub fn pipe_message_decodable_by_0_45(message: &ProtobufPipeMessage) -> bool {
    (0..=LAST_PIPE_SOURCE_OF_0_45 as i32).contains(&message.source)
}

#[cfg(test)]
mod tests {
    use super::super::generated_api::api::{
        action::SwitchToModePayload,
        event::{EventType, ModeUpdatePayload, MouseEventPayload, UserActionPayload},
    };
    use super::*;
    use crate::data::{Event, InputMode as DataInputMode, ModeInfo, Mouse};
    use crate::input::actions::Action;
    use prost::Message;

    fn protobuf_action(action: Action) -> ProtobufAction {
        action.try_into().expect("the action encodes")
    }

    fn user_action_event(action: Action) -> ProtobufEvent {
        ProtobufEvent::try_from(Event::UserAction(action, 1, None, None))
            .expect("the event encodes")
    }

    fn reaches_a_0_45_plugin_intact(event: ProtobufEvent) -> Option<ProtobufEvent> {
        let adapted = event_for_0_45_plugin(event)?;
        let bytes = adapted.encode_to_vec();
        Some(ProtobufEvent::decode(bytes.as_slice()).expect("the bytes decode"))
    }

    #[test]
    fn every_action_the_0_45_decoder_knew_still_passes_and_newer_ones_do_not() {
        for name in 0..=LAST_ACTION_NAME_OF_0_45 as i32 {
            let action = ProtobufAction {
                name,
                optional_payload: None,
            };
            let known = !ACTION_NAMES_0_45_CANNOT_DECODE
                .iter()
                .any(|unknown| *unknown as i32 == name);
            assert_eq!(action_decodable_by_0_45(&action), known, "action {}", name);
        }
        let newer = ProtobufAction {
            name: LAST_ACTION_NAME_OF_0_45 as i32 + 1,
            optional_payload: None,
        };
        assert!(!action_decodable_by_0_45(&newer));
    }

    #[test]
    fn a_user_action_a_0_45_plugin_cannot_decode_is_not_delivered_to_it() {
        assert!(reaches_a_0_45_plugin_intact(user_action_event(Action::Quit)).is_some());
        assert!(event_for_0_45_plugin(user_action_event(Action::CloseTabById { id: 1 })).is_none());
        assert!(event_for_0_45_plugin(user_action_event(Action::DismissInfoPopups)).is_none());
        assert!(
            event_for_0_45_plugin(user_action_event(Action::SwitchToMode {
                input_mode: DataInputMode::RenameSession
            }))
            .is_none()
        );
        assert!(
            event_for_0_45_plugin(user_action_event(Action::SwitchToMode {
                input_mode: DataInputMode::Locked
            }))
            .is_some()
        );
    }

    #[test]
    fn a_rename_session_mode_update_reaches_a_0_45_plugin_as_rename_tab() {
        let mode_info = ModeInfo {
            mode: DataInputMode::RenameSession,
            ..Default::default()
        };
        let event =
            ProtobufEvent::try_from(Event::ModeUpdate(mode_info)).expect("the event encodes");
        let adapted =
            reaches_a_0_45_plugin_intact(event).expect("mode updates are always delivered");
        match adapted.payload {
            Some(Payload::ModeUpdatePayload(ModeUpdatePayload { current_mode, .. })) => {
                assert_eq!(current_mode, InputMode::RenameTab as i32);
                assert!(input_mode_known_to_0_45(current_mode));
            },
            other => panic!("unexpected payload {:?}", other),
        }
    }

    #[test]
    fn other_mode_updates_reach_a_0_45_plugin_unchanged() {
        let mode_info = ModeInfo {
            mode: DataInputMode::Locked,
            ..Default::default()
        };
        let event =
            ProtobufEvent::try_from(Event::ModeUpdate(mode_info)).expect("the event encodes");
        assert_eq!(event_for_0_45_plugin(event.clone()), Some(event));
    }

    #[test]
    fn a_double_click_is_not_delivered_to_a_0_45_plugin_but_clicks_are() {
        let double_click =
            ProtobufEvent::try_from(Event::Mouse(Mouse::DoubleClick(1, 2))).expect("encodes");
        assert!(event_for_0_45_plugin(double_click).is_none());
        let click = ProtobufEvent::try_from(Event::Mouse(Mouse::LeftClick(1, 2))).expect("encodes");
        let scroll = ProtobufEvent::try_from(Event::Mouse(Mouse::ScrollRight(3))).expect("encodes");
        assert!(event_for_0_45_plugin(click).is_some());
        assert!(event_for_0_45_plugin(scroll).is_some());
        let unknown_future_mouse_event = ProtobufEvent {
            name: EventType::Mouse as i32,
            payload: Some(Payload::MouseEventPayload(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseDoubleClick as i32 + 1,
                ..Default::default()
            })),
        };
        assert!(event_for_0_45_plugin(unknown_future_mouse_event).is_none());
    }

    #[test]
    fn a_user_action_without_an_action_is_not_delivered_to_a_0_45_plugin() {
        let event = ProtobufEvent {
            name: EventType::UserAction as i32,
            payload: Some(Payload::UserActionPayload(UserActionPayload {
                action: None,
                client_id: 1,
                terminal_id: None,
                cli_client_id: None,
            })),
        };
        assert!(event_for_0_45_plugin(event).is_none());
    }

    #[test]
    fn switch_mode_payloads_are_checked_for_modes_the_0_45_decoder_knew() {
        let action = ProtobufAction {
            name: ActionName::SwitchModeForAllClients as i32,
            optional_payload: Some(OptionalPayload::SwitchModeForAllClientsPayload(
                SwitchToModePayload {
                    input_mode: InputMode::RenameSession as i32,
                },
            )),
        };
        assert!(!action_decodable_by_0_45(&action));
        assert!(action_decodable_by_0_45(&protobuf_action(
            Action::SwitchModeForAllClients {
                input_mode: DataInputMode::Tmux
            }
        )));
    }

    #[test]
    fn prompt_requests_are_not_piped_to_a_0_45_plugin() {
        let pipe = |source: PipeSource| ProtobufPipeMessage {
            source: source as i32,
            ..Default::default()
        };
        assert!(pipe_message_decodable_by_0_45(&pipe(PipeSource::Cli)));
        assert!(pipe_message_decodable_by_0_45(&pipe(PipeSource::Plugin)));
        assert!(pipe_message_decodable_by_0_45(&pipe(PipeSource::Keybind)));
        assert!(!pipe_message_decodable_by_0_45(&pipe(
            PipeSource::PromptRequest
        )));
    }
}
