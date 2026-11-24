//! F2-decoration-policy: cliente sintético com app_id configurável.
//! Uso: policy_client <app_id>  (padrão: aero-no-deco)

use std::{
    fs::OpenOptions,
    io::Write,
    os::fd::AsFd,
    time::{Duration, Instant},
};

use smithay::reexports::wayland_protocols::xdg::shell::client::{
    xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_registry::Event as RegistryEvent,
        wl_registry::WlRegistry, wl_seat::WlSeat, wl_shm::Format, wl_shm::WlShm,
        wl_shm_pool::WlShmPool, wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    shm: Option<WlShm>,
    ready: bool,
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
        let RegistryEvent::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(6), qh, ())),
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
            _ => {}
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
        state: &mut App,
        xdg: &XdgSurface,
        event: XdgSurfaceEvent,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let XdgSurfaceEvent::Configure { serial } = event {
            xdg.ack_configure(serial);
            state.ready = true;
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore XdgToplevel);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app_id = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "aero-no-deco".to_owned());
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        wm_base: None,
        shm: None,
        ready: false,
    };
    queue.roundtrip(&mut app)?;
    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let shm = app.shm.clone().ok_or("sem wl_shm")?;
    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_app_id(app_id.clone());
    toplevel.set_title(format!("policy-{app_id}"));
    let (w, h) = (240, 160);
    let stride = w * 4;
    let path = std::env::temp_dir().join(format!("aero-policy-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len((stride * h) as u64)?;
    file.write_all(&vec![70u8; (stride * h) as usize])?;
    let pool = shm.create_pool(file.as_fd(), stride * h, &qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    queue.roundtrip(&mut app)?;
    eprintln!("POLICY_CLIENT READY app_id={app_id}");
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(40));
    }
    let _ = app.ready;
    eprintln!("POLICY_CLIENT OK");
    Ok(())
}
