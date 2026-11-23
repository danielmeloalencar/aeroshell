//! Cliente de teste F1-csd-gtk: toplevel que pede CSD e usa xdg_toplevel.move/resize.
//!
//! Usa só `wayland-client` (dev-dependency existente) e bindings reexportados
//! pelo Smithay 0.7.0 fixado. Serve ao harness `tests/integration/f1_csd_gtk.py`.

use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    os::fd::AsFd,
    time::Duration,
};

use smithay::reexports::wayland_protocols::xdg::{
    decoration::zv1::client::{
        zxdg_decoration_manager_v1::ZxdgDecorationManagerV1,
        zxdg_toplevel_decoration_v1::{Mode as DecorationMode, ZxdgToplevelDecorationV1},
    },
    shell::client::{
        xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
        xdg_toplevel::{Event as ToplevelEvent, ResizeEdge, XdgToplevel},
        xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
    },
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_pointer::{ButtonState, Event as PointerEvent, WlPointer},
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

const WIDTH: i32 = 400;
const HEIGHT: i32 = 300;

impl Dispatch<XdgToplevel, u32> for App {
    fn event(
        state: &mut App,
        _toplevel: &XdgToplevel,
        event: ToplevelEvent,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let ToplevelEvent::Configure { width, height, .. } = event {
            if width > 0 && height > 0 {
                state.pending_size = Some((width, height));
            }
        }
    }
}

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    shm: Option<WlShm>,
    seat: Option<WlSeat>,
    decoration_manager: Option<ZxdgDecorationManagerV1>,
    surface: Option<WlSurface>,
    toplevel: Option<XdgToplevel>,
    serials: Vec<u32>,
    pending_size: Option<(i32, i32)>,
    buffer_size: (i32, i32),
    phase: u8,
    done: bool,
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
            eprintln!("CSD_CLIENT global {interface} v{version}");
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "zxdg_decoration_manager_v1" => {
                    state.decoration_manager = Some(registry.bind(name, version.min(1), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<XdgWmBase, ()> for App {
    fn event(
        _: &mut App,
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

impl Dispatch<XdgSurface, u32> for App {
    fn event(
        _: &mut App,
        surface: &XdgSurface,
        event: XdgSurfaceEvent,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let XdgSurfaceEvent::Configure { serial } = event {
            surface.ack_configure(serial);
        }
    }
}

impl Dispatch<WlPointer, ()> for App {
    fn event(
        state: &mut App,
        _: &WlPointer,
        event: PointerEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let PointerEvent::Button {
            serial,
            state: wayland_client::WEnum::Value(ButtonState::Pressed),
            ..
        } = event
        {
            state.serials.push(serial);
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore ZxdgDecorationManagerV1);
delegate_noop!(App: ignore ZxdgToplevelDecorationV1);

impl App {
    fn attach_buffer(
        &mut self,
        qh: &QueueHandle<App>,
        size: (i32, i32),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let shm = self.shm.clone().ok_or("sem wl_shm")?;
        let (width, height) = size;
        let stride = width * 4;
        let path = std::env::temp_dir().join(format!(
            "aero-csd-shm-{}-{}x{}",
            std::process::id(),
            width,
            height
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len((stride * height) as u64)?;
        let image = vec![40u8; (stride * height) as usize];
        file.write_all(&image)?;
        file.seek(SeekFrom::Start(0))?;
        let pool = shm.create_pool(file.as_fd(), stride * height, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, Format::Argb8888, qh, ());
        let surface = self.surface.clone().ok_or("sem surface")?;
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.buffer_size = size;
        Ok(())
    }

    fn drive_requests(&mut self) {
        let (Some(toplevel), Some(seat)) = (self.toplevel.clone(), self.seat.clone()) else {
            return;
        };
        if self.phase == 0 && !self.serials.is_empty() {
            let serial = self.serials.pop().unwrap_or(0);
            self.serials.clear();
            toplevel._move(&seat, serial);
            self.phase = 1;
            eprintln!("CSD_CLIENT move_requested serial={serial}");
        } else if self.phase == 1 && !self.serials.is_empty() {
            let Some(go) = std::env::var_os("AERO_CSD_GO") else {
                return;
            };
            if !std::path::Path::new(&go).exists() {
                return;
            }
            let serial = self.serials.pop().unwrap_or(0);
            self.serials.clear();
            toplevel.resize(&seat, serial, ResizeEdge::BottomRight);
            self.phase = 2;
            self.done = true;
            eprintln!("CSD_CLIENT resize_requested serial={serial}");
        }
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
        shm: None,
        seat: None,
        decoration_manager: None,
        surface: None,
        toplevel: None,
        serials: Vec::new(),
        pending_size: None,
        buffer_size: (WIDTH, HEIGHT),
        phase: 0,
        done: false,
    };
    queue.roundtrip(&mut app)?;

    let manager = app
        .decoration_manager
        .clone()
        .ok_or("sem zxdg_decoration_manager_v1")?;
    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let _pointer = seat.get_pointer(&qh, ());

    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg_surface.get_toplevel(&qh, 0u32);
    toplevel.set_title("aero-csd-probe".into());
    let decoration = manager.get_toplevel_decoration(&toplevel, &qh, ());
    decoration.set_mode(DecorationMode::ClientSide);
    app.surface = Some(surface);
    app.toplevel = Some(toplevel.clone());
    app.attach_buffer(&qh, (WIDTH, HEIGHT))?;
    // Pedido com serial inválido: o compositor deve recusar sem criar grab.
    toplevel._move(&seat, 0);
    eprintln!("CSD_CLIENT invalid_move_requested serial=0");
    eprintln!("CSD_CLIENT mode_requested=ClientSide");
    eprintln!("CSD_CLIENT pronto_para_input");

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        if let Some(size) = app.pending_size.take() {
            if size != app.buffer_size {
                app.attach_buffer(&qh, size)?;
                eprintln!("CSD_CLIENT recommit {}x{}", size.0, size.1);
            }
        }
        app.drive_requests();
        if app.done {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !app.done {
        return Err("move/resize não foram pedidos".into());
    }
    eprintln!("CSD_CLIENT OK");
    for _ in 0..40 {
        queue.roundtrip(&mut app)?;
        if let Some(size) = app.pending_size.take() {
            if size != app.buffer_size {
                app.attach_buffer(&qh, size)?;
                eprintln!("CSD_CLIENT recommit {}x{}", size.0, size.1);
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
