//! Cliente mínimo de teste (F1-popups-render): cria um toplevel xdg e um xdg_popup real.
//!
//! Não é produto: serve de cliente de teste para o harness `tests/integration/popup_client.py`.
//! Usa apenas `wayland-client` (dev-dependency existente) e os bindings gerados reexportados pelo
//! Smithay 0.7.0 fixado — nenhuma dependência nova.

use std::time::Duration;

use smithay::reexports::wayland_protocols::xdg::shell::client::{
    xdg_popup::XdgPopup,
    xdg_positioner::{Anchor, Gravity, XdgPositioner},
    xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_compositor::WlCompositor,
        wl_pointer::{Event as PointerEvent, WlPointer},
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    parent: Option<XdgSurface>,
    popup: Option<XdgPopup>,
    seat: Option<WlSeat>,
    pointer_serial: Option<u32>,
    grab_requested: bool,
    popup_created: bool,
    popup_configured: bool,
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        state: &mut App,
        registry: &WlRegistry,
        event: RegistryEvent,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        if let RegistryEvent::Global {
            name,
            interface,
            version,
        } = event
        {
            eprintln!("POPUP_CLIENT global {interface} v{version}");
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<XdgWmBase, ()> for App {
    fn event(
        _state: &mut App,
        wm_base: &XdgWmBase,
        event: WmBaseEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let WmBaseEvent::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<WlPointer, ()> for App {
    fn event(
        state: &mut App,
        _pointer: &WlPointer,
        event: PointerEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let PointerEvent::Button { serial, .. } = event {
            state.pointer_serial = Some(serial);
        }
    }
}

/// `udata == 0`: xdg_surface do toplevel; `udata == 1`: xdg_surface do popup.
impl Dispatch<XdgSurface, u32> for App {
    fn event(
        state: &mut App,
        surface: &XdgSurface,
        event: XdgSurfaceEvent,
        kind: &u32,
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        let XdgSurfaceEvent::Configure { serial } = event else {
            return;
        };
        surface.ack_configure(serial);
        match kind {
            0 if !state.popup_created => state.create_popup(qh),
            1 if !state.popup_configured => {
                state.popup_configured = true;
                eprintln!("POPUP_CLIENT popup_configured");
            }
            _ => {}
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore XdgPositioner);
delegate_noop!(App: ignore XdgToplevel);
delegate_noop!(App: ignore smithay::reexports::wayland_protocols::xdg::shell::client::xdg_popup::XdgPopup);

impl App {
    fn create_popup(&mut self, qh: &QueueHandle<App>) {
        let Some(compositor) = self.compositor.clone() else {
            return;
        };
        let Some(wm_base) = self.wm_base.clone() else {
            return;
        };
        let Some(parent) = self.parent.clone() else {
            return;
        };
        self.popup_created = true;

        let popup_surface = compositor.create_surface(qh, ());
        let popup_xdg = wm_base.get_xdg_surface(&popup_surface, qh, 1);
        let positioner = wm_base.create_positioner(qh, ());
        positioner.set_size(120, 40);
        positioner.set_anchor_rect(0, 0, 200, 30);
        positioner.set_anchor(Anchor::BottomRight);
        positioner.set_gravity(Gravity::BottomRight);
        let popup = popup_xdg.get_popup(Some(&parent), &positioner, qh, ());
        // xdg_popup.grab deve vir ANTES do commit que mapeia a superfície,
        // senão o Smithay responde `invalid_grab: xdg_popup already is mapped`.
        let serial = self.pointer_serial.unwrap_or(0);
        if let Some(seat) = self.seat.clone() {
            popup.grab(&seat, serial);
            self.grab_requested = true;
            eprintln!("POPUP_CLIENT grab_requested serial={serial}");
        }
        self.popup = Some(popup);
        popup_surface.commit();
        eprintln!("POPUP_CLIENT popup_created");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let display = connection.display();
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        wm_base: None,
        parent: None,
        popup: None,
        seat: None,
        pointer_serial: None,
        grab_requested: false,
        popup_created: false,
        popup_configured: false,
    };
    queue.roundtrip(&mut app)?;

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let _pointer = seat.get_pointer(&qh, ());
    let surface = compositor.create_surface(&qh, ());
    let toplevel_xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = toplevel_xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-popup-client".into());
    app.parent = Some(toplevel_xdg.clone());
    surface.commit();
    eprintln!("POPUP_CLIENT toplevel_committed");

    for _ in 0..60 {
        queue.roundtrip(&mut app)?;
        if app.popup_configured {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    if !app.popup_configured {
        return Err("popup nunca configurado".into());
    }

    for _ in 0..10 {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    if !app.grab_requested {
        return Err("nenhum button event para pedir grab".into());
    }
    if let Some(popup) = app.popup.take() {
        popup.destroy();
        eprintln!("POPUP_CLIENT dismissed");
    }
    queue.roundtrip(&mut app)?;
    eprintln!("POPUP_CLIENT OK");
    Ok(())
}
