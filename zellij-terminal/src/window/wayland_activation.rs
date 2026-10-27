use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use wayland_client::backend::{Backend, ObjectId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_keyboard::{self, WlKeyboard};
use wayland_client::protocol::wl_pointer::{self, WlPointer};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_token_v1::{
    self, XdgActivationTokenV1,
};
use wayland_protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::Window;

const TOKEN_WAIT: Duration = Duration::from_millis(500);

#[derive(Default)]
struct State {
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    last_serial: Option<u32>,
    token: Option<String>,
}

pub struct Activator {
    connection: Connection,
    queue: EventQueue<State>,
    state: State,
    activation: XdgActivationV1,
    seat: Option<WlSeat>,
}

fn surface_of(connection: &Connection, window: &Window) -> Option<WlSurface> {
    let handle = window.window_handle().ok()?.as_raw();
    let RawWindowHandle::Wayland(handle) = handle else {
        return None;
    };
    let id = unsafe { ObjectId::from_ptr(WlSurface::interface(), handle.surface.as_ptr().cast()) }
        .ok()?;
    WlSurface::from_id(connection, id).ok()
}

impl Activator {
    pub fn for_window(window: &Window) -> Result<Activator> {
        let handle = window
            .display_handle()
            .map_err(|e| anyhow!("the window has no display handle: {}", e))?
            .as_raw();
        let RawDisplayHandle::Wayland(handle) = handle else {
            return Err(anyhow!("the window is not on Wayland"));
        };
        let backend = unsafe { Backend::from_foreign_display(handle.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&connection)
            .map_err(|e| anyhow!("the Wayland globals could not be listed: {}", e))?;
        let qh = queue.handle();
        let activation: XdgActivationV1 = globals
            .bind(&qh, 1..=1, ())
            .map_err(|e| anyhow!("the compositor offers no xdg_activation_v1: {}", e))?;
        let seat: Option<WlSeat> = globals.bind(&qh, 1..=5, ()).ok();
        let mut activator = Activator {
            connection,
            queue,
            state: State::default(),
            activation,
            seat,
        };
        let _ = activator.queue.roundtrip(&mut activator.state);
        Ok(activator)
    }

    pub fn pump(&mut self) {
        let _ = self.queue.dispatch_pending(&mut self.state);
    }

    fn token_from(&mut self, requester: &Window) -> Option<String> {
        self.pump();
        let surface = surface_of(&self.connection, requester)?;
        self.state.token = None;
        let token = self
            .activation
            .get_activation_token(&self.queue.handle(), ());
        if let (Some(serial), Some(seat)) = (self.state.last_serial, self.seat.as_ref()) {
            token.set_serial(serial, seat);
        }
        token.set_surface(&surface);
        token.set_app_id(crate::window::application_id().to_owned());
        token.commit();
        let deadline = Instant::now() + TOKEN_WAIT;
        while self.state.token.is_none() && Instant::now() < deadline {
            if self.queue.roundtrip(&mut self.state).is_err() {
                break;
            }
        }
        token.destroy();
        self.state.token.take()
    }

    pub fn token(&mut self, requester: &Window) -> Option<String> {
        self.token_from(requester)
    }

    pub fn activate(&mut self, target: &Window, token: Option<String>, requester: Option<&Window>) {
        let token = match token {
            Some(token) => Some(token),
            None => requester.and_then(|requester| self.token_from(requester)),
        };
        let Some(token) = token else {
            report!("no activation token was available, so the compositor decides whether to raise the window");
            target.request_user_attention(Some(winit::window::UserAttentionType::Critical));
            return;
        };
        let Some(surface) = surface_of(&self.connection, target) else {
            return;
        };
        self.activation.activate(token, &surface);
        let _ = self.connection.flush();
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        {
            if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
            if capabilities.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_pointer::Event::Button {
            serial,
            state: WEnum::Value(wl_pointer::ButtonState::Pressed),
            ..
        } = event
        {
            state.last_serial = Some(serial);
        }
    }
}

impl Dispatch<WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Key {
            serial,
            state: WEnum::Value(wl_keyboard::KeyState::Pressed),
            ..
        } = event
        {
            state.last_serial = Some(serial);
        }
    }
}

impl Dispatch<XdgActivationV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &XdgActivationV1,
        _: <XdgActivationV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgActivationTokenV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &XdgActivationTokenV1,
        event: xdg_activation_token_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_activation_token_v1::Event::Done { token } = event {
            state.token = Some(token);
        }
    }
}
