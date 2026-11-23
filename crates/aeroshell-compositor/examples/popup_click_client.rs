//! Diagnóstico (F1-subsurfaces, juiz visual negou): um xdg_popup com grab real
//! recebe clique sintético? Instrumento de teste, não produto.
//!
//! Cria toplevel 200x150, um xdg_popup 120x40 ancorado no canto inferior direito
//! do rect (0,0,200,30) — na prática em (260,110) — pede grab ANTES do commit e
//! conta enter/button por superfície (pop-up vs. pai).

use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsFd;
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
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_pointer::{Event as PointerEvent, WlPointer},
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

#[derive(Default, Debug)]
struct Counts {
    enter: u32,
    button: u32,
}

struct App {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    wm_base: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    parent: Option<WlSurface>,
    /// xdg_surface do pai, necessário para `get_popup`.
    parent_xdg: Option<XdgSurface>,
    popup_surface: Option<WlSurface>,
    popup: Option<XdgPopup>,
    popup_created: bool,
    popup_configured: bool,
    /// `wl_pointer.button` não carrega a superfície: vem do último `enter`.
    entered_popup: Option<bool>,
    counts: [Counts; 2],
    buffers: Vec<WlBuffer>,
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
            "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
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
        kind: &u32,
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        let XdgSurfaceEvent::Configure { serial } = event else {
            return;
        };
        xdg.ack_configure(serial);
        match kind {
            0 if !state.popup_created => state.create_popup(qh),
            1 if !state.popup_configured => {
                state.popup_configured = true;
                eprintln!("POPUP_CLICK popup_configured");
            }
            _ => {}
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
        let is_popup = match event {
            PointerEvent::Enter { surface, .. } => {
                let p = state.popup_surface.as_ref() == Some(&surface);
                state.entered_popup = Some(p);
                state.counts[usize::from(p)].enter += 1;
                eprintln!(
                    "POPUP_CLICK enter label={}",
                    if p { "popup" } else { "parent" }
                );
                return;
            }
            PointerEvent::Button { serial, .. } => {
                eprintln!("POPUP_CLICK button_event serial={serial}");
                (state.entered_popup, serial)
            }
            _ => return,
        };
        let (Some(p), serial) = is_popup else {
            return;
        };
        state.counts[usize::from(p)].button += 1;
        eprintln!(
            "POPUP_CLICK button label={} serial={serial}",
            if p { "popup" } else { "parent" }
        );
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore XdgPositioner);
delegate_noop!(App: ignore XdgToplevel);
delegate_noop!(App: ignore XdgPopup);

impl App {
    fn solid_buffer(&mut self, qh: &QueueHandle<App>, size: (i32, i32)) -> Option<WlBuffer> {
        let shm = self.shm.clone()?;
        let (w, h) = size;
        let stride = w * 4;
        let path = std::env::temp_dir().join(format!("aero-popup-click-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .ok()?;
        file.set_len((stride * h) as u64).ok()?;
        file.write_all(&vec![90u8; (stride * h) as usize]).ok()?;
        file.seek(SeekFrom::Start(0)).ok()?;
        let pool = shm.create_pool(file.as_fd(), stride * h, qh, ());
        let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, qh, ());
        self.buffers.push(buffer.clone());
        Some(buffer)
    }

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
        let Some(parent_xdg) = self.parent_xdg.clone() else {
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
        let popup = popup_xdg.get_popup(Some(&parent_xdg), &positioner, qh, ());
        // Regra do F1-popups: grab ANTES do commit que mapeia a superfície.
        if let Some(seat) = self.seat.clone() {
            popup.grab(&seat, 0);
            eprintln!("POPUP_CLICK grab_requested serial=0");
        }
        self.popup = Some(popup);
        self.popup_surface = Some(popup_surface.clone());
        // Um popup SEM buffer tem geometria 0x0 e nenhuma região de input:
        // é preciso anexar um buffer também a ele para o hit-test enxergar.
        if let Some(buffer) = self.solid_buffer(qh, (120, 40)) {
            popup_surface.attach(Some(&buffer), 0, 0);
        }
        popup_surface.commit();
        if let Some(buffer) = self.solid_buffer(qh, (200, 150)) {
            parent.attach(Some(&buffer), 0, 0);
        }
        parent.commit();
        eprintln!("POPUP_CLICK committed");
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
        shm: None,
        wm_base: None,
        seat: None,
        parent: None,
        parent_xdg: None,
        popup_surface: None,
        popup: None,
        popup_created: false,
        popup_configured: false,
        entered_popup: None,
        counts: std::array::from_fn(|_| Counts::default()),
        buffers: Vec::new(),
    };
    queue.roundtrip(&mut app)?;

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let _pointer = seat.get_pointer(&qh, ());
    let parent = compositor.create_surface(&qh, ());
    let parent_xdg = wm_base.get_xdg_surface(&parent, &qh, 0);
    let toplevel = parent_xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-popup-click".into());
    app.parent = Some(parent.clone());
    app.parent_xdg = Some(parent_xdg);
    parent.commit();
    eprintln!("POPUP_CLICK toplevel_committed");

    let deadline = std::time::Instant::now() + Duration::from_secs(9);
    while std::time::Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!(
        "POPUP_CLICK contadores pai={:?} popup={:?}",
        app.counts[0], app.counts[1]
    );
    eprintln!("POPUP_CLICK OK");
    Ok(())
}
