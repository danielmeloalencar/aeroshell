//! Diagnóstico F1-final-1c: toplevel com buffer do tamanho da SAÍDA (como Kate/Kwrite).
//! Cliente de teste, não produto. Sem dep nova; wayland-client já é dev-dep.

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;
use std::time::Duration;

use smithay::reexports::wayland_protocols::xdg::shell::client::{
    xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

struct App {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    wm_base: Option<XdgWmBase>,
    configured: bool,
    buffer: Option<WlBuffer>,
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
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
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
            if !state.configured {
                state.configured = true;
                eprintln!("LARGE_CLIENT configured serial={serial}");
            }
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
    let (w, h): (i32, i32) = (
        std::env::var("AERO_LARGE_W")
            .unwrap_or_else(|_| "1280".into())
            .parse()?,
        std::env::var("AERO_LARGE_H")
            .unwrap_or_else(|_| "800".into())
            .parse()?,
    );
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        shm: None,
        wm_base: None,
        configured: false,
        buffer: None,
    };
    queue.roundtrip(&mut app)?;
    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let shm = app.shm.clone().ok_or("sem wl_shm")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-large-client".into());
    let stride = w * 4;
    let path = std::env::temp_dir().join(format!("aero-large-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len((stride * h) as u64)?;
    file.write_all(&vec![60u8; (stride * h) as usize])?;
    file.seek(SeekFrom::Start(0))?;
    let pool = shm.create_pool(file.as_fd(), stride * h, &qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, &qh, ());
    app.buffer = Some(buffer.clone());
    let no_commit = std::env::var_os("AERO_LARGE_NO_COMMIT").is_some();
    let delay: u64 = std::env::var("AERO_LARGE_DELAY_MS")
        .unwrap_or_else(|_| "0".into())
        .parse()?;
    let start = std::time::Instant::now();
    let mut committed = false;
    if delay == 0 && !no_commit {
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        committed = true;
        eprintln!("LARGE_CLIENT committed {w}x{h}");
    }

    let deadline = start + Duration::from_secs(12);
    while std::time::Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        if !committed && !no_commit && start.elapsed() >= Duration::from_millis(delay) {
            surface.attach(Some(&buffer), 0, 0);
            surface.commit();
            committed = true;
            eprintln!(
                "LARGE_CLIENT committed_late {w}x{h} t={:?}",
                start.elapsed()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("LARGE_CLIENT OK");
    Ok(())
}
