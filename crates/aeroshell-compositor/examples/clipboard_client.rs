//! F1-clipboard-two-clients: cliente A (--offer) escreve, B (--receive) lê.
//! Sem dependência nova: só `wayland-client` (dev-dep existente) e bindings
//! reexportados pelo Smithay 0.7.0 fixado. Usado por
//! `tests/integration/f1_clipboard_two_clients.py`.

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::fd::AsFd,
    time::{Duration, Instant},
};

use smithay::reexports::wayland_protocols::xdg::shell::client::{
    xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
    xdg_toplevel::XdgToplevel,
    xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
};
use wayland_client::{
    delegate_noop, event_created_child,
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_data_device::{Event as DataDeviceEvent, WlDataDevice},
        wl_data_device_manager::WlDataDeviceManager,
        wl_data_offer::WlDataOffer,
        wl_data_source::{Event as DataSourceEvent, WlDataSource},
        wl_pointer::{Event as PointerEvent, WlPointer},
        wl_registry::{Event as RegistryEvent, WlRegistry},
        wl_seat::WlSeat,
        wl_shm::{Format, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

const MIME: &str = "text/plain;charset=utf-8";
const TEXT: &str = "aero-clipboard-cross-client-2026";

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    shm: Option<WlShm>,
    manager: Option<WlDataDeviceManager>,
    data_device: Option<WlDataDevice>,
    is_receiver: bool,
    pointer: Option<WlPointer>,
    source: Option<WlDataSource>,
    configured: bool,
    buffer: Option<WlBuffer>,
    pointer_serial: Option<u32>,
    selection_set: bool,
    received: Option<String>,
    recv_path: std::path::PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Offer,
    Receive,
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
            "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
            "wl_data_device_manager" => {
                state.manager = Some(registry.bind(name, version.min(3), qh, ()))
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

impl Dispatch<WlDataSource, ()> for App {
    fn event(
        _state: &mut App,
        _: &WlDataSource,
        event: DataSourceEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let DataSourceEvent::Send { mime_type, fd } = event {
            let mut file: File = fd.into();
            if mime_type == MIME {
                let _ = file.write_all(TEXT.as_bytes());
                eprintln!("CLIP_A SEND mime={mime_type} bytes={}", TEXT.len());
            }
        }
    }
}

impl Dispatch<WlDataDevice, ()> for App {
    fn event(
        state: &mut App,
        _: &WlDataDevice,
        event: DataDeviceEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        // O Smithay anuncia a seleção com `data_offer` + `selection`; `enter`
        // é só para drag-and-drop e não é enviado por set_data_device_focus.
        if state.is_receiver {
            if let DataDeviceEvent::Selection {
                id: Some(offer), ..
            } = event
            {
                let file = match OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&state.recv_path)
                {
                    Ok(file) => file,
                    Err(err) => {
                        eprintln!("CLIP_B ERRO ao criar arquivo de leitura: {err}");
                        return;
                    }
                };
                // O Wayland duplica o fd no pedido; fechar o nosso aqui é
                // seguro e o harness lê o arquivo depois da escrita.
                offer.receive(MIME.into(), file.as_fd());
                eprintln!("CLIP_B RECEIVE pedido");
            }
        }
    }

    // `wl_data_device.enter` cria um `wl_data_offer` filho: sem esta
    // especialização o wayland-client aborta no dispatch do evento.
    event_created_child!(App, WlDataDevice, [
        0 => (WlDataOffer, ()),
    ]);
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore XdgToplevel);
delegate_noop!(App: ignore WlSeat);
delegate_noop!(App: ignore WlDataDeviceManager);
delegate_noop!(App: ignore WlDataOffer);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("--offer") => Mode::Offer,
        Some("--receive") => Mode::Receive,
        other => return Err(format!("modo inválido: {other:?} (use --offer|--receive)").into()),
    };
    let recv_path = std::env::temp_dir().join(format!("aero-clipboard-{}", std::process::id()));
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        wm_base: None,
        seat: None,
        shm: None,
        manager: None,
        data_device: None,
        is_receiver: mode == Mode::Receive,
        pointer: None,
        source: None,
        configured: false,
        buffer: None,
        pointer_serial: None,
        selection_set: false,
        received: None,
        recv_path,
    };
    queue.roundtrip(&mut app)?;

    let compositor = app.compositor.clone().ok_or("sem wl_compositor")?;
    let wm_base = app.wm_base.clone().ok_or("sem xdg_wm_base")?;
    let seat = app.seat.clone().ok_or("sem wl_seat")?;
    let shm = app.shm.clone().ok_or("sem wl_shm")?;
    let manager = app.manager.clone().ok_or("sem wl_data_device_manager")?;
    let data_device = manager.get_data_device(&seat, &qh, ());
    app.data_device = Some(data_device.clone());

    // Superfície com buffer shm para virar alvo de hit-test/foco.
    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    let title = if mode == Mode::Offer {
        "aero-clipboard-A"
    } else {
        "aero-clipboard-B"
    };
    toplevel.set_title(title.into());
    let (w, h) = (200_i32, 100_i32);
    let stride = w * 4;
    let path = std::env::temp_dir().join(format!("aero-clipbuf-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)?;
    file.set_len((stride * h) as u64)?;
    file.write_all(&vec![40u8; (stride * h) as usize])?;
    let pool = shm.create_pool(file.as_fd(), stride * h, &qh, ());
    let buffer = pool.create_buffer(0, w, h, stride, Format::Argb8888, &qh, ());
    app.buffer = Some(buffer.clone());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    queue.roundtrip(&mut app)?;
    eprintln!(
        "CLIP_{} ready title={title}",
        if mode == Mode::Offer { 'A' } else { 'B' }
    );

    if mode == Mode::Offer {
        let source = manager.create_data_source(&qh, ());
        source.offer(MIME.into());
        app.source = Some(source.clone());
    }
    let pointer = seat.get_pointer(&qh, ());
    app.pointer = Some(pointer);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        if mode == Mode::Offer && !app.selection_set {
            if let (Some(serial), Some(source)) = (app.pointer_serial, app.source.clone()) {
                data_device.set_selection(Some(&source), serial);
                app.selection_set = true;
                eprintln!("CLIP_A SELECTION_SET serial={serial}");
            }
        }
        if mode == Mode::Receive && app.received.is_none() {
            // O compositor escreve no fd depois do pedido; só ler quando o
            // arquivo tiver o tamanho esperado evita amostra vazia (corrida).
            let ready = std::fs::metadata(&app.recv_path)
                .map(|meta| meta.len() >= TEXT.len() as u64)
                .unwrap_or(false);
            if ready {
                let mut data = String::new();
                let _ = File::open(&app.recv_path).and_then(|mut f| f.read_to_string(&mut data));
                let got = data.trim_end_matches('\0').to_owned();
                if got == TEXT {
                    app.received = Some(got);
                    eprintln!("CLIP_B GOT {TEXT}");
                    eprintln!("CLIP_B OK");
                    break;
                }
                eprintln!("CLIP_B GOT inesperado={got:?}");
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    if mode == Mode::Receive {
        if app.received.is_none() {
            return Err("B não recebeu o texto esperado a tempo".into());
        }
        return Ok(());
    }
    if !app.selection_set {
        return Err("A nunca recebeu serial de botão para set_selection".into());
    }
    eprintln!("CLIP_A OK");
    Ok(())
}
