//! The light/dark preference of a Linux desktop, as the XDG desktop portal
//! publishes it. winit reports no system theme on X11 or Wayland, so the
//! portal's `org.freedesktop.appearance` `color-scheme` setting is read
//! directly: once at startup, and again whenever the desktop signals a change.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use zellij_utils::data::HostTerminalThemeMode;

use super::dbus::{match_rule, Bus, Handler, Kind, Message, Value};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";
const CALL_TIMEOUT: Duration = Duration::from_millis(1500);

/// The preference last read, as the portal numbers it: 1 dark, 2 light,
/// anything else none. Windows opened after a change read it from here.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> Option<HostTerminalThemeMode> {
    mode_of(CURRENT.load(Ordering::Relaxed) as u32)
}

/// The portal's number for a preference. 0 means the user has none, and
/// values beyond 2 are ones this does not know; both leave the theme to the
/// session's own default.
fn mode_of(value: u32) -> Option<HostTerminalThemeMode> {
    match value {
        1 => Some(HostTerminalThemeMode::Dark),
        2 => Some(HostTerminalThemeMode::Light),
        _ => None,
    }
}

type Post = Arc<dyn Fn(Option<HostTerminalThemeMode>) + Send + Sync>;

fn publish(value: u32, post: &Post) {
    CURRENT.store(value.min(u8::MAX as u32) as u8, Ordering::Relaxed);
    post(mode_of(value));
}

/// The new preference a `SettingChanged` signal carries, if it is about the
/// colour scheme.
fn changed(message: &Message) -> Option<u32> {
    if message.kind != Kind::Signal || !message.is(SETTINGS, "SettingChanged") {
        return None;
    }
    let namespace = message.body.first().and_then(Value::as_str)?;
    let key = message.body.get(1).and_then(Value::as_str)?;
    if namespace != NAMESPACE || key != KEY {
        return None;
    }
    message.body.get(2).and_then(Value::as_u32)
}

/// Reads the preference, asking with `ReadOne` and, for a portal too old to
/// know it, with `Read`. Either answer is a variant, the older one a variant
/// within a variant, which `Value::as_u32` sees through.
fn read(bus: &Bus) -> Option<u32> {
    ["ReadOne", "Read"].into_iter().find_map(|method| {
        let call = Message::call(
            SERVICE,
            PATH,
            SETTINGS,
            method,
            vec![Value::str(NAMESPACE), Value::str(KEY)],
        );
        bus.call(call, CALL_TIMEOUT)
            .ok()
            .and_then(|reply| reply.body.first().and_then(Value::as_u32))
    })
}

/// Follows the desktop's preference on a background thread, calling `post`
/// with it at startup and with each change. A desktop with no portal is
/// left alone: `post` is never called.
pub fn watch(post: impl Fn(Option<HostTerminalThemeMode>) + Send + Sync + 'static) {
    let post: Post = Arc::new(post);
    let spawned = std::thread::Builder::new()
        .name("system_appearance".to_owned())
        .spawn(move || {
            if let Err(e) = follow(Bus::session, post) {
                report!("the system appearance is not followed: {}", e);
            }
        });
    if let Err(e) = spawned {
        report!("the system appearance is not followed: {}", e);
    }
}

fn follow(
    connect: impl FnOnce(Box<dyn FnOnce(Bus) -> Handler>) -> Result<Bus>,
    post: Post,
) -> Result<Bus> {
    let on_signal = post.clone();
    let bus = connect(Box::new(move |_bus| {
        Box::new(move |message: Message| {
            if let Some(value) = changed(&message) {
                publish(value, &on_signal);
            }
        })
    }))?;
    bus.add_match(&match_rule(SETTINGS, Some("SettingChanged")))?;
    if let Some(value) = read(&bus) {
        publish(value, &post);
    }
    Ok(bus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::desktop::dbus::fake::FakeBus;
    use std::os::unix::net::UnixStream;
    use std::sync::Mutex;
    use std::time::Instant;

    fn setting(namespace: &str, key: &str, value: u32) -> Message {
        Message::signal(
            PATH,
            SETTINGS,
            "SettingChanged",
            vec![
                Value::str(namespace),
                Value::str(key),
                Value::variant(Value::U32(value)),
            ],
        )
    }

    #[test]
    fn the_portal_numbers_map_to_modes_and_no_preference_to_none() {
        assert_eq!(mode_of(1), Some(HostTerminalThemeMode::Dark));
        assert_eq!(mode_of(2), Some(HostTerminalThemeMode::Light));
        assert_eq!(mode_of(0), None);
        assert_eq!(mode_of(3), None);
    }

    #[test]
    fn only_a_change_to_the_colour_scheme_is_taken_up() {
        assert_eq!(changed(&setting(NAMESPACE, KEY, 2)), Some(2));
        assert_eq!(changed(&setting(NAMESPACE, "accent-color", 2)), None);
        assert_eq!(
            changed(&setting("org.gnome.desktop.interface", KEY, 2)),
            None
        );
        let other_signal = Message::signal(PATH, SETTINGS, "Other", vec![]);
        assert_eq!(changed(&other_signal), None);
    }

    fn posted() -> (Post, Arc<Mutex<Vec<Option<HostTerminalThemeMode>>>>) {
        let seen = Arc::new(Mutex::new(vec![]));
        let record = seen.clone();
        let post: Post = Arc::new(move |mode| record.lock().unwrap().push(mode));
        (post, seen)
    }

    fn wait_for(seen: &Mutex<Vec<Option<HostTerminalThemeMode>>>, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while seen.lock().unwrap().len() < count && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_startup_preference_and_each_later_change_are_posted() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let portal = FakeBus::serve(theirs, |call, _| {
            call.is(SETTINGS, "ReadOne")
                .then(|| vec![Value::variant(Value::U32(1))])
        });
        let (post, seen) = posted();
        let _bus = follow(move |make| Bus::over(ours, make), post).unwrap();
        wait_for(&seen, 1);
        portal.signal(setting(NAMESPACE, KEY, 2));
        portal.signal(setting(NAMESPACE, "contrast", 1));
        portal.signal(setting(NAMESPACE, KEY, 0));
        wait_for(&seen, 3);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Some(HostTerminalThemeMode::Dark),
                Some(HostTerminalThemeMode::Light),
                None
            ]
        );
        let rules = portal.calls();
        assert!(rules
            .iter()
            .any(|call| call.is("org.freedesktop.DBus", "AddMatch")));
    }

    #[test]
    fn a_portal_that_only_knows_the_older_read_is_asked_with_it() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let _portal = FakeBus::serve(theirs, |call, _| {
            call.is(SETTINGS, "Read")
                .then(|| vec![Value::variant(Value::variant(Value::U32(2)))])
        });
        let (post, seen) = posted();
        let _bus = follow(move |make| Bus::over(ours, make), post).unwrap();
        wait_for(&seen, 1);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![Some(HostTerminalThemeMode::Light)]
        );
    }

    #[test]
    fn a_desktop_with_no_portal_posts_nothing() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let _portal = FakeBus::serve(theirs, |_, _| None);
        let (post, seen) = posted();
        let _bus = follow(move |make| Bus::over(ours, make), post).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(seen.lock().unwrap().is_empty());
    }
}
