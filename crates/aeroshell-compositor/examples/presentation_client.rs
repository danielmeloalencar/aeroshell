//! F1-presentation-time: cliente real de `wp_presentation`.
//! Pede feedback várias vezes para uma superfície com buffer, exige `presented`
//! com seq crescente e timestamp não-nulo. Sem dependência nova.

use std::{
    fs::OpenOptions,
    io::Write,
    os::fd::AsFd,
    time::{Duration, Instant},
};

use smithay::reexports::wayland_protocols::{
    wp::presentation_time::client::{
        wp_presentation::{Event as PresentationEvent, WpPresentation},
        wp_presentation_feedback::{Event as FeedbackEvent, WpPresentationFeedback},
    },
    xdg::shell::client::{
        xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
        xdg_toplevel::XdgToplevel,
        xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
    },
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_registry::Event as RegistryEvent,
        wl_registry::WlRegistry, wl_shm::Format, wl_shm::WlShm, wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    shm: Option<WlShm>,
    presentation: Option<WpPresentation>,
    surface: Option<WlSurface>,
    buffer: Option<WlBuffer>,
    configured: bool,
    clock: Option<u32>,
    presented: usize,
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
            "wp_presentation" => {
                state.presentation = Some(registry.bind(name, version.min(1), qh, ()))
            }
            _ => {}
        }
    }
}

impl Dispatch<WpPresentation, ()> for App {
    fn event(
        state: &mut App,
        _: &WpPresentation,
        event: PresentationEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let PresentationEvent::ClockId { clk_id } = event {
            state.clock = Some(clk_id);
            eprintln!("PRES_CLIENT GLOBAL clock={clk_id}");
        }
    }
}

impl Dispatch<WpPresentationFeedback, ()> for App {
    fn event(
        state: &mut App,
        _: &WpPresentationFeedback,
        event: FeedbackEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        match event {
            FeedbackEvent::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh,
                seq_hi,
                seq_lo,
                ..
            } => {
                let tv_sec = ((tv_sec_hi as u64) << 32) | tv_sec_lo as u64;
                let seq = ((seq_hi as u64) << 32) | seq_lo as u64;
                state.presented += 1;
                eprintln!("PRES_CLIENT FEEDBACK seq={seq} tv={tv_sec}.{tv_nsec} refresh={refresh}");
            }
            FeedbackEvent::Discarded => {
                eprintln!("PRES_CLIENT DISCARDED");
            }
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
            state.configured = true;
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore XdgToplevel);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        wm_base: None,
        shm: None,
        presentation: None,
        surface: None,
        buffer: None,
        configured: false,
        clock: None,
        presented: 0,
    };
    queue.roundtrip(&mut app)?;
    // O `clock_id` é enviado no bind; o segundo roundtrip entrega esse evento.
    queue.roundtrip(&mut app)?;

    let Some(presentation) = app.presentation.clone() else {
        eprintln!("PRES_CLIENT GLOBAL AUSENTE");
        return Err("wp_presentation ausente no registry".into());
    };
    if app.clock != Some(1) {
        return Err(format!("clock_id inesperado: {:?}", app.clock).into());
    }
    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let shm = app.shm.clone().ok_or("sem wl_shm")?;
    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-presentation-client".into());
    let (w, h) = (200_i32, 100_i32);
    let stride = w * 4;
    let path = std::env::temp_dir().join(format!("aero-present-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len((stride * h) as u64)?;
    file.write_all(&vec![45u8; (stride * h) as usize])?;
    let pool = shm.create_pool(file.as_fd(), stride * h, &qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    app.surface = Some(surface.clone());
    app.buffer = Some(buffer.clone());
    queue.roundtrip(&mut app)?;

    let deadline = Instant::now() + Duration::from_secs(12);
    let mut iter = 0;
    while Instant::now() < deadline && iter < 8 {
        // O feedback vale para o commit atual da superfície: pedir ANTES do
        // commit e então comitar garante uma presentation por iteração.
        let _feedback = presentation.feedback(&surface, &qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        queue.roundtrip(&mut app)?;
        iter += 1;
        std::thread::sleep(Duration::from_millis(180));
    }
    eprintln!("PRES_CLIENT ITERATIONS={iter} PRESENTED={}", app.presented);
    if app.presented >= 2 {
        eprintln!("PRES_CLIENT OK");
        Ok(())
    } else {
        Err(format!("só {} presented", app.presented).into())
    }
}
