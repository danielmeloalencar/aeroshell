//! Cliente de teste do protocolo aero_frame_v1 (S5): valida global, get_frame e eventos.

#[cfg(feature = "bench")]
use std::os::fd::AsFd;
use wayland_client::{protocol::wl_registry, Connection, Dispatch, Proxy, QueueHandle};

mod protocol {
    mod core_interfaces {
        wayland_scanner::generate_interfaces!("/usr/share/wayland/wayland.xml");
    }

    mod generated {
        #[allow(unused_imports)]
        use super::core_interfaces::*;
        #[allow(clippy::single_component_path_imports)]
        use wayland_client;
        #[cfg(feature = "bench")]
        pub use wayland_client::protocol::{wl_buffer, wl_shm, wl_shm_pool};
        pub use wayland_client::protocol::{wl_compositor, wl_seat, wl_surface};

        wayland_scanner::generate_interfaces!("../../protocols/aero_frame_v1.xml");
        wayland_scanner::generate_client_code!("../../protocols/aero_frame_v1.xml");

        pub mod xdg {
            #[allow(unused_imports)]
            use super::super::core_interfaces::*;
            #[allow(clippy::single_component_path_imports)]
            use wayland_client;
            pub use wayland_client::protocol::{wl_output, wl_seat, wl_surface};
            wayland_scanner::generate_interfaces!(
                "/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml"
            );
            wayland_scanner::generate_client_code!(
                "/usr/share/wayland-protocols/stable/xdg-shell/xdg-shell.xml"
            );
        }
    }

    pub use generated::*;
}

use protocol::{
    aero_frame_manager_v1::{AeroFrameManagerV1, Event as ManagerEvent},
    aero_frame_v1::{AeroFrameV1, Event as FrameEvent},
    wl_compositor::WlCompositor,
    wl_seat::WlSeat,
    wl_surface::WlSurface,
    xdg::{xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase},
};
#[cfg(feature = "bench")]
use protocol::{
    wl_buffer::{self, WlBuffer},
    wl_shm::{self, WlShm},
    wl_shm_pool,
};

/// Cria um buffer shm com o tamanho pedido. Arquivo em /dev/shm: sem libc,
/// sem `from_raw_fd` (o fd vem de `File::as_fd`, com dono).
#[cfg(feature = "bench")]
fn make_buffer(shm: &WlShm, qh: &QueueHandle<State>, width: i32, height: i32) -> Option<WlBuffer> {
    let stride = width * 4;
    let size = stride * height;
    let path = format!("/dev/shm/aero-frame-bench-{}", std::process::id());
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .ok()?;
    file.set_len(size as u64).ok()?;
    let pool = shm.create_pool(file.as_fd(), size, qh, ());
    Some(pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ()))
}

struct State {
    compositor: Option<WlCompositor>,
    manager: Option<AeroFrameManagerV1>,
    frames: Vec<AeroFrameV1>,
    xdg_shell: Option<XdgWmBase>,
    seat: Option<WlSeat>,
    #[cfg(feature = "bench")]
    shm: Option<WlShm>,
    surface: Option<WlSurface>,
    configured: u32,
    closed: u32,
    saw_manager: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            eprintln!("global {name}: {interface} v{version}");
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<WlCompositor, (), State>(
                        name,
                        version.min(5),
                        qh,
                        (),
                    ));
                }
                "xdg_wm_base" => {
                    state.xdg_shell =
                        Some(registry.bind::<XdgWmBase, (), State>(name, version.min(6), qh, ()));
                }
                "wl_seat" => {
                    state.seat =
                        Some(registry.bind::<WlSeat, (), State>(name, version.min(9), qh, ()));
                }
                #[cfg(feature = "bench")]
                "wl_shm" => {
                    state.shm =
                        Some(registry.bind::<WlShm, (), State>(name, version.min(2), qh, ()));
                }
                "aero_frame_manager_v1" => {
                    eprintln!(
                        "bind manager global={name} v={} iface={}",
                        version.min(1),
                        AeroFrameManagerV1::interface().name
                    );
                    state.saw_manager = true;
                    state.manager = Some(registry.bind::<AeroFrameManagerV1, (), State>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    ));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<WlCompositor, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlCompositor,
        _event: protocol::wl_compositor::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlSurface,
        _event: protocol::wl_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(feature = "bench")]
macro_rules! empty_dispatch {
    ($ty:ty, $event:ty) => {
        impl Dispatch<$ty, ()> for State {
            fn event(
                _state: &mut Self,
                _proxy: &$ty,
                _event: $event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }
    };
}
#[cfg(feature = "bench")]
empty_dispatch!(WlShm, wl_shm::Event);
#[cfg(feature = "bench")]
empty_dispatch!(wl_shm_pool::WlShmPool, wl_shm_pool::Event);
#[cfg(feature = "bench")]
empty_dispatch!(WlBuffer, wl_buffer::Event);

impl Dispatch<AeroFrameManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &AeroFrameManagerV1,
        event: ManagerEvent,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            ManagerEvent::FrameRequested {
                window_id,
                width,
                height,
                ..
            } => {
                eprintln!("frame_requested window_id={window_id} {width}x{height}");
                let _ = qh;
            }
        }
    }
}

impl Dispatch<XdgWmBase, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &XdgWmBase,
        _event: protocol::xdg::xdg_wm_base::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgSurface, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &XdgSurface,
        _event: protocol::xdg::xdg_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<XdgToplevel, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &XdgToplevel,
        _event: protocol::xdg::xdg_toplevel::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WlSeat,
        _event: protocol::wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<AeroFrameV1, ()> for State {
    fn event(
        state: &mut Self,
        frame: &AeroFrameV1,
        event: FrameEvent,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            FrameEvent::Configure {
                serial,
                width,
                height,
                ..
            } => {
                state.configured += 1;
                #[cfg(not(feature = "bench"))]
                let _ = (qh, width, height);
                #[cfg(feature = "bench")]
                {
                    let t_nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    eprintln!("CLIENT_CONFIGURE_RECV serial={serial} t={t_nanos} {width}x{height}");
                    if std::env::var_os("AERO_FRAME_BENCH").is_some() {
                        let delay: u64 = std::env::var("AERO_FRAME_ACK_DELAY_MS")
                            .ok()
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0);
                        if delay > 0 {
                            std::thread::sleep(std::time::Duration::from_millis(delay));
                        }
                        if let (Some(shm), Some(surface)) = (&state.shm, &state.surface) {
                            let mismatch =
                                std::env::var_os("AERO_FRAME_CONTENT_MISMATCH").is_some();
                            let bw = if mismatch { (width - 20).max(1) } else { width };
                            let bh = if mismatch {
                                (height - 12).max(1)
                            } else {
                                height
                            };
                            if let Some(buffer) = make_buffer(shm, qh, bw, bh) {
                                surface.attach(Some(&buffer), 0, 0);
                            }
                        }
                        if let Some(surface) = &state.surface {
                            surface.commit();
                        }
                        eprintln!("CLIENT_COMMIT serial={serial}");
                        frame.ack_configure(serial);
                        let ack_nanos = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0);
                        eprintln!("CLIENT_ACK_SENT serial={serial} t={ack_nanos}");
                    } else {
                        frame.ack_configure(serial);
                    }
                }
                #[cfg(not(feature = "bench"))]
                frame.ack_configure(serial);
            }
            FrameEvent::Closed => {
                state.closed += 1;
                eprintln!("closed recebido");
            }
        }
    }
}

fn main() {
    if std::env::var_os("AERO_WAIT_SOCKET").is_some() {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_owned());
        let socket = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
        let path = std::path::Path::new(&runtime_dir).join(&socket);
        while !path.exists() {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let connection = Connection::connect_to_env().expect("conectar ao socket do AeroShell");
    let display = connection.display();
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    display.get_registry(&qh, ());

    let mut state = State {
        compositor: None,
        manager: None,
        frames: Vec::new(),
        xdg_shell: None,
        seat: None,
        #[cfg(feature = "bench")]
        shm: None,
        surface: None,
        configured: 0,
        closed: 0,
        saw_manager: false,
    };
    queue.roundtrip(&mut state).expect("roundtrip");

    if std::env::var_os("AERO_EXPECT_NO_GLOBAL").is_some() {
        assert!(!state.saw_manager, "cliente comum NÃO deveria ver o global");
        eprintln!("OK: aero_frame_manager_v1 não é visível para cliente comum");
        return;
    }

    if std::env::var_os("AERO_ONLY_BIND").is_some() {
        queue.roundtrip(&mut state).expect("roundtrip bind only");
        eprintln!("OK bind only");
        return;
    }

    let compositor = state
        .compositor
        .clone()
        .expect("wl_compositor não anunciado");
    let surface = compositor.create_surface(&qh, ());
    state.surface = Some(surface.clone());
    queue.roundtrip(&mut state).expect("roundtrip surface");

    if std::env::var_os("AERO_TEST_ROLE").is_some() {
        let shell = state.xdg_shell.clone().expect("xdg_wm_base não anunciado");
        let xdg_surface = shell.get_xdg_surface(&surface, &qh, ());
        let _toplevel = xdg_surface.get_toplevel(&qh, ());
        queue.roundtrip(&mut state).expect("roundtrip xdg toplevel");
        eprintln!("toplevel criado na mesma wl_surface");
    }

    let manager = state
        .manager
        .clone()
        .expect("aero_frame_manager_v1 não anunciado");
    let window_id: u32 = std::env::var("AERO_FRAME_WINDOW_ID")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let surface = state.surface.clone().unwrap();
    let frame = manager.get_frame(&surface, window_id, &qh, ());
    state.frames.push(frame);
    if std::env::var_os("AERO_TEST_DOUBLE").is_some() {
        // Superfície nova: isola `already_constructed` do erro de role.
        let surface2 = compositor.create_surface(&qh, ());
        queue.roundtrip(&mut state).expect("roundtrip surface2");
        manager.get_frame(&surface2, window_id, &qh, ());
    }
    queue.roundtrip(&mut state).expect("roundtrip get_frame");
    queue.roundtrip(&mut state).expect("roundtrip configure");
    #[cfg(feature = "bench")]
    if std::env::var_os("AERO_FRAME_BENCH").is_some() {
        queue.roundtrip(&mut state).expect("roundtrip bench flush");
    }

    if std::env::var_os("AERO_TEST_FALSE_SERIAL").is_some() {
        let seat = state.seat.clone().expect("wl_seat não anunciado");
        if let Some(frame) = state.frames.first() {
            // serial 0xDEAD nunca foi gerado por um clique no frame.
            frame._move(&seat, 0xDEAD);
        }
        queue.roundtrip(&mut state).expect("roundtrip false serial");
        eprintln!("move com serial falso enviado");
    }

    if std::env::var_os("AERO_TEST_OK").is_some() {
        assert!(state.saw_manager, "cliente privilegiado deve ver o global");
        assert_eq!(state.configured, 1, "configure não recebido");
        assert_eq!(state.closed, 0);
        eprintln!("OK: privilegiado viu o global e completou o roundtrip");
    }

    eprintln!(
        "configured={} closed={} saw_manager={}",
        state.configured, state.closed, state.saw_manager
    );
    if std::env::var_os("AERO_EXPECT_CLOSED").is_some() {
        assert_eq!(state.closed, 1, "esperado closed para janela inexistente");
    }
}
