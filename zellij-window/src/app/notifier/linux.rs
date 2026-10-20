use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;

use super::{escape_markup, Events, Sink, SinkEvent};
use crate::app::engine::{Content, NotifyId};
use crate::dbus::{match_rule, Bus, Kind, Message, Value};

pub const SERVICE: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);
const REASON_DISMISSED: u32 = 2;

#[derive(Default)]
struct Ids {
    by_id: HashMap<NotifyId, u32>,
    by_number: HashMap<u32, NotifyId>,
    tokens: HashMap<u32, String>,
}

impl Ids {
    fn remember(&mut self, id: &NotifyId, number: u32) {
        if let Some(previous) = self.by_id.insert(id.clone(), number) {
            if previous != number {
                self.by_number.remove(&previous);
            }
        }
        self.by_number.insert(number, id.clone());
    }

    fn forget_number(&mut self, number: u32) -> Option<NotifyId> {
        let id = self.by_number.remove(&number)?;
        if self.by_id.get(&id) == Some(&number) {
            self.by_id.remove(&id);
        }
        self.tokens.remove(&number);
        Some(id)
    }
}

pub struct Freedesktop {
    bus: Bus,
    ids: Arc<Mutex<Ids>>,
    markup: bool,
}

fn on_signal(message: &Message, ids: &Mutex<Ids>, events: &Events) {
    if message.kind != Kind::Signal || message.interface.as_deref() != Some(SERVICE) {
        return;
    }
    let number = message.body.first().and_then(Value::as_u32);
    let Some(number) = number else {
        return;
    };
    let Ok(mut ids) = ids.lock() else {
        return;
    };
    match message.member.as_deref() {
        Some("ActivationToken") => {
            if let Some(token) = message.body.get(1).and_then(Value::as_str) {
                ids.tokens.insert(number, token.to_owned());
            }
        },
        Some("ActionInvoked") => {
            let action = message.body.get(1).and_then(Value::as_str).unwrap_or("");
            if action != "default" {
                return;
            }
            if let Some(id) = ids.by_number.get(&number).cloned() {
                let token = ids.tokens.remove(&number);
                drop(ids);
                events(SinkEvent::Clicked { id, token });
            }
        },
        Some("NotificationClosed") => {
            let reason = message.body.get(1).and_then(Value::as_u32).unwrap_or(0);
            if let Some(id) = ids.forget_number(number) {
                drop(ids);
                if reason == REASON_DISMISSED {
                    events(SinkEvent::Dismissed(id));
                }
            }
        },
        _ => {},
    }
}

impl Freedesktop {
    pub fn open(events: Events) -> Result<Freedesktop> {
        Freedesktop::with(events, Bus::session)
    }

    #[cfg(test)]
    fn over(stream: std::os::unix::net::UnixStream, events: Events) -> Result<Freedesktop> {
        Freedesktop::with(events, move |make| Bus::over(stream, make))
    }

    fn with(
        events: Events,
        connect: impl FnOnce(Box<dyn FnOnce(Bus) -> crate::dbus::Handler>) -> Result<Bus>,
    ) -> Result<Freedesktop> {
        let ids: Arc<Mutex<Ids>> = Arc::new(Mutex::new(Ids::default()));
        let handler_ids = ids.clone();
        let bus = connect(Box::new(move |_bus| {
            Box::new(move |message: Message| on_signal(&message, &handler_ids, &events))
        }))?;
        bus.add_match(&match_rule(SERVICE, None))?;
        let capabilities = bus
            .call(
                Message::call(SERVICE, PATH, SERVICE, "GetCapabilities", vec![]),
                CALL_TIMEOUT,
            )
            .map(|reply| {
                reply
                    .body
                    .first()
                    .map(|list| {
                        list.items()
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default()
            })?;
        Ok(Freedesktop {
            bus,
            ids,
            markup: capabilities.iter().any(|c| c == "body-markup"),
        })
    }

    fn body(&self, text: &str) -> String {
        if self.markup {
            escape_markup(text)
        } else {
            text.to_owned()
        }
    }

    fn notify(&self, replaces: u32, title: &str, body: &str, clickable: bool) -> Option<u32> {
        let actions: Vec<&str> = if clickable {
            vec!["default", "Open"]
        } else {
            vec![]
        };
        let call = notify_call(replaces, title, &self.body(body), &actions);
        match self.bus.call(call, CALL_TIMEOUT) {
            Ok(reply) => reply.body.first().and_then(Value::as_u32),
            Err(e) => {
                report!("a desktop notification could not be shown: {:#}", e);
                None
            },
        }
    }
}

pub fn notify_call(replaces: u32, title: &str, body: &str, actions: &[&str]) -> Message {
    Message::call(
        SERVICE,
        PATH,
        SERVICE,
        "Notify",
        vec![
            Value::str("Zellij"),
            Value::U32(replaces),
            Value::str(crate::window::application_id()),
            Value::str(title),
            Value::str(body),
            Value::strings(actions.iter().copied()),
            Value::dict(vec![
                ("desktop-entry", Value::str(crate::window::application_id())),
                ("urgency", Value::Byte(1)),
            ]),
            Value::I32(-1),
        ],
    )
}

impl Sink for Freedesktop {
    fn show(&mut self, id: &NotifyId, content: &Content) {
        let replaces = self
            .ids
            .lock()
            .ok()
            .and_then(|ids| ids.by_id.get(id).copied())
            .unwrap_or(0);
        if let Some(number) = self.notify(replaces, &content.title, &content.body, true) {
            if let Ok(mut ids) = self.ids.lock() {
                ids.remember(id, number);
            }
        }
    }

    fn withdraw(&mut self, id: &NotifyId) {
        let number = self.ids.lock().ok().and_then(|mut ids| {
            let number = ids.by_id.get(id).copied()?;
            ids.forget_number(number);
            Some(number)
        });
        let Some(number) = number else {
            return;
        };
        let call = Message::call(
            SERVICE,
            PATH,
            SERVICE,
            "CloseNotification",
            vec![Value::U32(number)],
        );
        if let Err(e) = self.bus.call(call, CALL_TIMEOUT) {
            report!("a desktop notification could not be withdrawn: {:#}", e);
        }
    }

    fn simple(&mut self, title: &str, body: &str) {
        self.notify(0, title, body, false);
    }
}

impl Drop for Freedesktop {
    fn drop(&mut self) {
        self.bus.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::engine::ItemKey;
    use zellij_utils::data::PaneId;

    fn id() -> NotifyId {
        NotifyId::Item(ItemKey {
            session: "b".to_owned(),
            pane: PaneId::Terminal(1),
        })
    }

    fn signal(member: &str, body: Vec<Value>) -> Message {
        Message::signal(PATH, SERVICE, member, body)
    }

    fn recorded() -> (Events, Arc<Mutex<Vec<SinkEvent>>>) {
        let seen: Arc<Mutex<Vec<SinkEvent>>> = Arc::default();
        let into = seen.clone();
        (
            Arc::new(move |event| into.lock().unwrap().push(event)),
            seen,
        )
    }

    #[test]
    fn a_click_on_the_default_action_reports_the_item_with_its_activation_token() {
        let ids = Mutex::new(Ids::default());
        ids.lock().unwrap().remember(&id(), 7);
        let (events, seen) = recorded();
        on_signal(
            &signal("ActivationToken", vec![Value::U32(7), Value::str("tok")]),
            &ids,
            &events,
        );
        on_signal(
            &signal("ActionInvoked", vec![Value::U32(7), Value::str("default")]),
            &ids,
            &events,
        );
        on_signal(
            &signal("ActionInvoked", vec![Value::U32(8), Value::str("default")]),
            &ids,
            &events,
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![SinkEvent::Clicked {
                id: id(),
                token: Some("tok".to_owned())
            }]
        );
    }

    #[test]
    fn closing_forgets_the_number_and_only_a_user_dismissal_is_reported() {
        let ids = Mutex::new(Ids::default());
        ids.lock().unwrap().remember(&id(), 7);
        let (events, seen) = recorded();
        on_signal(
            &signal("NotificationClosed", vec![Value::U32(7), Value::U32(1)]),
            &ids,
            &events,
        );
        assert!(seen.lock().unwrap().is_empty());
        assert!(ids.lock().unwrap().by_id.is_empty());
        ids.lock().unwrap().remember(&id(), 9);
        on_signal(
            &signal("NotificationClosed", vec![Value::U32(9), Value::U32(2)]),
            &ids,
            &events,
        );
        assert_eq!(*seen.lock().unwrap(), vec![SinkEvent::Dismissed(id())]);
    }

    #[test]
    fn a_replaced_notification_keeps_one_number_per_item() {
        let mut ids = Ids::default();
        ids.remember(&id(), 7);
        ids.remember(&id(), 7);
        assert_eq!(ids.by_number.len(), 1);
        ids.remember(&id(), 8);
        assert_eq!(ids.by_number.len(), 1);
        assert_eq!(ids.by_id.get(&id()), Some(&8));
    }

    #[test]
    fn a_notification_server_on_the_bus_sees_show_replace_withdraw_and_reports_a_click() {
        let (ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        let server = crate::dbus::fake::FakeBus::serve(theirs, |call, bus| {
            if call.is(SERVICE, "GetCapabilities") {
                return Some(vec![Value::strings(["body", "body-markup", "actions"])]);
            }
            if call.is(SERVICE, "Notify") {
                let replaces = call.body[1].as_u32().unwrap();
                let number = if replaces == 0 { 41 } else { replaces };
                if replaces != 0 {
                    bus.signal(Message::signal(
                        PATH,
                        SERVICE,
                        "ActivationToken",
                        vec![Value::U32(number), Value::str("token-1")],
                    ));
                    bus.signal(Message::signal(
                        PATH,
                        SERVICE,
                        "ActionInvoked",
                        vec![Value::U32(number), Value::str("default")],
                    ));
                }
                return Some(vec![Value::U32(number)]);
            }
            if call.is(SERVICE, "CloseNotification") {
                return Some(vec![]);
            }
            None
        });
        let (events, seen) = recorded();
        let mut sink = Freedesktop::over(ours, events).unwrap();
        sink.show(
            &id(),
            &Content {
                title: "b pane1".to_owned(),
                body: "<b>x</b>".to_owned(),
            },
        );
        sink.show(
            &id(),
            &Content {
                title: "b pane1".to_owned(),
                body: "y".to_owned(),
            },
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while seen.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        sink.withdraw(&id());
        let calls = server.calls();
        let notifies: Vec<&Message> = calls.iter().filter(|c| c.is(SERVICE, "Notify")).collect();
        assert_eq!(notifies.len(), 2);
        assert_eq!(notifies[0].body[1], Value::U32(0));
        assert_eq!(notifies[0].body[4], Value::str("&lt;b&gt;x&lt;/b&gt;"));
        assert_eq!(
            notifies[1].body[1],
            Value::U32(41),
            "the second show replaces the first"
        );
        let closes: Vec<&Message> = calls
            .iter()
            .filter(|c| c.is(SERVICE, "CloseNotification"))
            .collect();
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0].body, vec![Value::U32(41)]);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![SinkEvent::Clicked {
                id: id(),
                token: Some("token-1".to_owned())
            }]
        );
    }

    #[test]
    fn the_notify_call_replaces_by_number_and_offers_a_default_action() {
        let call = notify_call(7, "b pane1", "x", &["default", "Open"]);
        assert!(call.is(SERVICE, "Notify"));
        assert_eq!(call.body[1], Value::U32(7));
        assert_eq!(call.body[3], Value::str("b pane1"));
        assert_eq!(call.body[5], Value::strings(["default", "Open"]));
        assert_eq!(call.body[7], Value::I32(-1));
        let (signature, _) = crate::dbus::encode_body(&call.body);
        assert_eq!(signature, "susssasa{sv}i");
    }
}
