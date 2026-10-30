use anyhow::{bail, Result};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_foundation::{NSArray, NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
    UNNotificationPresentationOptions, UNNotificationRequest, UNNotificationResponse,
    UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::{Events, Sink, SinkEvent};
use crate::app::engine::{Content, NotifyId};

pub struct DelegateIvars {
    events: Events,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ZellijNotificationDelegate"]
    #[ivars = DelegateIvars]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            completion_handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            completion_handler
                .call((UNNotificationPresentationOptions::Banner
                    | UNNotificationPresentationOptions::List,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            completion_handler: &block2::DynBlock<dyn Fn()>,
        ) {
            let identifier = response.notification().request().identifier().to_string();
            if let Some(id) = NotifyId::decode(&identifier) {
                (self.ivars().events)(SinkEvent::Clicked { id, token: None });
            }
            completion_handler.call(());
        }
    }
);

impl Delegate {
    fn new(events: Events) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { events });
        unsafe { msg_send![super(this), init] }
    }
}

pub struct UserNotifications {
    center: Retained<UNUserNotificationCenter>,
    _delegate: Retained<Delegate>,
}

impl UserNotifications {
    pub fn open(events: Events) -> Result<UserNotifications> {
        if let Some(reason) = crate::app::bundle::notifications_need_a_bundle() {
            bail!("{}", reason);
        }
        let center = UNUserNotificationCenter::currentNotificationCenter();
        let delegate = Delegate::new(events);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let answered = RcBlock::new(|granted: Bool, _error: *mut NSError| {
            if !granted.as_bool() {
                report!("macOS did not allow Zellij to show notifications");
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &answered,
        );
        Ok(UserNotifications {
            center,
            _delegate: delegate,
        })
    }

    fn deliver(&self, identifier: &str, thread: &str, content: &Content) {
        let body = UNMutableNotificationContent::new();
        body.setTitle(&NSString::from_str(&content.title));
        body.setBody(&NSString::from_str(&content.body));
        body.setThreadIdentifier(&NSString::from_str(thread));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(identifier),
            &body,
            None,
        );
        self.center
            .addNotificationRequest_withCompletionHandler(&request, None);
    }
}

impl Sink for UserNotifications {
    fn show(&mut self, id: &NotifyId, content: &Content) {
        self.deliver(&id.encode(), id.session(), content);
    }

    fn withdraw(&mut self, id: &NotifyId) {
        let identifiers = NSArray::from_retained_slice(&[NSString::from_str(&id.encode())]);
        self.center
            .removeDeliveredNotificationsWithIdentifiers(&identifiers);
        self.center
            .removePendingNotificationRequestsWithIdentifiers(&identifiers);
    }

    fn simple(&mut self, title: &str, body: &str) {
        let content = Content {
            title: title.to_owned(),
            body: body.to_owned(),
        };
        self.deliver("notice", "zellij", &content);
    }
}
