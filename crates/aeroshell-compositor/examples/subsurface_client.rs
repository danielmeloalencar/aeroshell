//! Cliente mínimo de teste (F1-subsurfaces): toplevel + duas wl_subsurface reais.
//!
//! Não é produto: serve de cliente de teste para o harness `tests/integration/f1_subsurfaces.py`.
//! Usa apenas `wayland-client` (dev-dependency existente) e bindings reexportados pelo
//! Smithay 0.7.0 fixado — nenhuma dependência nova.
//!
//! Cobre os dois modos do protocolo:
//!   - `sync` (padrão): o commit do filho fica pendente até o commit do pai;
//!   - `desync`: o commit do filho entra imediatamente.
//!
//! Um `wl_pointer` recebe motion/button sintéticos injetados pelo harness para provar
//! que o hit-test (`surface_under(..., WindowSurfaceType::ALL)`) entrega eventos ao filho.

use std::fs::{File, OpenOptions};
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
        wl_pointer::{Event as PointerEvent, WlPointer},
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_subcompositor::WlSubcompositor,
        wl_subsurface::WlSubsurface,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

const PARENT: &str = "parent";
const SYNC: &str = "synced";
const DESYNC: &str = "desynced";

/// Cores em bytes BGRA (Format::Argb8888 em memória little-endian).
fn color_bytes(r: u8, g: u8, b: u8) -> [u8; 4] {
    [b, g, r, 255]
}

#[derive(Default, Debug)]
struct Counters {
    enter: u32,
    button: u32,
}

struct App {
    compositor: Option<WlCompositor>,
    subcompositor: Option<WlSubcompositor>,
    shm: Option<WlShm>,
    wm_base: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    parent: Option<WlSurface>,
    /// (superfície, subsurface, rótulo) para identificar eventos de ponteiro.
    children: Vec<(WlSurface, WlSubsurface, &'static str)>,
    buffers: Vec<WlBuffer>,
    children_created: bool,
    counters: [Counters; 3],
    /// `wl_pointer.button` não carrega a superfície: o alvo vem do último `enter`.
    last_entered: Option<WlSurface>,
}

impl App {
    fn label_for(&self, surface: &WlSurface) -> Option<&'static str> {
        if self.parent.as_ref() == Some(surface) {
            return Some(PARENT);
        }
        self.children
            .iter()
            .find(|(child, _, _)| child == surface)
            .map(|(_, _, label)| *label)
    }

    fn index_for(label: &str) -> usize {
        match label {
            PARENT => 0,
            SYNC => 1,
            DESYNC => 2,
            _ => usize::MAX,
        }
    }

    fn solid_buffer(
        &mut self,
        qh: &QueueHandle<App>,
        size: (i32, i32),
        pixel: [u8; 4],
    ) -> Result<WlBuffer, Box<dyn std::error::Error>> {
        let shm = self.shm.clone().ok_or("sem wl_shm")?;
        let (width, height) = size;
        let stride = width * 4;
        let path = std::env::temp_dir().join(format!(
            "aero-subsurface-shm-{}-{}x{}-{:02x}{:02x}{:02x}",
            std::process::id(),
            width,
            height,
            pixel[2],
            pixel[1],
            pixel[0]
        ));
        let mut file: File = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len((stride * height) as u64)?;
        let image: Vec<u8> = pixel.repeat((width * height) as usize);
        file.write_all(&image)?;
        file.seek(SeekFrom::Start(0))?;
        let pool = shm.create_pool(file.as_fd(), stride * height, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, Format::Argb8888, qh, ());
        self.buffers.push(buffer.clone());
        Ok(buffer)
    }

    /// Cria os dois filhos e comita na ordem exigida pelo protocolo:
    /// filhos primeiro (o sync fica pendente), pai por último (aplica o sync).
    fn create_children(&mut self, qh: &QueueHandle<App>) -> Result<(), Box<dyn std::error::Error>> {
        let compositor = self.compositor.clone().ok_or("sem wl_compositor")?;
        let subcompositor = self.subcompositor.clone().ok_or("sem wl_subcompositor")?;
        let parent = self.parent.clone().ok_or("sem superfície pai")?;

        let specs = [
            (SYNC, (20, 20), color_bytes(220, 30, 30)),
            (DESYNC, (220, 20), color_bytes(30, 190, 40)),
        ];
        for (label, pos, pixel) in specs {
            let child = compositor.create_surface(qh, ());
            let subsurface = subcompositor.get_subsurface(&child, &parent, qh, ());
            subsurface.set_position(pos.0, pos.1);
            if label == DESYNC {
                subsurface.set_desync();
            }
            let buffer = self.solid_buffer(qh, (140, 90), pixel)?;
            child.attach(Some(&buffer), 0, 0);
            child.commit();
            self.children.push((child, subsurface, label));
            eprintln!(
                "SUBSURFACE_CLIENT child_created label={label} pos=({},{}) mode={}",
                pos.0,
                pos.1,
                if label == DESYNC { "desync" } else { "sync" }
            );
        }
        // Pai: azul, 400x300. O commit aplica o filho synced.
        let blue = self.solid_buffer(qh, (400, 300), color_bytes(30, 60, 220))?;
        parent.attach(Some(&blue), 0, 0);
        parent.commit();
        self.children_created = true;
        eprintln!("SUBSURFACE_CLIENT parent_committed");
        Ok(())
    }
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
        eprintln!("SUBSURFACE_CLIENT global {interface} v{version}");
        match interface.as_str() {
            "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(6), qh, ())),
            "wl_subcompositor" => {
                state.subcompositor = Some(registry.bind(name, version.min(1), qh, ()))
            }
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
            "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            _ => {}
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

impl Dispatch<XdgSurface, u32> for App {
    fn event(
        state: &mut App,
        xdg: &XdgSurface,
        event: XdgSurfaceEvent,
        _: &u32,
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        let XdgSurfaceEvent::Configure { serial } = event else {
            return;
        };
        xdg.ack_configure(serial);
        if !state.children_created {
            if let Err(err) = state.create_children(qh) {
                eprintln!("SUBSURFACE_CLIENT ERRO criar filhos: {err}");
            }
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
        let (surface, kind, serial) = match event {
            PointerEvent::Enter { surface, .. } => {
                state.last_entered = Some(surface.clone());
                (surface, "enter", None)
            }
            PointerEvent::Button { serial, .. } => {
                let Some(surface) = state.last_entered.clone() else {
                    return;
                };
                (surface, "button", Some(serial))
            }
            _ => return,
        };
        let Some(label) = state.label_for(&surface) else {
            return;
        };
        let idx = Self::index_for(label);
        if idx >= state.counters.len() {
            return;
        }
        match kind {
            "enter" => state.counters[idx].enter += 1,
            _ => state.counters[idx].button += 1,
        }
        eprintln!(
            "SUBSURFACE_CLIENT pointer kind={kind} label={label}{}",
            serial.map(|s| format!(" serial={s}")).unwrap_or_default()
        );
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSubcompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlSubsurface);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore XdgToplevel);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let display = connection.display();
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        subcompositor: None,
        shm: None,
        wm_base: None,
        seat: None,
        parent: None,
        children: Vec::new(),
        buffers: Vec::new(),
        children_created: false,
        counters: std::array::from_fn(|_| Counters::default()),
        last_entered: None,
    };
    queue.roundtrip(&mut app)?;

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let _pointer = seat.get_pointer(&qh, ());

    let parent = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&parent, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-subsurface-client".into());
    app.parent = Some(parent.clone());
    parent.commit();
    eprintln!("SUBSURFACE_CLIENT toplevel_committed");

    // Janela em (60,80): filhos em (80,100) e (280,100) nas coordenadas do output.
    // Fica vivo por 8 s depois de criar os filhos: o harness amostra o dump
    // full-res WHILE a janela existe (o arquivo é sobrescrito a cada frame).
    let deadline = std::time::Instant::now() + Duration::from_secs(9);
    while std::time::Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!(
        "SUBSURFACE_CLIENT contadores parent={:?} sync={:?} desync={:?}",
        app.counters[0], app.counters[1], app.counters[2]
    );
    if !app.children_created {
        return Err("filhos nunca criados".into());
    }
    for (_, subsurface, _) in app.children.drain(..) {
        subsurface.destroy();
    }
    queue.roundtrip(&mut app)?;
    eprintln!("SUBSURFACE_CLIENT OK");
    Ok(())
}
