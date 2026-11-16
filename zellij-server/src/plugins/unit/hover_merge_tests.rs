use super::{is_hover, subscription_event_type, HoverGenerations};
use std::collections::BTreeSet;
use zellij_utils::data::{BareKey, Event, EventType, KeyModifier, KeyWithModifier, Mouse};

fn shift() -> BTreeSet<KeyModifier> {
    let mut modifiers = BTreeSet::new();
    modifiers.insert(KeyModifier::Shift);
    modifiers
}

#[test]
fn a_newer_hover_supersedes_an_older_one_for_the_same_plugin_and_client() {
    let mut generations = HoverGenerations::default();
    let first = generations.queue(1, 1);
    assert!(!generations.is_superseded(1, 1, first));
    let second = generations.queue(1, 1);
    assert!(generations.is_superseded(1, 1, first));
    assert!(!generations.is_superseded(1, 1, second));
}

#[test]
fn hovers_of_other_plugins_or_clients_do_not_supersede_each_other() {
    let mut generations = HoverGenerations::default();
    let plugin_one = generations.queue(1, 1);
    let plugin_two = generations.queue(2, 1);
    let other_client = generations.queue(1, 2);
    generations.queue(2, 1);
    generations.queue(1, 2);
    assert!(!generations.is_superseded(1, 1, plugin_one));
    assert!(generations.is_superseded(2, 1, plugin_two));
    assert!(generations.is_superseded(1, 2, other_client));
}

#[test]
fn a_generation_never_queued_is_not_superseded() {
    let generations = HoverGenerations::default();
    assert!(!generations.is_superseded(7, 3, 1));
}

#[test]
fn only_hover_events_are_merged() {
    assert!(is_hover(&Event::Mouse(Mouse::Hover(1, 2))));
    assert!(is_hover(&Event::MouseWithModifiers(
        Mouse::Hover(1, 2),
        shift()
    )));
    assert!(!is_hover(&Event::Mouse(Mouse::LeftClick(1, 2))));
    assert!(!is_hover(&Event::Mouse(Mouse::Release(1, 2))));
    assert!(!is_hover(&Event::Mouse(Mouse::ScrollDown(1))));
    assert!(!is_hover(&Event::MouseWithModifiers(
        Mouse::LeftClick(1, 2),
        shift()
    )));
    assert!(!is_hover(&Event::Key(KeyWithModifier::new(BareKey::Enter))));
    assert!(!is_hover(&Event::Timer(1.0)));
}

#[test]
fn a_mouse_event_with_modifiers_matches_a_mouse_subscription() {
    assert_eq!(
        subscription_event_type(&Event::MouseWithModifiers(Mouse::LeftClick(1, 2), shift()))
            .unwrap(),
        EventType::Mouse
    );
    assert_eq!(
        subscription_event_type(&Event::Mouse(Mouse::LeftClick(1, 2))).unwrap(),
        EventType::Mouse
    );
    assert_eq!(
        subscription_event_type(&Event::Key(KeyWithModifier::new(BareKey::Enter))).unwrap(),
        EventType::Key
    );
    assert_eq!(
        subscription_event_type(&Event::Timer(1.0)).unwrap(),
        EventType::Timer
    );
}
