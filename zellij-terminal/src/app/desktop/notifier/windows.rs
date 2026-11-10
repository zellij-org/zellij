use anyhow::{anyhow, Result};
use windows::core::{IInspectable, Ref, HSTRING};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::{
    NotificationSetting, ToastDismissalReason, ToastDismissedEventArgs, ToastNotification,
    ToastNotificationManager, ToastNotifier,
};

use super::{escape_markup, Events, Sink, SinkEvent};
use crate::app::policy::engine::{Content, NotifyId};
use crate::config::identity::identity;

pub struct Toasts {
    notifier: ToastNotifier,
    app_id: HSTRING,
    events: Events,
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

fn app_notifier() -> Result<(ToastNotifier, HSTRING)> {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
    let app_id = HSTRING::from(identity().windows_app_id());
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&app_id)
        .map_err(|e| anyhow!("no toast notifier: {}", e))?;
    Ok((notifier, app_id))
}

/// Whether Windows lets the app's toasts appear. Until Windows has shown the app's first toast,
/// it cannot tell and answers with an error.
pub fn notification_setting() -> Result<NotificationSetting> {
    let (notifier, _) = app_notifier()?;
    Ok(notifier.Setting()?)
}

/// A toast that times out still waits in the notification centre, and one the app withdraws is
/// not the user's doing, so only the user closing it counts as a dismissal.
fn dismissed_by_user(reason: ToastDismissalReason) -> bool {
    reason == ToastDismissalReason::UserCanceled
}

impl Toasts {
    pub fn open(events: Events) -> Result<Toasts> {
        let (notifier, app_id) = app_notifier()?;
        Ok(Toasts {
            notifier,
            app_id,
            events,
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
            self.watch(&toast, id)?;
        }
        self.notifier.Show(&toast)?;
        Ok(())
    }

    fn watch(&self, toast: &ToastNotification, id: &NotifyId) -> Result<()> {
        let events = self.events.clone();
        let clicked = id.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_: Ref<ToastNotification>, _: Ref<IInspectable>| {
                events(SinkEvent::Clicked {
                    id: clicked.clone(),
                    token: None,
                });
                Ok(())
            },
        ))?;
        let events = self.events.clone();
        let dismissed = id.clone();
        toast.Dismissed(&TypedEventHandler::<
            ToastNotification,
            ToastDismissedEventArgs,
        >::new(
            move |_: Ref<ToastNotification>, args: Ref<ToastDismissedEventArgs>| {
                if dismissed_by_user(args.ok()?.Reason()?) {
                    events(SinkEvent::Dismissed(dismissed.clone()));
                }
                Ok(())
            },
        ))?;
        Ok(())
    }
}

impl Sink for Toasts {
    fn show(&mut self, id: &NotifyId, content: &Content) {
        let tag = short_tag(&id.encode());
        let group = short_tag(id.session());
        if let Err(e) = self.post(Some((&tag, &group)), Some(id), content) {
            report!("a toast could not be shown: {}", e);
        }
    }

    fn withdraw(&mut self, id: &NotifyId) {
        let tag = short_tag(&id.encode());
        let group = short_tag(id.session());
        let removed = ToastNotificationManager::History().and_then(|history| {
            history.RemoveGroupedTagWithId(&HSTRING::from(tag), &HSTRING::from(group), &self.app_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_user_closing_a_toast_counts_as_a_dismissal() {
        assert!(dismissed_by_user(ToastDismissalReason::UserCanceled));
        assert!(
            !dismissed_by_user(ToastDismissalReason::TimedOut),
            "a toast that times out still waits in the notification centre"
        );
        assert!(
            !dismissed_by_user(ToastDismissalReason::ApplicationHidden),
            "a toast the app withdrew is not the user's doing"
        );
    }
}
