use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use windows::core::{IInspectable, Ref, HSTRING};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager, ToastNotifier};

use super::{escape_markup, Events, Sink, SinkEvent};
use crate::app::bundle::WINDOWS_APP_ID;
use crate::app::engine::{Content, NotifyId};

pub struct Toasts {
    notifier: ToastNotifier,
    events: Events,
    known: Arc<Mutex<HashMap<String, NotifyId>>>,
}

fn short_tag(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:016x}", hash)
}

pub fn toast_xml(content: &Content) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_markup(&content.title),
        escape_markup(&content.body)
    )
}

impl Toasts {
    pub fn open(events: Events) -> Result<Toasts> {
        unsafe {
            let _ = windows::Win32::System::WinRT::RoInitialize(
                windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
            );
        }
        crate::app::platform_windows::register_app_id();
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(WINDOWS_APP_ID))
                .map_err(|e| anyhow!("no toast notifier: {}", e))?;
        Ok(Toasts {
            notifier,
            events,
            known: Arc::default(),
        })
    }

    fn post(
        &self,
        tag: Option<(&str, &str)>,
        id: Option<&NotifyId>,
        content: &Content,
    ) -> Result<()> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(toast_xml(content)))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        if let Some((tag, group)) = tag {
            toast.SetTag(&HSTRING::from(tag))?;
            toast.SetGroup(&HSTRING::from(group))?;
        }
        if let Some(id) = id {
            let events = self.events.clone();
            let id = id.clone();
            toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                move |_: Ref<ToastNotification>, _: Ref<IInspectable>| {
                    events(SinkEvent::Clicked {
                        id: id.clone(),
                        token: None,
                    });
                    Ok(())
                },
            ))?;
        }
        self.notifier.Show(&toast)?;
        Ok(())
    }
}

impl Sink for Toasts {
    fn show(&mut self, id: &NotifyId, content: &Content) {
        let encoded = id.encode();
        let tag = short_tag(&encoded);
        let group = short_tag(id.session());
        if let Ok(mut known) = self.known.lock() {
            known.insert(tag.clone(), id.clone());
        }
        if let Err(e) = self.post(Some((&tag, &group)), Some(id), content) {
            report!("a toast could not be shown: {}", e);
        }
    }

    fn withdraw(&mut self, id: &NotifyId) {
        let tag = short_tag(&id.encode());
        let group = short_tag(id.session());
        if let Ok(mut known) = self.known.lock() {
            known.remove(&tag);
        }
        let removed = ToastNotificationManager::History().and_then(|history| {
            history.RemoveGroupedTagWithId(
                &HSTRING::from(tag),
                &HSTRING::from(group),
                &HSTRING::from(WINDOWS_APP_ID),
            )
        });
        if let Err(e) = removed {
            report!("a toast could not be withdrawn: {}", e);
        }
    }

    fn simple(&mut self, title: &str, body: &str) {
        let content = Content {
            title: title.to_owned(),
            body: body.to_owned(),
        };
        if let Err(e) = self.post(None, None, &content) {
            report!("a toast could not be shown: {}", e);
        }
    }
}
