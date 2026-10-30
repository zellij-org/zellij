use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use zellij_utils::input::attention::{StatusSet, POPUP_STATUSES};

use crate::app::engine::{Command, Engine, Policy};
use crate::app::watcher::{Snapshot, Watcher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeEvent {
    Shown {
        id: String,
        title: String,
        body: String,
    },
    Withdrawn {
        id: String,
    },
}

pub struct NotificationProbe {
    events: Arc<Mutex<Vec<ProbeEvent>>>,
    latest: Arc<Mutex<Snapshot>>,
    watcher: Watcher,
}

impl NotificationProbe {
    pub fn start(sock_dir: PathBuf, details: bool) -> Result<NotificationProbe> {
        let (snapshots, received) = channel::<Snapshot>();
        let watcher = Watcher::spawn(sock_dir, details, move |snapshot| {
            let _ = snapshots.send(snapshot);
        })?;
        let events: Arc<Mutex<Vec<ProbeEvent>>> = Arc::default();
        let latest: Arc<Mutex<Snapshot>> = Arc::default();
        let policy = Policy {
            statuses: StatusSet::of(&POPUP_STATUSES),
            details,
            ..Policy::default()
        };
        let recorded = events.clone();
        let kept = latest.clone();
        std::thread::Builder::new()
            .name("notification_probe".to_owned())
            .spawn(move || {
                let mut engine = Engine::new();
                loop {
                    let now = Instant::now();
                    let wait = engine
                        .next_deadline()
                        .map(|at| at.saturating_duration_since(now))
                        .unwrap_or(Duration::from_millis(250))
                        .min(Duration::from_millis(250));
                    let commands = match received.recv_timeout(wait) {
                        Ok(snapshot) => {
                            let commands = engine.update(&snapshot.global, policy, Instant::now());
                            if let Ok(mut kept) = kept.lock() {
                                *kept = snapshot;
                            }
                            commands
                        },
                        Err(RecvTimeoutError::Timeout) => engine.tick(Instant::now()),
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    if let Ok(mut recorded) = recorded.lock() {
                        recorded.extend(commands.into_iter().map(|command| match command {
                            Command::Show { id, content } => ProbeEvent::Shown {
                                id: id.encode(),
                                title: content.title,
                                body: content.body,
                            },
                            Command::Withdraw { id } => ProbeEvent::Withdrawn { id: id.encode() },
                        }));
                    }
                }
            })?;
        Ok(NotificationProbe {
            events,
            latest,
            watcher,
        })
    }

    pub fn events(&self) -> Vec<ProbeEvent> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn live_sessions(&self) -> Vec<String> {
        self.watcher.live_sessions()
    }

    pub fn tracked_panes(&self, session: &str) -> Vec<String> {
        self.latest
            .lock()
            .map(|snapshot| {
                snapshot
                    .global
                    .items
                    .iter()
                    .filter(|item| item.session == session)
                    .map(|item| item.item.pane_id.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn item_id(session: &str, pane: &str) -> String {
        format!("item/{}/{}", pane, session)
    }
}
