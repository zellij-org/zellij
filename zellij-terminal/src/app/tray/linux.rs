use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Result};

use super::{find_action, menu, Anchor, MenuEntry, Tray, TrayEvent, TrayEvents};
use crate::app::icon::{IconCache, SNI_SIZES};
use crate::app::model::TrayState;
use crate::dbus::{
    match_rule, Bus, Handler, Kind, Message, Value, BUS_SERVICE, INTROSPECTABLE, PROPERTIES,
};

pub const ITEM_PATH: &str = "/StatusNotifierItem";
pub const MENU_PATH: &str = "/MenuBar";
pub const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
pub const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
pub const WATCHER_SERVICE: &str = "org.kde.StatusNotifierWatcher";
pub const WATCHER_PATH: &str = "/StatusNotifierWatcher";
pub const NEUTRAL_INK: [u8; 3] = [168, 168, 168];
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

pub struct Shared {
    tooltip: String,
    pixmaps: Value,
    paused: bool,
    revision: u32,
    anchor: Option<(i32, i32)>,
    token: Option<String>,
}

type MakeHandler = Box<dyn FnOnce(Bus) -> Handler>;
type Dial = Box<dyn FnMut(MakeHandler) -> Result<Bus>>;

pub struct StatusNotifier {
    dial: Dial,
    events: TrayEvents,
    shared: Arc<Mutex<Shared>>,
    bus: Option<Bus>,
    cache: IconCache,
    visible: bool,
    service: String,
    complained: bool,
}

fn pixmaps(cache: &mut IconCache, state: &TrayState) -> Value {
    Value::Array(
        "(iiay)".to_owned(),
        SNI_SIZES
            .iter()
            .map(|size| {
                let image = cache.get(*size, &state.look, NEUTRAL_INK);
                Value::Struct(vec![
                    Value::I32(image.width as i32),
                    Value::I32(image.height as i32),
                    Value::bytes(&image.to_argb_big_endian()),
                ])
            })
            .collect(),
    )
}

impl StatusNotifier {
    pub fn open(events: TrayEvents, state: &TrayState) -> StatusNotifier {
        StatusNotifier::open_with(Box::new(|make| Bus::session(make)), events, state)
    }

    #[cfg(test)]
    fn over(
        stream: std::os::unix::net::UnixStream,
        events: TrayEvents,
        state: &TrayState,
    ) -> StatusNotifier {
        let mut stream = Some(stream);
        StatusNotifier::open_with(
            Box::new(move |make| match stream.take() {
                Some(stream) => Bus::over(stream, make),
                None => bail!("the test bus was used already"),
            }),
            events,
            state,
        )
    }

    fn open_with(dial: Dial, events: TrayEvents, state: &TrayState) -> StatusNotifier {
        let mut cache = IconCache::default();
        let shared = Arc::new(Mutex::new(Shared {
            tooltip: state.tooltip.clone(),
            pixmaps: pixmaps(&mut cache, state),
            paused: state.paused,
            revision: 1,
            anchor: None,
            token: None,
        }));
        let mut tray = StatusNotifier {
            dial,
            events,
            shared,
            bus: None,
            cache,
            visible: false,
            service: format!("org.kde.StatusNotifierItem-{}-1", std::process::id()),
            complained: false,
        };
        tray.set_visible(true);
        tray
    }

    fn connect(&mut self) -> Result<Bus> {
        let shared = self.shared.clone();
        let events = self.events.clone();
        let service = self.service.clone();
        let bus = (self.dial)(Box::new(move |bus| {
            Box::new(move |message: Message| {
                handle(&bus, &message, &shared, &events, &service);
            })
        }))?;
        let claimed = bus.request_name(&self.service).and_then(|claimed| {
            if !claimed {
                bail!("the bus name {} is taken", self.service);
            }
            bus.add_match(&format!(
                "{},arg0='{}'",
                match_rule(BUS_SERVICE, Some("NameOwnerChanged")),
                WATCHER_SERVICE
            ))
        });
        if let Err(e) = claimed {
            bus.shutdown();
            return Err(e);
        }
        if let Err(e) = register(&bus, &self.service) {
            if !self.complained {
                self.complained = true;
                report!("the tray icon waits for a tray host: {:#}", e);
            }
        }
        Ok(bus)
    }

    fn signal(&self, interface: &str, member: &str, body: Vec<Value>) {
        if let Some(bus) = &self.bus {
            let _ = bus.send(&Message::signal(
                if interface == MENU_INTERFACE {
                    MENU_PATH
                } else {
                    ITEM_PATH
                },
                interface,
                member,
                body,
            ));
        }
    }
}

fn register(bus: &Bus, service: &str) -> Result<()> {
    if !bus.name_has_owner(WATCHER_SERVICE) {
        bail!(
            "no tray host is running (on GNOME, install the AppIndicator and KStatusNotifierItem \
             extension); notifications still work"
        );
    }
    bus.call(
        Message::call(
            WATCHER_SERVICE,
            WATCHER_PATH,
            WATCHER_SERVICE,
            "RegisterStatusNotifierItem",
            vec![Value::str(service)],
        ),
        CALL_TIMEOUT,
    )?;
    Ok(())
}

impl Tray for StatusNotifier {
    fn update(&mut self, state: &TrayState) {
        let pixmaps = pixmaps(&mut self.cache, state);
        let (icon_changed, tooltip_changed, menu_changed) = {
            let Ok(mut shared) = self.shared.lock() else {
                return;
            };
            let icon_changed = shared.pixmaps != pixmaps;
            let tooltip_changed = shared.tooltip != state.tooltip;
            let menu_changed = shared.paused != state.paused;
            shared.pixmaps = pixmaps;
            shared.tooltip = state.tooltip.clone();
            if menu_changed {
                shared.paused = state.paused;
                shared.revision += 1;
            }
            (
                icon_changed,
                tooltip_changed,
                menu_changed.then_some(shared.revision),
            )
        };
        if icon_changed {
            self.signal(ITEM_INTERFACE, "NewIcon", vec![]);
        }
        if tooltip_changed {
            self.signal(ITEM_INTERFACE, "NewToolTip", vec![]);
        }
        if let Some(revision) = menu_changed {
            self.signal(
                MENU_INTERFACE,
                "LayoutUpdated",
                vec![Value::U32(revision), Value::I32(0)],
            );
        }
    }

    fn set_visible(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if !visible {
            if let Some(bus) = self.bus.take() {
                bus.shutdown();
            }
            return;
        }
        match self.connect() {
            Ok(bus) => self.bus = Some(bus),
            Err(e) => {
                if !self.complained {
                    self.complained = true;
                    report!("the tray icon is not shown: {:#}", e);
                }
            },
        }
    }

    fn anchor(&self) -> Option<Anchor> {
        let (x, y) = self.shared.lock().ok()?.anchor?;
        Some(Anchor::At {
            x: x as f64,
            y: y as f64,
        })
    }
}

impl Drop for StatusNotifier {
    fn drop(&mut self) {
        if let Some(bus) = self.bus.take() {
            bus.shutdown();
        }
    }
}

fn handle(
    bus: &Bus,
    message: &Message,
    shared: &Mutex<Shared>,
    events: &TrayEvents,
    service: &str,
) {
    if message.kind == Kind::Signal {
        if message.is(BUS_SERVICE, "NameOwnerChanged")
            && message.body.first().and_then(Value::as_str) == Some(WATCHER_SERVICE)
            && message
                .body
                .get(2)
                .and_then(Value::as_str)
                .map(|owner| !owner.is_empty())
                .unwrap_or(false)
        {
            let bus = bus.clone();
            let service = service.to_owned();
            let _ = std::thread::Builder::new()
                .name("sni_register".to_owned())
                .spawn(move || {
                    let _ = register(&bus, &service);
                });
        }
        return;
    }
    if message.kind != Kind::MethodCall {
        return;
    }
    let reply = match answer(message, shared, events) {
        Some(Ok(body)) => Message::reply_to(message, body),
        Some(Err(text)) => {
            Message::error_to(message, "org.freedesktop.DBus.Error.InvalidArgs", &text)
        },
        None => Message::error_to(
            message,
            "org.freedesktop.DBus.Error.UnknownMethod",
            "unknown method",
        ),
    };
    if !message.no_reply {
        let _ = bus.send(&reply);
    }
}

fn answer(
    message: &Message,
    shared: &Mutex<Shared>,
    events: &TrayEvents,
) -> Option<Result<Vec<Value>, String>> {
    let path = message.path.as_deref().unwrap_or("");
    let interface = message.interface.as_deref().unwrap_or("");
    let member = message.member.as_deref().unwrap_or("");
    let mut shared = shared.lock().ok()?;
    match (path, interface, member) {
        (_, "org.freedesktop.DBus.Peer", "Ping") => Some(Ok(vec![])),
        (_, INTROSPECTABLE, "Introspect") => Some(Ok(vec![Value::str(introspection(path))])),
        (ITEM_PATH, PROPERTIES, "GetAll") => Some(Ok(vec![Value::Array(
            "{sv}".to_owned(),
            item_properties(&shared)
                .into_iter()
                .map(|(name, value)| {
                    Value::DictEntry(Box::new(Value::str(name)), Box::new(Value::variant(value)))
                })
                .collect(),
        )])),
        (ITEM_PATH, PROPERTIES, "Get") => {
            let name = message.body.get(1).and_then(Value::as_str).unwrap_or("");
            Some(
                item_properties(&shared)
                    .into_iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, value)| vec![Value::variant(value)])
                    .ok_or_else(|| format!("no property {}", name)),
            )
        },
        (ITEM_PATH, ITEM_INTERFACE, "Activate" | "SecondaryActivate") => {
            let x = message.body.first().and_then(Value::as_i32);
            let y = message.body.get(1).and_then(Value::as_i32);
            let at = x.zip(y).filter(|(x, y)| *x != 0 || *y != 0);
            let token = shared.token.take();
            drop(shared);
            events(TrayEvent::Activate { at, token });
            Some(Ok(vec![]))
        },
        (ITEM_PATH, ITEM_INTERFACE, "ProvideXdgActivationToken") => {
            shared.token = message
                .body
                .first()
                .and_then(Value::as_str)
                .map(str::to_owned);
            Some(Ok(vec![]))
        },
        (ITEM_PATH, ITEM_INTERFACE, "ContextMenu" | "Scroll") => Some(Ok(vec![])),
        (MENU_PATH, PROPERTIES, "GetAll") => Some(Ok(vec![Value::dict(menu_properties())])),
        (MENU_PATH, PROPERTIES, "Get") => {
            let name = message.body.get(1).and_then(Value::as_str).unwrap_or("");
            Some(
                menu_properties()
                    .into_iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, value)| vec![Value::variant(value)])
                    .ok_or_else(|| format!("no property {}", name)),
            )
        },
        (MENU_PATH, MENU_INTERFACE, "GetLayout") => {
            let parent = message.body.first().and_then(Value::as_i32).unwrap_or(0);
            let entries = menu(shared.paused);
            Some(Ok(vec![
                Value::U32(shared.revision),
                menu_layout(&entries, parent),
            ]))
        },
        (MENU_PATH, MENU_INTERFACE, "GetGroupProperties") => {
            let entries = menu(shared.paused);
            let ids: Vec<i32> = message
                .body
                .first()
                .map(|ids| ids.items().iter().filter_map(Value::as_i32).collect())
                .unwrap_or_default();
            let all = flatten(&entries);
            let chosen: Vec<Value> = all
                .iter()
                .filter(|(id, _)| ids.is_empty() || ids.contains(id))
                .map(|(id, entry)| Value::Struct(vec![Value::I32(*id), entry_properties(entry)]))
                .collect();
            Some(Ok(vec![Value::Array("(ia{sv})".to_owned(), chosen)]))
        },
        (MENU_PATH, MENU_INTERFACE, "GetProperty") => {
            let id = message.body.first().and_then(Value::as_i32).unwrap_or(-1);
            let name = message.body.get(1).and_then(Value::as_str).unwrap_or("");
            let entries = menu(shared.paused);
            let found = flatten(&entries)
                .into_iter()
                .find(|(i, _)| *i == id)
                .and_then(|(_, entry)| {
                    entry_properties(entry)
                        .items()
                        .iter()
                        .find_map(|pair| match pair {
                            Value::DictEntry(key, value) if key.as_str() == Some(name) => {
                                Some((**value).clone())
                            },
                            _ => None,
                        })
                });
            Some(
                found
                    .map(|value| vec![value])
                    .ok_or_else(|| "no such property".to_owned()),
            )
        },
        (MENU_PATH, MENU_INTERFACE, "Event") => {
            let id = message.body.first().and_then(Value::as_i32).unwrap_or(-1);
            let event = message.body.get(1).and_then(Value::as_str).unwrap_or("");
            let action = (event == "clicked")
                .then(|| menu_action_for(shared.paused, id))
                .flatten();
            drop(shared);
            if let Some(action) = action {
                events(TrayEvent::Menu(action));
            }
            Some(Ok(vec![]))
        },
        (MENU_PATH, MENU_INTERFACE, "EventGroup") => {
            let paused = shared.paused;
            drop(shared);
            if let Some(group) = message.body.first() {
                for event in group.items() {
                    let parts = event.items();
                    let id = parts.first().and_then(Value::as_i32).unwrap_or(-1);
                    let kind = parts.get(1).and_then(Value::as_str).unwrap_or("");
                    if kind == "clicked" {
                        if let Some(action) = menu_action_for(paused, id) {
                            events(TrayEvent::Menu(action));
                        }
                    }
                }
            }
            Some(Ok(vec![Value::Array("i".to_owned(), vec![])]))
        },
        (MENU_PATH, MENU_INTERFACE, "AboutToShow") => Some(Ok(vec![Value::Bool(false)])),
        (MENU_PATH, MENU_INTERFACE, "AboutToShowGroup") => Some(Ok(vec![
            Value::Array("i".to_owned(), vec![]),
            Value::Array("i".to_owned(), vec![]),
        ])),
        _ => None,
    }
}

pub fn menu_action_for(paused: bool, id: i32) -> Option<super::MenuAction> {
    find_action(&menu(paused), id)
}

fn item_properties(shared: &Shared) -> Vec<(&'static str, Value)> {
    vec![
        ("Category", Value::str("ApplicationStatus")),
        ("Id", Value::str(crate::window::application_id())),
        ("Title", Value::str("Zellij")),
        ("Status", Value::str("Active")),
        ("WindowId", Value::I32(0)),
        ("IconName", Value::str("")),
        ("IconThemePath", Value::str("")),
        ("IconPixmap", shared.pixmaps.clone()),
        ("OverlayIconName", Value::str("")),
        (
            "OverlayIconPixmap",
            Value::Array("(iiay)".to_owned(), vec![]),
        ),
        ("AttentionIconName", Value::str("")),
        (
            "AttentionIconPixmap",
            Value::Array("(iiay)".to_owned(), vec![]),
        ),
        ("AttentionMovieName", Value::str("")),
        (
            "ToolTip",
            Value::Struct(vec![
                Value::str(""),
                Value::Array("(iiay)".to_owned(), vec![]),
                Value::str("Zellij"),
                Value::str(shared.tooltip.clone()),
            ]),
        ),
        ("ItemIsMenu", Value::Bool(false)),
        ("Menu", Value::Path(MENU_PATH.to_owned())),
    ]
}

fn menu_properties() -> Vec<(&'static str, Value)> {
    vec![
        ("Version", Value::U32(3)),
        ("TextDirection", Value::str("ltr")),
        ("Status", Value::str("normal")),
        ("IconThemePath", Value::Array("s".to_owned(), vec![])),
    ]
}

fn flatten(entries: &[MenuEntry]) -> Vec<(i32, &MenuEntry)> {
    let mut out = vec![];
    for entry in entries {
        match entry {
            MenuEntry::Item { id, .. } | MenuEntry::Separator { id } => out.push((*id, entry)),
            MenuEntry::Submenu { id, items, .. } => {
                out.push((*id, entry));
                out.extend(flatten(items));
            },
        }
    }
    out
}

fn entry_properties(entry: &MenuEntry) -> Value {
    match entry {
        MenuEntry::Item { label, enabled, .. } => Value::dict(vec![
            ("label", Value::str(*label)),
            ("enabled", Value::Bool(*enabled)),
            ("visible", Value::Bool(true)),
        ]),
        MenuEntry::Submenu { label, .. } => Value::dict(vec![
            ("label", Value::str(*label)),
            ("enabled", Value::Bool(true)),
            ("visible", Value::Bool(true)),
            ("children-display", Value::str("submenu")),
        ]),
        MenuEntry::Separator { .. } => Value::dict(vec![
            ("type", Value::str("separator")),
            ("visible", Value::Bool(true)),
        ]),
    }
}

fn node(id: i32, properties: Value, children: Vec<Value>) -> Value {
    Value::Struct(vec![
        Value::I32(id),
        properties,
        Value::Array("v".to_owned(), children),
    ])
}

fn subtree(entry: &MenuEntry) -> Value {
    match entry {
        MenuEntry::Submenu { id, items, .. } => node(
            *id,
            entry_properties(entry),
            items
                .iter()
                .map(|item| Value::variant(subtree(item)))
                .collect(),
        ),
        MenuEntry::Item { id, .. } | MenuEntry::Separator { id } => {
            node(*id, entry_properties(entry), vec![])
        },
    }
}

pub fn menu_layout(entries: &[MenuEntry], parent: i32) -> Value {
    if parent != 0 {
        if let Some((_, entry)) = flatten(entries).into_iter().find(|(id, _)| *id == parent) {
            return subtree(entry);
        }
    }
    node(
        0,
        Value::dict(vec![("children-display", Value::str("submenu"))]),
        entries
            .iter()
            .map(|entry| Value::variant(subtree(entry)))
            .collect(),
    )
}

fn introspection(path: &str) -> String {
    let body = match path {
        ITEM_PATH => {
            "<interface name=\"org.kde.StatusNotifierItem\">\
             <method name=\"Activate\"><arg type=\"i\" direction=\"in\"/><arg type=\"i\" direction=\"in\"/></method>\
             <method name=\"SecondaryActivate\"><arg type=\"i\" direction=\"in\"/><arg type=\"i\" direction=\"in\"/></method>\
             <method name=\"ContextMenu\"><arg type=\"i\" direction=\"in\"/><arg type=\"i\" direction=\"in\"/></method>\
             <method name=\"Scroll\"><arg type=\"i\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/></method>\
             <signal name=\"NewIcon\"/><signal name=\"NewToolTip\"/><signal name=\"NewStatus\"><arg type=\"s\"/></signal>\
             <property name=\"Category\" type=\"s\" access=\"read\"/>\
             <property name=\"Id\" type=\"s\" access=\"read\"/>\
             <property name=\"Title\" type=\"s\" access=\"read\"/>\
             <property name=\"Status\" type=\"s\" access=\"read\"/>\
             <property name=\"IconPixmap\" type=\"a(iiay)\" access=\"read\"/>\
             <property name=\"ToolTip\" type=\"(sa(iiay)ss)\" access=\"read\"/>\
             <property name=\"ItemIsMenu\" type=\"b\" access=\"read\"/>\
             <property name=\"Menu\" type=\"o\" access=\"read\"/>\
             </interface>"
        },
        MENU_PATH => {
            "<interface name=\"com.canonical.dbusmenu\">\
             <method name=\"GetLayout\"><arg type=\"i\" direction=\"in\"/><arg type=\"i\" direction=\"in\"/><arg type=\"as\" direction=\"in\"/><arg type=\"u\" direction=\"out\"/><arg type=\"(ia{sv}av)\" direction=\"out\"/></method>\
             <method name=\"GetGroupProperties\"><arg type=\"ai\" direction=\"in\"/><arg type=\"as\" direction=\"in\"/><arg type=\"a(ia{sv})\" direction=\"out\"/></method>\
             <method name=\"GetProperty\"><arg type=\"i\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"out\"/></method>\
             <method name=\"Event\"><arg type=\"i\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"in\"/><arg type=\"u\" direction=\"in\"/></method>\
             <method name=\"EventGroup\"><arg type=\"a(isvu)\" direction=\"in\"/><arg type=\"ai\" direction=\"out\"/></method>\
             <method name=\"AboutToShow\"><arg type=\"i\" direction=\"in\"/><arg type=\"b\" direction=\"out\"/></method>\
             <method name=\"AboutToShowGroup\"><arg type=\"ai\" direction=\"in\"/><arg type=\"ai\" direction=\"out\"/><arg type=\"ai\" direction=\"out\"/></method>\
             <signal name=\"LayoutUpdated\"><arg type=\"u\"/><arg type=\"i\"/></signal>\
             <property name=\"Version\" type=\"u\" access=\"read\"/>\
             <property name=\"Status\" type=\"s\" access=\"read\"/>\
             </interface>"
        },
        _ => "",
    };
    format!(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
         \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\
         <node><interface name=\"org.freedesktop.DBus.Introspectable\">\
         <method name=\"Introspect\"><arg type=\"s\" direction=\"out\"/></method></interface>\
         <interface name=\"org.freedesktop.DBus.Properties\">\
         <method name=\"Get\"><arg type=\"s\" direction=\"in\"/><arg type=\"s\" direction=\"in\"/><arg type=\"v\" direction=\"out\"/></method>\
         <method name=\"GetAll\"><arg type=\"s\" direction=\"in\"/><arg type=\"a{{sv}}\" direction=\"out\"/></method></interface>\
         {}</node>",
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::{tray_state, Pill, PillColor, TrayLook};
    use crate::app::tray::MenuAction;

    fn shared() -> Mutex<Shared> {
        let mut cache = IconCache::default();
        let state = tray_state(&Default::default(), false);
        Mutex::new(Shared {
            tooltip: "Zellij · 0 running".to_owned(),
            pixmaps: pixmaps(&mut cache, &state),
            paused: false,
            revision: 1,
            anchor: None,
            token: None,
        })
    }

    fn call(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Message {
        let mut message = Message::call(":1.1", path, interface, member, body);
        message.serial = 3;
        message.sender = Some(":1.2".to_owned());
        message
    }

    fn recorded() -> (TrayEvents, Arc<Mutex<Vec<TrayEvent>>>) {
        let seen: Arc<Mutex<Vec<TrayEvent>>> = Arc::default();
        let into = seen.clone();
        (Arc::new(move |e| into.lock().unwrap().push(e)), seen)
    }

    #[test]
    fn the_item_offers_pixmaps_at_every_size_and_a_tooltip() {
        let (events, _) = recorded();
        let body = answer(
            &call(
                ITEM_PATH,
                PROPERTIES,
                "GetAll",
                vec![Value::str(ITEM_INTERFACE)],
            ),
            &shared(),
            &events,
        )
        .unwrap()
        .unwrap();
        let (signature, _) = crate::dbus::encode_body(&body);
        assert_eq!(signature, "a{sv}");
        let properties = body[0].items();
        let pixmaps = properties
            .iter()
            .find_map(|entry| match entry {
                Value::DictEntry(key, value) if key.as_str() == Some("IconPixmap") => Some(value),
                _ => None,
            })
            .unwrap();
        let sizes: Vec<i32> = pixmaps
            .items()
            .iter()
            .map(|p| p.items()[0].as_i32().unwrap())
            .collect();
        assert_eq!(sizes, vec![16, 22, 24, 32, 48]);
        let tooltip = answer(
            &call(
                ITEM_PATH,
                PROPERTIES,
                "Get",
                vec![Value::str(ITEM_INTERFACE), Value::str("ToolTip")],
            ),
            &shared(),
            &events,
        )
        .unwrap()
        .unwrap();
        assert_eq!(tooltip[0].items()[3].as_str(), Some("Zellij · 0 running"));
    }

    #[test]
    fn activate_reports_the_click_position_and_menu_clicks_their_action() {
        let (events, seen) = recorded();
        let shared = shared();
        answer(
            &call(
                ITEM_PATH,
                ITEM_INTERFACE,
                "ProvideXdgActivationToken",
                vec![Value::str("t")],
            ),
            &shared,
            &events,
        );
        answer(
            &call(
                ITEM_PATH,
                ITEM_INTERFACE,
                "Activate",
                vec![Value::I32(100), Value::I32(5)],
            ),
            &shared,
            &events,
        );
        answer(
            &call(
                MENU_PATH,
                MENU_INTERFACE,
                "Event",
                vec![
                    Value::I32(4),
                    Value::str("clicked"),
                    Value::variant(Value::I32(0)),
                    Value::U32(0),
                ],
            ),
            &shared,
            &events,
        );
        answer(
            &call(
                MENU_PATH,
                MENU_INTERFACE,
                "Event",
                vec![
                    Value::I32(4),
                    Value::str("hovered"),
                    Value::variant(Value::I32(0)),
                    Value::U32(0),
                ],
            ),
            &shared,
            &events,
        );
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                TrayEvent::Activate {
                    at: Some((100, 5)),
                    token: Some("t".to_owned())
                },
                TrayEvent::Menu(MenuAction::NewWindow),
            ]
        );
    }

    #[test]
    fn the_menu_layout_nests_submenus_and_has_the_dbusmenu_shape() {
        let layout = menu_layout(&menu(false), 0);
        assert_eq!(layout.signature(), "(ia{sv}av)");
        let children = layout.items()[2].items();
        assert_eq!(children.len(), 10);
        let pause = children[6].items();
        assert_eq!(pause[0], Value::I32(5));
        assert_eq!(pause[2].items().len(), 3);
        let sub = menu_layout(&menu(false), 6);
        assert_eq!(sub.items()[0], Value::I32(6));
        assert_eq!(sub.items()[2].items().len(), 2);
    }

    #[test]
    fn unknown_methods_are_refused() {
        let (events, _) = recorded();
        assert!(answer(
            &call(ITEM_PATH, ITEM_INTERFACE, "Explode", vec![]),
            &shared(),
            &events
        )
        .is_none());
    }

    #[test]
    fn a_new_pill_changes_the_pixmaps() {
        let mut cache = IconCache::default();
        let calm = tray_state(&Default::default(), false);
        let mut attention = calm.clone();
        attention.look = TrayLook {
            pill: Some(Pill {
                color: PillColor::Attention,
                count: 2,
            }),
            dimmed: false,
        };
        assert_ne!(pixmaps(&mut cache, &calm), pixmaps(&mut cache, &attention));
    }

    fn host(
        name_granted: u32,
        watcher: Arc<std::sync::atomic::AtomicBool>,
    ) -> (crate::dbus::fake::FakeBus, StatusNotifier) {
        let (ours, theirs) = std::os::unix::net::UnixStream::pair().unwrap();
        let bus = crate::dbus::fake::FakeBus::serve(theirs, move |call, _| {
            match call.member.as_deref() {
                Some("RequestName") => Some(vec![Value::U32(name_granted)]),
                Some("NameHasOwner") => Some(vec![Value::Bool(
                    watcher.load(std::sync::atomic::Ordering::SeqCst),
                )]),
                Some("RegisterStatusNotifierItem") => Some(vec![]),
                _ => None,
            }
        });
        let (events, _) = recorded();
        let state = tray_state(&Default::default(), false);
        let tray = StatusNotifier::over(ours, events, &state);
        (bus, tray)
    }

    fn members(calls: &[Message]) -> Vec<String> {
        calls
            .iter()
            .filter_map(|call| call.member.clone())
            .collect()
    }

    fn registrations(bus: &crate::dbus::fake::FakeBus) -> Vec<Message> {
        bus.calls()
            .into_iter()
            .filter(|call| call.member.as_deref() == Some("RegisterStatusNotifierItem"))
            .collect()
    }

    #[test]
    fn the_item_claims_its_name_and_registers_with_the_running_watcher() {
        let watcher = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (bus, tray) = host(1, watcher);
        assert!(tray.bus.is_some());
        let calls = bus.calls();
        assert_eq!(
            members(&calls),
            vec![
                "Hello".to_owned(),
                "RequestName".to_owned(),
                "AddMatch".to_owned(),
                "NameHasOwner".to_owned(),
                "RegisterStatusNotifierItem".to_owned()
            ]
        );
        assert_eq!(calls[1].body[0].as_str(), Some(tray.service.as_str()));
        let rule = calls[2].body[0].as_str().unwrap_or_default().to_owned();
        assert!(rule.contains("NameOwnerChanged"), "{}", rule);
        assert!(rule.contains(WATCHER_SERVICE), "{}", rule);
        assert_eq!(calls[3].body[0].as_str(), Some(WATCHER_SERVICE));
        assert_eq!(calls[4].destination.as_deref(), Some(WATCHER_SERVICE));
        assert_eq!(calls[4].path.as_deref(), Some(WATCHER_PATH));
        assert_eq!(calls[4].body[0].as_str(), Some(tray.service.as_str()));
    }

    #[test]
    fn without_a_tray_host_the_item_waits_and_registers_when_one_appears() {
        let watcher = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (bus, tray) = host(1, watcher.clone());
        assert!(tray.bus.is_some(), "the item stays on the bus for a host");
        assert!(registrations(&bus).is_empty());
        watcher.store(true, std::sync::atomic::Ordering::SeqCst);
        bus.signal(Message::signal(
            crate::dbus::BUS_PATH,
            BUS_SERVICE,
            "NameOwnerChanged",
            vec![
                Value::str(WATCHER_SERVICE),
                Value::str(""),
                Value::str(":1.9"),
            ],
        ));
        let registered = registrations(&bus);
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].body[0].as_str(), Some(tray.service.as_str()));
    }

    #[test]
    fn a_watcher_that_goes_away_is_not_registered_with_and_a_restart_registers_again() {
        let watcher = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (bus, _tray) = host(1, watcher);
        assert_eq!(registrations(&bus).len(), 1);
        let owner_changed = |owner: &str| {
            Message::signal(
                crate::dbus::BUS_PATH,
                BUS_SERVICE,
                "NameOwnerChanged",
                vec![
                    Value::str(WATCHER_SERVICE),
                    Value::str(":1.9"),
                    Value::str(owner),
                ],
            )
        };
        bus.signal(owner_changed(""));
        assert_eq!(registrations(&bus).len(), 1);
        bus.signal(owner_changed(":1.10"));
        assert_eq!(registrations(&bus).len(), 2);
    }

    #[test]
    fn a_taken_bus_name_leaves_the_icon_hidden() {
        let watcher = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let (bus, tray) = host(3, watcher);
        assert!(tray.bus.is_none());
        assert!(registrations(&bus).is_empty());
    }
}
