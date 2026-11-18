//! aeroshell-compositor — dono do estado e do input.

mod aero_frame_protocol;
mod backend;
mod cli;
#[cfg(feature = "bench")]
mod frame_metrics;
mod input;
mod protocols;
mod state;

use std::{
    error::Error,
    io::BufRead,
    os::{
        fd::{FromRawFd, OwnedFd},
        unix::net::{UnixListener, UnixStream},
    },
    sync::Arc,
};

use calloop::channel;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};
pub use wayland_server;
pub use wayland_server::protocol::{wl_seat, wl_surface};

pub use state::{AeroState, ClientState};

pub struct CalloopData {
    state: AeroState,
    display_handle: DisplayHandle,
}

fn main() -> Result<(), Box<dyn Error>> {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }

    // Converter a ponta privilegiada ANTES de qualquer biblioteca cliente Wayland:
    // winit/wayland-client consomem WAYLAND_SOCKET e tomariam posse do mesmo fd.
    let privileged_fd: Option<OwnedFd> = std::env::var_os("AERO_PRIVILEGED_FD")
        .and_then(|value| value.to_str().and_then(|v| v.parse::<i32>().ok()))
        .map(|fd| {
            // SAFETY: `fd` é a ponta do socketpair herdada do supervisor, passa
            // exatamente uma vez para `OwnedFd`, que passa a ser seu único dono.
            unsafe { OwnedFd::from_raw_fd(fd) }
        });
    if std::env::var_os("AERO_PRIVILEGED_FD").is_some() {
        std::env::remove_var("AERO_PRIVILEGED_FD");
    }

    let opts = cli::parse();
    let selected_backend = backend::current(&opts);
    tracing::info!(
        backend = selected_backend.name(),
        seat = %selected_backend.seat_name(),
        socket = %opts.socket,
        "iniciando AeroShell"
    );

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let display: Display<AeroState> = Display::new()?;
    let display_handle = display.handle();
    let state = AeroState::new(&mut event_loop, display, &opts)?;

    // O env do host NÃO é alterado: só o processo filho recebe o socket do AeroShell.
    let socket_name = state.socket_name.clone();
    if let Some(command) = &opts.command {
        // `sh -c` permite `--command "zenity --info --text=..."` com argumentos.
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("WAYLAND_DISPLAY", &socket_name)
            // A ponta privilegiada do socketpair NUNCA é herdada por clientes comuns.
            .env_remove("WAYLAND_SOCKET")
            // Sem remover DISPLAY, GTK3 abre no X11 do host em vez do nosso compositor.
            .env_remove("DISPLAY")
            .env("GDK_BACKEND", "wayland")
            .spawn();
        match status {
            Ok(mut child) => {
                tracing::info!(?command, pid = child.id(), "cliente inicial iniciado");
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(err) => tracing::warn!(?command, %err, "falha ao iniciar cliente"),
        }
    }

    let mut data = CalloopData {
        state,
        display_handle,
    };

    // Knob de benchmark: simula janelas existentes para que get_frame com
    // window_id=0 não seja tratado como corrida de fechamento.
    // Só existe sob a feature `bench`; o binário de produção não lê este env.
    #[cfg(feature = "bench")]
    if let Ok(count) = std::env::var("AERO_FRAME_BENCH_WINDOWS").and_then(|v| {
        v.parse::<usize>()
            .map_err(|_| std::env::VarError::NotPresent)
    }) {
        data.state.test_window_count = count;
    }

    // O supervisor entrega uma ponta do socketpair ao shell via WAYLAND_SOCKET.
    // Essa conexão é marcada como privilegiada; o socket público nunca é.
    if let Some(fd) = privileged_fd {
        let stream = UnixStream::from(fd);
        data.display_handle.insert_client(
            stream,
            Arc::new(ClientState {
                is_shell: true,
                ..Default::default()
            }),
        )?;
        tracing::info!("shell privilegiado inserido via AERO_PRIVILEGED_FD");
    }

    backend::init(&opts, &mut event_loop, &mut data)?;

    // Injeção de ponteiro sintético (S3): `motion x y`, `button press|release`.
    if let Some(path) = std::env::var_os("AERO_TEST_INPUT_SOCK") {
        let (sender, source) = channel::channel::<String>();
        let listener = UnixListener::bind(&path)?;
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                for line in std::io::BufReader::new(stream)
                    .lines()
                    .map_while(Result::ok)
                {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        });
        event_loop
            .handle()
            .insert_source(source, |event, _, data| {
                let channel::Event::Msg(msg) = event else {
                    return;
                };
                let mut parts = msg.split_whitespace();
                match parts.next() {
                    Some("motion") => {
                        let x = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        let y = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                        data.state.inject_motion(x, y);
                    }
                    Some("button") => match parts.next() {
                        Some("press") => data.state.inject_button(true),
                        Some("release") => data.state.inject_button(false),
                        _ => {}
                    },
                    _ => {}
                }
            })?;
        tracing::info!(?path, "injeção de ponteiro sintético ativa");
    }

    event_loop.run(None, &mut data, |data| {
        #[cfg(feature = "bench")]
        data.state.check_frame_timeouts();
        let _ = data;
    })?;
    #[cfg(feature = "bench")]
    data.state.log_frame_summary();

    Ok(())
}

#[cfg(test)]
mod protocol_tests {
    //! Testes de protocolo no mesmo processo: socketpair, sem display, env ou exec.
    use super::ClientState as ServerClientState;
    use super::*;
    use crate::cli::Options;
    use std::{
        os::unix::net::UnixStream,
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use wayland_client::{
        backend::Backend, protocol::wl_registry, Connection, Dispatch, QueueHandle,
    };

    mod client_protocol {
        mod core_interfaces {
            #[allow(clippy::single_component_path_imports)]
            use wayland_backend;
            wayland_scanner::generate_interfaces!("/usr/share/wayland/wayland.xml");
        }

        mod generated {
            #[allow(unused_imports)]
            use super::core_interfaces::*;
            #[allow(clippy::single_component_path_imports)]
            use wayland_backend;
            #[allow(clippy::single_component_path_imports)]
            use wayland_client;
            pub use wayland_client::protocol::{wl_compositor, wl_seat, wl_surface};

            wayland_scanner::generate_interfaces!("../../protocols/aero_frame_v1.xml");
            wayland_scanner::generate_client_code!("../../protocols/aero_frame_v1.xml");
        }
        pub use generated::*;
        pub mod xdg {
            #[allow(unused_imports)]
            use super::core_interfaces::*;
            use wayland_backend;
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

    use client_protocol::{
        aero_frame_manager_v1::AeroFrameManagerV1,
        aero_frame_v1::{AeroFrameV1, Event as FrameEvent},
        wl_compositor::WlCompositor,
        wl_seat::WlSeat,
        wl_surface::WlSurface,
        xdg::{xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase},
    };

    #[derive(Default)]
    struct ClientState {
        manager: Option<AeroFrameManagerV1>,
        compositor: Option<WlCompositor>,
        shell: Option<XdgWmBase>,
        seat: Option<WlSeat>,
        surface: Option<WlSurface>,
        frames: Vec<AeroFrameV1>,
        configured: usize,
        closed: usize,
        saw_manager: bool,
        protocol_error: bool,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for ClientState {
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
                match interface.as_str() {
                    "wl_compositor" => {
                        state.compositor = Some(registry.bind::<WlCompositor, (), Self>(
                            name,
                            version.min(5),
                            qh,
                            (),
                        ));
                    }
                    "wl_seat" => {
                        state.seat =
                            Some(registry.bind::<WlSeat, (), Self>(name, version.min(9), qh, ()));
                    }
                    "xdg_wm_base" => {
                        state.shell = Some(registry.bind::<XdgWmBase, (), Self>(
                            name,
                            version.min(6),
                            qh,
                            (),
                        ));
                    }
                    "aero_frame_manager_v1" => {
                        state.saw_manager = true;
                        state.manager = Some(registry.bind::<AeroFrameManagerV1, (), Self>(
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

    macro_rules! empty_dispatch {
        ($ty:ty, $event:ty) => {
            impl Dispatch<$ty, ()> for ClientState {
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
    empty_dispatch!(WlCompositor, client_protocol::wl_compositor::Event);
    empty_dispatch!(WlSurface, client_protocol::wl_surface::Event);
    empty_dispatch!(WlSeat, client_protocol::wl_seat::Event);
    empty_dispatch!(XdgWmBase, client_protocol::xdg::xdg_wm_base::Event);
    empty_dispatch!(XdgSurface, client_protocol::xdg::xdg_surface::Event);
    empty_dispatch!(XdgToplevel, client_protocol::xdg::xdg_toplevel::Event);
    impl Dispatch<AeroFrameManagerV1, ()> for ClientState {
        fn event(
            state: &mut Self,
            _proxy: &AeroFrameManagerV1,
            _event: client_protocol::aero_frame_manager_v1::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
            state.protocol_error = true;
        }
    }

    impl Dispatch<AeroFrameV1, ()> for ClientState {
        fn event(
            state: &mut Self,
            _proxy: &AeroFrameV1,
            event: FrameEvent,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
            match event {
                FrameEvent::Configure { serial, .. } => {
                    state.configured += 1;
                    let _ = serial;
                }
                FrameEvent::Closed => state.closed += 1,
            }
        }
    }

    struct Harness<'a> {
        event_loop: EventLoop<'a, CalloopData>,
        data: CalloopData,
        connection: Connection,
        queue: wayland_client::EventQueue<ClientState>,
        client: ClientState,
        protocol_error: bool,
    }

    impl<'a> Harness<'a> {
        fn new(privileged: bool) -> Self {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);
            let (server, client) = UnixStream::pair().unwrap();
            let mut event_loop = EventLoop::try_new().unwrap();
            let display = Display::new().unwrap();
            let mut display_handle = display.handle();
            let opts = Options {
                socket: format!(
                    "aero-test-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, Ordering::Relaxed)
                ),
                ..Default::default()
            };
            let state = AeroState::new(&mut event_loop, display, &opts).unwrap();
            display_handle
                .insert_client(
                    server,
                    Arc::new(ServerClientState {
                        is_shell: privileged,
                        ..Default::default()
                    }),
                )
                .unwrap();
            let mut data = CalloopData {
                state,
                display_handle,
            };
            data.state.test_window_count = 1;

            let connection = Connection::from_backend(Backend::connect(client).unwrap());
            let queue = connection.new_event_queue();
            let qh = queue.handle();
            connection.display().get_registry(&qh, ());
            let mut harness = Self {
                event_loop,
                data,
                connection,
                queue,
                client: ClientState::default(),
                protocol_error: false,
            };
            harness.pump();
            harness
        }

        fn pump(&mut self) -> Option<String> {
            let error = None;
            if self.queue.flush().is_err() {
                return Some("flush falhou".to_owned());
            }
            for _ in 0..40 {
                // O Display é owned pelo source `Generic` dentro do event loop;
                // `dispatch` aciona o callback que chama `dispatch_clients`.
                let _ = self
                    .event_loop
                    .dispatch(Some(Duration::from_millis(1)), &mut self.data);
                let _ = self.data.display_handle.flush_clients();
                if let Some(guard) = self.connection.prepare_read() {
                    let _ = guard.read();
                }
                match self.queue.dispatch_pending(&mut self.client) {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("TEST client dispatch error: {err:?}");
                        self.protocol_error = true;
                    }
                }
                if self.client.protocol_error {
                    self.client.protocol_error = false;
                    self.protocol_error = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            error
        }

        fn create_surface(&mut self) {
            let compositor = self.client.compositor.clone().unwrap();
            let surface = compositor.create_surface(&self.queue.handle(), ());
            self.client.surface = Some(surface);
            self.pump();
        }

        fn get_frame(&mut self, window_id: u32) -> Option<String> {
            let manager = self.client.manager.clone().unwrap();
            let surface = self.client.surface.clone().unwrap();
            let frame = manager.get_frame(&surface, window_id, &self.queue.handle(), ());
            self.client.frames.push(frame);
            self.pump()
        }
    }

    #[test]
    fn a_public_client_never_sees_the_manager() {
        let h = Harness::new(false);
        assert!(!h.client.saw_manager);
        assert!(h.client.manager.is_none());
    }

    #[test]
    fn privileged_client_completes_roundtrip() {
        let mut h = Harness::new(true);
        assert!(h.client.saw_manager);
        h.create_surface();
        assert!(h.get_frame(0).is_none());
        assert_eq!(h.client.configured, 1);
        assert_eq!(h.client.closed, 0);
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn second_get_frame_is_already_constructed() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.get_frame(0).is_none());
        let compositor = h.client.compositor.clone().unwrap();
        let surface2 = compositor.create_surface(&h.queue.handle(), ());
        h.client.surface = Some(surface2);
        h.pump();
        let _err = h.get_frame(0);
        assert!(
            h.protocol_error,
            "segundo get_frame deveria gerar erro de protocolo"
        );
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn surface_with_toplevel_role_is_rejected() {
        let mut h = Harness::new(true);
        h.create_surface();
        let shell = h.client.shell.clone().unwrap();
        let surface = h.client.surface.clone().unwrap();
        let xdg_surface = shell.get_xdg_surface(&surface, &h.queue.handle(), ());
        let _toplevel = xdg_surface.get_toplevel(&h.queue.handle(), ());
        h.pump();
        let _err = h.get_frame(0);
        let role_rejected = h.data.state.frames.is_empty();
        assert!(
            role_rejected,
            "superfície com toplevel não deveria virar frame"
        );
    }

    #[test]
    fn move_with_fake_serial_is_ignored() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.get_frame(0).is_none());
        let seat = h.client.seat.clone().unwrap();
        let frame = h.client.frames.first().unwrap().clone();
        frame._move(&seat, 0xDEAD);
        h.pump();
        assert!(h.data.state.last_geometry_action.is_none());
    }
}
