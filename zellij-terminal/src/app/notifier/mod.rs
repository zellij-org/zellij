use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;

use crate::app::engine::{Command, Content, NotifyId};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkEvent {
    Clicked { id: NotifyId, token: Option<String> },
    Dismissed(NotifyId),
}

pub type Events = Arc<dyn Fn(SinkEvent) + Send + Sync>;

pub trait Sink {
    fn show(&mut self, id: &NotifyId, content: &Content);
    fn withdraw(&mut self, id: &NotifyId);
    fn simple(&mut self, title: &str, body: &str);
}

enum Op {
    Apply(Command),
    Simple { title: String, body: String },
}

pub struct Notifier {
    ops: Sender<Op>,
}

impl Notifier {
    pub fn spawn(events: Events) -> Notifier {
        Notifier::spawn_with(move || open(events))
    }

    pub fn spawn_with(open: impl FnOnce() -> Option<Box<dyn Sink>> + Send + 'static) -> Notifier {
        let (ops, received) = channel::<Op>();
        let _ = std::thread::Builder::new()
            .name("app_notifier".to_owned())
            .spawn(move || {
                let mut sink = open();
                for op in received {
                    let Some(sink) = sink.as_mut() else {
                        continue;
                    };
                    match op {
                        Op::Apply(Command::Show { id, content }) => sink.show(&id, &content),
                        Op::Apply(Command::Withdraw { id }) => sink.withdraw(&id),
                        Op::Simple { title, body } => sink.simple(&title, &body),
                    }
                }
            });
        Notifier { ops }
    }

    pub fn apply(&self, commands: Vec<Command>) {
        for command in commands {
            let _ = self.ops.send(Op::Apply(command));
        }
    }

    pub fn simple(&self, title: &str, body: &str) {
        let _ = self.ops.send(Op::Simple {
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }
}

#[cfg(target_os = "linux")]
fn open(events: Events) -> Option<Box<dyn Sink>> {
    match linux::Freedesktop::open(events) {
        Ok(sink) => Some(Box::new(sink)),
        Err(e) => {
            report!("desktop notifications are off: {:#}", e);
            None
        },
    }
}

#[cfg(target_os = "macos")]
fn open(events: Events) -> Option<Box<dyn Sink>> {
    match macos::UserNotifications::open(events) {
        Ok(sink) => Some(Box::new(sink)),
        Err(e) => {
            report!("desktop notifications are off: {:#}", e);
            None
        },
    }
}

#[cfg(windows)]
fn open(events: Events) -> Option<Box<dyn Sink>> {
    match windows::Toasts::open(events) {
        Ok(sink) => Some(Box::new(sink)),
        Err(e) => {
            report!("desktop notifications are off: {:#}", e);
            None
        },
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn open(_events: Events) -> Option<Box<dyn Sink>> {
    None
}

#[cfg_attr(not(any(target_os = "linux", windows, test)), allow(dead_code))]
pub fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
pub mod recording {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Recorded {
        Show(NotifyId, Content),
        Withdraw(NotifyId),
        Simple(String, String),
    }

    #[derive(Clone, Default)]
    pub struct Recorder(pub Arc<Mutex<Vec<Recorded>>>);

    impl Recorder {
        pub fn taken(&self) -> Vec<Recorded> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    impl Sink for Recorder {
        fn show(&mut self, id: &NotifyId, content: &Content) {
            self.0
                .lock()
                .unwrap()
                .push(Recorded::Show(id.clone(), content.clone()));
        }
        fn withdraw(&mut self, id: &NotifyId) {
            self.0.lock().unwrap().push(Recorded::Withdraw(id.clone()));
        }
        fn simple(&mut self, title: &str, body: &str) {
            self.0
                .lock()
                .unwrap()
                .push(Recorded::Simple(title.to_owned(), body.to_owned()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recording::*;
    use super::*;
    use crate::app::engine::ItemKey;
    use std::time::{Duration, Instant};
    use zellij_utils::data::PaneId;

    #[test]
    fn markup_is_escaped_so_untrusted_text_stays_text() {
        assert_eq!(
            escape_markup("<b>x</b> & 'y' \"z\""),
            "&lt;b&gt;x&lt;/b&gt; &amp; &apos;y&apos; &quot;z&quot;"
        );
    }

    #[test]
    fn commands_reach_the_sink_in_order_on_its_own_thread() {
        let recorder = Recorder::default();
        let sink = recorder.clone();
        let notifier = Notifier::spawn_with(move || Some(Box::new(sink) as Box<dyn Sink>));
        let id = NotifyId::Item(ItemKey {
            session: "b".to_owned(),
            pane: PaneId::Terminal(1),
        });
        let content = Content {
            title: "b pane1".to_owned(),
            body: "x".to_owned(),
        };
        notifier.apply(vec![
            Command::Show {
                id: id.clone(),
                content: content.clone(),
            },
            Command::Withdraw { id: id.clone() },
        ]);
        notifier.simple("Zellij", "hello");
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen = vec![];
        while seen.len() < 3 && Instant::now() < deadline {
            seen.extend(recorder.taken());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            seen,
            vec![
                Recorded::Show(id.clone(), content),
                Recorded::Withdraw(id),
                Recorded::Simple("Zellij".to_owned(), "hello".to_owned()),
            ]
        );
    }
}
