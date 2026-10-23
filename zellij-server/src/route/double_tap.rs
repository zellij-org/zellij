use std::time::{Duration, Instant};
use zellij_utils::{
    data::{KeyModifier, KeyWithModifier},
    errors::ErrorContext,
    ipc::ClientToServerMsg,
};

type Input = (ClientToServerMsg, ErrorContext);

#[derive(Default)]
pub(super) struct DoubleTap {
    pending: Option<(Input, Instant)>,
}

pub(super) enum Resolution {
    Wait,
    Dispatch(Input),
    // Resolve the next input only after dispatching the single tap, since its
    // action can change the mode and therefore the next input's binding.
    SingleThenInput(Input, Input),
}

impl DoubleTap {
    pub fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|(_, deadline)| *deadline)
    }

    pub fn flush(&mut self) -> Option<Input> {
        self.pending.take().map(|(input, _)| input)
    }

    pub fn resolve(
        &mut self,
        (mut instruction, context): Input,
        now: Instant,
        delay_for_key: impl FnOnce(&KeyWithModifier) -> Option<Duration>,
    ) -> Resolution {
        let is_input = matches!(
            instruction,
            ClientToServerMsg::Key { .. } | ClientToServerMsg::Action { .. }
        );
        if self
            .pending
            .as_ref()
            .map_or(false, |(_, deadline)| is_input || now >= *deadline)
        {
            let ((previous, previous_context), deadline) = self.pending.take().unwrap();
            match (&previous, &mut instruction) {
                (
                    ClientToServerMsg::Key { key: first, .. },
                    ClientToServerMsg::Key { key: second, .. },
                ) if first == second && now < deadline => {
                    second.key_modifiers.insert(KeyModifier::DoubleTap);
                    Resolution::Dispatch((instruction, context))
                },
                _ => Resolution::SingleThenInput(
                    (previous, previous_context),
                    (instruction, context),
                ),
            }
        } else {
            if let ClientToServerMsg::Key { key, .. } = &instruction {
                if !key.key_modifiers.contains(&KeyModifier::DoubleTap) {
                    if let Some(delay) = delay_for_key(key).filter(|delay| !delay.is_zero()) {
                        if let Some(deadline) = now.checked_add(delay) {
                            self.pending = Some(((instruction, context), deadline));
                            return Resolution::Wait;
                        }
                    }
                }
            }
            Resolution::Dispatch((instruction, context))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::{data::InputMode, input::actions::Action};

    fn key(name: &str) -> Input {
        (
            ClientToServerMsg::Key {
                key: name.parse().unwrap(),
                raw_bytes: vec![7],
                is_kitty_keyboard_protocol: true,
            },
            ErrorContext::default(),
        )
    }

    fn delay(_: &KeyWithModifier) -> Option<Duration> {
        Some(Duration::from_millis(600))
    }

    fn assert_key(input: Input, expected: &str) {
        match input.0 {
            ClientToServerMsg::Key {
                key,
                raw_bytes,
                is_kitty_keyboard_protocol,
            } => {
                assert_eq!(key, expected.parse().unwrap());
                assert_eq!(raw_bytes, vec![7]);
                assert!(is_kitty_keyboard_protocol);
            },
            _ => panic!("expected key"),
        }
    }

    #[test]
    fn matching_second_tap_cancels_single_and_timer() {
        let mut taps = DoubleTap::default();
        let now = Instant::now();
        assert!(matches!(
            taps.resolve(key("Ctrl g"), now, delay),
            Resolution::Wait
        ));
        match taps.resolve(key("Ctrl g"), now + Duration::from_millis(599), delay) {
            Resolution::Dispatch(input) => assert_key(input, "DoubleTap Ctrl g"),
            _ => panic!("expected double tap"),
        }
        assert!(taps.deadline().is_none());
        assert!(taps.flush().is_none());
        assert!(matches!(
            taps.resolve(key("Ctrl g"), now, delay),
            Resolution::Wait
        ));
    }

    #[test]
    fn timeout_releases_original_key_with_its_protocol_and_bytes() {
        let mut taps = DoubleTap::default();
        let now = Instant::now();
        taps.resolve(key("Ctrl g"), now, delay);
        assert_eq!(taps.deadline(), Some(now + Duration::from_millis(600)));
        assert_key(taps.flush().unwrap(), "Ctrl g");
        assert!(taps.deadline().is_none());
    }

    #[test]
    fn different_key_runs_after_single_and_is_reinterpreted() {
        let mut taps = DoubleTap::default();
        let now = Instant::now();
        taps.resolve(key("Ctrl g"), now, delay);
        match taps.resolve(key("x"), now + Duration::from_millis(10), delay) {
            Resolution::SingleThenInput(single, next) => {
                assert_key(single, "Ctrl g");
                // The single tap changed modes; x has no double binding there.
                match taps.resolve(next, now, |_| None) {
                    Resolution::Dispatch(input) => assert_key(input, "x"),
                    _ => panic!("next input must use the new mode"),
                }
            },
            _ => panic!("expected single then next input"),
        }
    }

    #[test]
    fn second_tap_at_or_after_deadline_is_a_new_first_tap() {
        for millis in [600, 601] {
            let mut taps = DoubleTap::default();
            let now = Instant::now();
            taps.resolve(key("Ctrl g"), now, delay);
            match taps.resolve(key("Ctrl g"), now + Duration::from_millis(millis), delay) {
                Resolution::SingleThenInput(single, next) => {
                    assert_key(single, "Ctrl g");
                    assert!(matches!(taps.resolve(next, now, delay), Resolution::Wait));
                },
                _ => panic!("expired tap must run singly"),
            }
        }
    }

    #[test]
    fn unbound_keys_and_zero_delay_are_immediate() {
        let mut taps = DoubleTap::default();
        assert!(matches!(
            taps.resolve(key("x"), Instant::now(), |_| None),
            Resolution::Dispatch(_)
        ));
        assert!(matches!(
            taps.resolve(key("Ctrl g"), Instant::now(), |_| Some(Duration::ZERO)),
            Resolution::Dispatch(_)
        ));
        assert!(taps.deadline().is_none());
    }

    #[test]
    fn protocol_messages_do_not_cancel_but_user_actions_do() {
        let mut taps = DoubleTap::default();
        let now = Instant::now();
        taps.resolve(key("Ctrl g"), now, delay);
        let focus = (
            ClientToServerMsg::HostTerminalFocusChanged { focused: true },
            ErrorContext::default(),
        );
        assert!(matches!(
            taps.resolve(focus, now, delay),
            Resolution::Dispatch(_)
        ));
        assert!(taps.deadline().is_some());
        let action = (
            ClientToServerMsg::Action {
                action: Action::SwitchToMode {
                    input_mode: InputMode::Normal,
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            },
            ErrorContext::default(),
        );
        assert!(matches!(
            taps.resolve(action, now, delay),
            Resolution::SingleThenInput(_, _)
        ));
    }
}
