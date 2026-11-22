//! Cliente de teste F1i+ (no produto): lista globals, exercita xdg_activation.
//!
//! Serve ao harness `tests/integration/f1i_protocols.py`. Usa só `wayland-client`
//! (dev-dependency existente) e bindings reexportados pelo Smithay 0.7.0 fixado.
//! Nenhuma dependência nova e nenhuma variável de ambiente lida fora de `bench`.

use std::{
    fs::OpenOptions,
    os::fd::AsFd,
    time::{Duration, Instant},
};

use smithay::reexports::wayland_protocols::{
    wp::text_input::zv3::client::zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    xdg::{
        activation::v1::client::{
            xdg_activation_token_v1::{Event as TokenEvent, XdgActivationTokenV1},
            xdg_activation_v1::XdgActivationV1,
        },
        shell::client::{
            xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
            xdg_toplevel::XdgToplevel,
            xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
        },
    },
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

struct App {
    globals: Vec<(String, u32)>,
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    shm: Option<WlShm>,
    activation: Option<XdgActivationV1>,
    surface: Option<WlSurface>,
    pointer_serial: Option<u32>,
    token: Option<String>,
    activated: bool,
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
            state.globals.push((interface.clone(), version));
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
                "xdg_activation_v1" => {
                    state.activation = Some(registry.bind(name, version.min(1), qh, ()))
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

impl Dispatch<WlPointer, ()> for App {
    fn event(
        state: &mut App,
        _: &WlPointer,
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

impl Dispatch<XdgActivationTokenV1, ()> for App {
    fn event(
        state: &mut App,
        _: &XdgActivationTokenV1,
        event: TokenEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let TokenEvent::Done { token } = event {
            eprintln!("F1I_CLIENT token_recebido");
            state.token = Some(token);
        }
    }
}

delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore XdgToplevel);
delegate_noop!(App: ignore XdgActivationV1);
delegate_noop!(App: ignore ZwpTextInputManagerV3);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let display = connection.display();
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut app = App {
        globals: Vec::new(),
        compositor: None,
        wm_base: None,
        seat: None,
        shm: None,
        activation: None,
        surface: None,
        pointer_serial: None,
        token: None,
        activated: false,
    };
    queue.roundtrip(&mut app)?;

    for (interface, version) in &app.globals {
        eprintln!("F1I_CLIENT global {interface} v{version}");
    }
    let required = [
        "zwp_text_input_manager_v3",
        "zwp_input_method_manager_v2",
        "xdg_activation_v1",
    ];
    let mut missing = 0;
    for interface in required {
        let present = app.globals.iter().any(|(name, _)| name == interface);
        eprintln!(
            "F1I_CLIENT CHECK {interface} {}",
            if present { "presente" } else { "AUSENTE" }
        );
        if !present {
            missing += 1;
        }
    }
    if missing > 0 {
        eprintln!("F1I_CLIENT FALHA: {missing} globals ausentes");
        return Err("globals obrigatórios ausentes".into());
    }

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let shm = app.shm.clone().ok_or("sem wl_shm")?;
    let activation = app.activation.clone().ok_or("sem xdg_activation_v1")?;
    let _pointer = seat.get_pointer(&qh, ());
    let surface = compositor.create_surface(&qh, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg_surface.get_toplevel(&qh, ());
    toplevel.set_title("aero-f1i-test".into());
    app.surface = Some(surface.clone());

    // Sem buffer a geometria é 0×0 e o input sintético não acerta a janela:
    // o serial do botão (usado no token) nunca chegaria ao cliente.
    let (width, height, stride) = (800i32, 600i32, 800i32 * 4);
    let path = std::env::temp_dir().join(format!("aero-f1i-shm-{}", std::process::id()));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len((stride * height) as u64)?;
    let pool = shm.create_pool(file.as_fd(), stride * height, &qh, ());
    let buffer = pool.create_buffer(0, width, height, stride, Format::Argb8888, &qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    eprintln!("F1I_CLIENT pronto_para_input");

    let deadline = Instant::now() + Duration::from_secs(8);
    while app.pointer_serial.is_none() && Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(25));
    }
    let serial = app.pointer_serial.ok_or("nenhum button event (serial)")?;
    eprintln!("F1I_CLIENT serial_recebido {serial}");

    let token = activation.get_activation_token(&qh, ());
    token.set_app_id("aero-f1i-test".into());
    token.set_serial(serial, &seat);
    token.set_surface(&surface);
    token.commit();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.token.is_none() && Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(25));
    }
    let token = app.token.take().ok_or("token de ativação não chegou")?;
    activation.activate(token, &surface);
    app.activated = true;
    eprintln!("F1I_CLIENT activation_requested serial={serial}");
    queue.roundtrip(&mut app)?;
    if !app.activated {
        return Err("ativação não enviada".into());
    }
    eprintln!("F1I_CLIENT OK");
    Ok(())
}
