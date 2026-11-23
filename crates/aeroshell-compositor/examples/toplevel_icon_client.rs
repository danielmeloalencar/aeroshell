//! Cliente de teste F1-toplevel-icon: cria um toplevel e anexa um ícone 16×16 vermelho.
//!
//! Usa só `wayland-client` (dev-dependency existente) e bindings reexportados pelo
//! Smithay 0.7.0 fixado. Serve ao harness `tests/integration/f1_toplevel_icon.py`.

use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    os::fd::AsFd,
    time::Duration,
};

use smithay::reexports::wayland_protocols::{
    wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    xdg::{
        shell::client::{
            xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
            xdg_toplevel::XdgToplevel,
            xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
        },
        toplevel_icon::v1::client::{
            xdg_toplevel_icon_manager_v1::XdgToplevelIconManagerV1,
            xdg_toplevel_icon_v1::XdgToplevelIconV1,
        },
    },
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

const ICON_IFACE: &str = "xdg_toplevel_icon_manager_v1";
const SIZE: i32 = 16;

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    shm: Option<WlShm>,
    manager: Option<XdgToplevelIconManagerV1>,
    globals: Vec<String>,
    icon: Option<XdgToplevelIconV1>,
    buffer: Option<WlBuffer>,
    toplevel_handle: Option<XdgToplevel>,
    surface_handle: Option<WlSurface>,
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
            state.globals.push(interface.clone());
            eprintln!("ICON_CLIENT global {interface} v{version}");
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
                ICON_IFACE => state.manager = Some(registry.bind(name, version.min(1), qh, ())),
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
        state: &mut App,
        surface: &XdgSurface,
        event: XdgSurfaceEvent,
        _: &u32,
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        if let XdgSurfaceEvent::Configure { serial } = event {
            surface.ack_configure(serial);
            if state.icon.is_none() {
                state.attach_icon(qh);
            }
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore XdgToplevel);
delegate_noop!(App: ignore XdgToplevelIconManagerV1);
delegate_noop!(App: ignore XdgToplevelIconV1);
delegate_noop!(App: ignore ZwpTextInputManagerV3);

impl App {
    fn attach_icon(&mut self, qh: &QueueHandle<App>) {
        let (Some(shm), Some(manager)) = (self.shm.clone(), self.manager.clone()) else {
            return;
        };
        let stride = SIZE * 4;
        let path = std::env::temp_dir().join(format!("aero-icon-shm-{}", std::process::id()));
        let Ok(mut file) = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
        else {
            return;
        };
        if file.set_len((stride * SIZE) as u64).is_err() {
            return;
        }
        // ARGB8888 little-endian: bytes BGRA. 0xFFFF0000 => 00 00 FF FF.
        let pixel = [0u8, 0, 255, 255];
        let image: Vec<u8> = pixel.repeat(SIZE as usize * SIZE as usize);
        if file.write_all(&image).is_err() || file.seek(SeekFrom::Start(0)).is_err() {
            return;
        }
        let pool = shm.create_pool(file.as_fd(), stride * SIZE, qh, ());
        let buffer = pool.create_buffer(0, SIZE, SIZE, stride, Format::Argb8888, qh, ());
        let icon = manager.create_icon(qh, ());
        // ARGB8888 little-endian: bytes BGRA; 0xFFFF0000 => 00 00 FF FF (vermelho).
        icon.add_buffer(&buffer, 1);
        manager.set_icon(&self.toplevel().expect("toplevel"), Some(&icon));
        self.icon = Some(icon);
        self.buffer = Some(buffer);
        self.done = true;
        // O cached state do ícone só se torna `current` no commit da superfície.
        self.surface().expect("surface").commit();
        eprintln!("ICON_CLIENT icon_set size={SIZE}x{SIZE} format=argb8888 color=red");
    }

    fn toplevel(&self) -> Option<XdgToplevel> {
        self.toplevel_handle.clone()
    }

    fn surface(&self) -> Option<WlSurface> {
        self.surface_handle.clone()
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
        manager: None,
        globals: Vec::new(),
        icon: None,
        buffer: None,
        toplevel_handle: None,
        surface_handle: None,
        done: false,
    };
    queue.roundtrip(&mut app)?;

    let present = app.globals.iter().any(|g| g == ICON_IFACE);
    eprintln!(
        "ICON_CLIENT CHECK {ICON_IFACE} {}",
        if present { "presente" } else { "AUSENTE" }
    );
    if !present {
        return Err("xdg_toplevel_icon_v1 ausente".into());
    }

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("aero-icon-probe".into());
    app.toplevel_handle = Some(toplevel);
    app.surface_handle = Some(surface.clone());
    surface.commit();

    for _ in 0..80 {
        queue.roundtrip(&mut app)?;
        if app.done {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !app.done {
        return Err("ícone nunca foi anexado".into());
    }
    for _ in 0..4 {
        queue.roundtrip(&mut app)?;
    }
    eprintln!("ICON_CLIENT OK");

    // F1-toplevel-icon-protocol: reproduz a sequência do Qt (kwrite) que
    // destrói o ícone e o wl_buffer DEPOIS do set_icon. Sob env de teste.
    if std::env::var_os("AERO_ICON_DESTROY_AFTER_SET").is_some() {
        if let (Some(icon), Some(buffer)) = (app.icon.take(), app.buffer.take()) {
            icon.destroy();
            buffer.destroy();
            for _ in 0..4 {
                queue.roundtrip(&mut app)?;
            }
            eprintln!("ICON_CLIENT DESTROY_AFTER_SET_OK");
        }
        return Ok(());
    }
    // Mantém a janela viva para o harness consultar `icon 0` no IPC.
    for tick in 0..120 {
        queue.roundtrip(&mut app)?;
        if tick % 40 == 0 {
            eprintln!("ICON_CLIENT alive {tick}");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
