//! aeroshell-compositor — dono do estado e do input.

mod aero_frame_protocol;
mod backend;
mod cli;
// 02j: os testes puros precisam do módulo sem `bench`; o binário de produção
// não pode compilá-lo (evita código morto e mantém as strings fora dele).
#[cfg(any(test, feature = "bench"))]
mod frame_diagnostics;
#[cfg(feature = "bench")]
mod frame_metrics;
// 02o: readback de pixels do framebuffer; exclusivo de `bench`.
#[cfg(feature = "bench")]
mod frame_pixel_probe;
#[cfg(feature = "bench")]
mod frame_titlebar_probe;
mod grab;
mod input;
mod protocols;
mod render;
mod state;
mod supervisor;
mod window_ipc;

use std::{error::Error, io::BufRead, os::unix::net::UnixListener};

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
    let mut supervisor = supervisor::Supervisor::new();

    let mut data = CalloopData {
        state,
        display_handle,
    };

    // Knob de benchmark: simula janelas existentes para que associate com
    // window_id=0 não seja tratado como corrida de fechamento.
    // Só existe sob a feature `bench`; o binário de produção não lê este env.
    #[cfg(feature = "bench")]
    if let Ok(count) = std::env::var("AERO_FRAME_BENCH_WINDOWS").and_then(|v| {
        v.parse::<usize>()
            .map_err(|_| std::env::VarError::NotPresent)
    }) {
        data.state.test_window_count = count;
    }

    // O supervisor interno cria o socketpair privilegiado e lança o shell.
    // A ponta do compositor é inserida como is_shell=true; o socket público
    // nunca é privilegiado. Nenhum env decide privilégio (AGENTS.md §3.5).
    backend::init(&opts, &mut event_loop, &mut data)?;

    if let Some(shell_path) = &opts.shell {
        supervisor.spawn_shell(
            &mut data.display_handle,
            shell_path,
            &opts.shell_args,
            &opts.shell_env,
            &data.state.socket_name,
        )?;
    }

    // Cliente inicial: com frame_host (--shell), ADIA o lançamento até o primeiro
    // associate (cold-start temporário: a janela nunca aparece sem decoração).
    // Sem frame_host, lança imediato. Só vale para o primeiro cliente.
    if let Some(command) = &opts.command {
        if opts.shell.is_some() {
            data.state.pending_initial_command = Some(command.clone());
            tracing::info!(?command, "cliente inicial adiado até o frame associar");
        } else {
            match supervisor::spawn_public_client(&socket_name, command) {
                Ok(pid) => tracing::info!(?command, pid, "cliente inicial iniciado"),
                Err(err) => tracing::warn!(?command, %err, "falha ao iniciar cliente"),
            }
        }
    }

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

    // IPC de controle de janelas (list/focus/close/maximize) para o SDK/TS.
    if let Some(path) = std::env::var_os("AERO_WINDOW_IPC_SOCK") {
        let (sender, source) = channel::channel::<window_ipc::Command>();
        window_ipc::spawn(&path.to_string_lossy(), sender)?;
        event_loop
            .handle()
            .insert_source(source, |event, _, data| {
                let channel::Event::Msg((cmd, reply)) = event else {
                    return;
                };
                let resp = data.state.window_command(&cmd);
                let _ = reply.send(resp);
            })?;
        tracing::info!(?path, "IPC de janelas ativo");
    }

    event_loop.run(None, &mut data, |data| {
        #[cfg(feature = "bench")]
        data.state.check_frame_timeouts();
        let _ = data;
    })?;
    // Shutdown gracioso dos filhos do supervisor. Em morte abrupta (SIGKILL/
    // crash) o PDEATHSIG configurado no spawn garante o término pelo kernel.
    supervisor.shutdown();
    #[cfg(feature = "bench")]
    data.state.flush_frame_diagnostics();
    #[cfg(feature = "bench")]
    data.state.log_frame_summary();

    Ok(())
}

#[cfg(test)]
mod render_compartilhado {
    //! F1a: o render do frame deve viver em `render/elements.rs`, agnóstico de backend.
    #[test]
    fn frame_target_soma_insets_e_shadow() {
        let fgeo = crate::state::FrameGeometry {
            insets: (6, 30, 6, 30),
            shadow: (2, 2, 2, 2),
            ..Default::default()
        };
        let content = smithay::utils::Size::<i32, smithay::utils::Logical>::new(700, 500);
        let target = crate::render::elements::frame_target(content, &fgeo);
        // conteúdo + insets (12/60) + shadow (4/4) = 716x564.
        assert_eq!((target.w, target.h), (716, 564));
    }
}

#[cfg(test)]
mod protocol_tests {
    //! Testes de protocolo no mesmo processo: socketpair, sem display, env ou exec.
    use super::ClientState as ServerClientState;
    use super::*;
    use crate::cli::Options;
    use std::{
        os::unix::net::UnixStream,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
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
        server_client: wayland_server::Client,
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
            let server_client = display_handle
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
                server_client,
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

        fn associate(&mut self, window_id: u32) -> Option<String> {
            let manager = self.client.manager.clone().unwrap();
            let surface = self.client.surface.clone().unwrap();
            let frame = manager.associate(&surface, window_id, &self.queue.handle(), ());
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
        assert!(h.associate(0).is_none());
        assert_eq!(h.client.configured, 1);
        assert_eq!(h.client.closed, 0);
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn second_associate_of_window_is_already_constructed() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.associate(0).is_none());
        let compositor = h.client.compositor.clone().unwrap();
        let surface2 = compositor.create_surface(&h.queue.handle(), ());
        h.client.surface = Some(surface2);
        h.pump();
        let _err = h.associate(0);
        assert!(
            h.protocol_error,
            "segunda associação da janela deveria gerar erro de protocolo"
        );
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn second_associate_of_same_surface_is_already_constructed() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.associate(0).is_none());
        let _err = h.associate(1);
        assert!(
            h.protocol_error,
            "segunda associação da mesma superfície deveria gerar erro"
        );
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn associate_before_role_is_ok_after_role_is_error() {
        let mut h = Harness::new(true);
        h.create_surface();
        // Caminho feliz: associar ANTES de a superfície ganhar role.
        assert!(h.associate(0).is_none());
        assert_eq!(h.data.state.frames.len(), 1);
        assert_eq!(h.client.configured, 1);

        // Erro de role: associar uma superfície que já tem xdg_toplevel.
        let compositor = h.client.compositor.clone().unwrap();
        let surface2 = compositor.create_surface(&h.queue.handle(), ());
        h.client.surface = Some(surface2);
        h.pump();
        let shell = h.client.shell.clone().unwrap();
        let surface = h.client.surface.clone().unwrap();
        let xdg_surface = shell.get_xdg_surface(&surface, &h.queue.handle(), ());
        let _toplevel = xdg_surface.get_toplevel(&h.queue.handle(), ());
        h.pump();
        let _err = h.associate(1);
        assert!(
            h.protocol_error,
            "associate DEPOIS do role deveria ser erro de role"
        );
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn associate_accepts_layer_role_and_rejects_toplevel_popup_cursor() {
        use smithay::wayland::compositor::give_role;
        use smithay::wayland::seat::CURSOR_IMAGE_ROLE;
        use smithay::wayland::shell::wlr_layer::LAYER_SURFACE_ROLE;
        use smithay::wayland::shell::xdg::{XDG_POPUP_ROLE, XDG_TOPLEVEL_ROLE};
        use wayland_client::Proxy as _;

        // O sinal de aceitação/rejeição é frames.len()+configured: o evento
        // normal frame_requested também marca protocol_error no harness.
        // layer_surface: ACEITA (o frame é uma layer surface do shell).
        let mut h = Harness::new(true);
        h.create_surface();
        let server = h
            .server_client
            .object_from_protocol_id::<wl_surface::WlSurface>(
                &h.data.display_handle,
                h.client.surface.clone().unwrap().id().protocol_id(),
            )
            .unwrap();
        give_role(&server, LAYER_SURFACE_ROLE).unwrap();
        assert!(h.associate(0).is_none(), "layer_surface deveria ser ACEITO");
        assert_eq!(h.data.state.frames.len(), 1);
        assert_eq!(h.client.configured, 1);

        // toplevel / popup / cursor: REJEITA (superfície de cliente comum).
        for role in [XDG_TOPLEVEL_ROLE, XDG_POPUP_ROLE, CURSOR_IMAGE_ROLE] {
            let mut h = Harness::new(true);
            h.create_surface();
            let server = h
                .server_client
                .object_from_protocol_id::<wl_surface::WlSurface>(
                    &h.data.display_handle,
                    h.client.surface.clone().unwrap().id().protocol_id(),
                )
                .unwrap();
            give_role(&server, role).unwrap();
            let _ = h.associate(0);
            assert!(
                h.connection.protocol_error().is_some(),
                "role {role} deveria ser REJEITADO"
            );
            assert_eq!(h.data.state.frames.len(), 0);
            assert_eq!(h.client.configured, 0);
        }
    }

    #[test]
    fn unknown_window_associate_gets_closed() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.associate(999).is_none());
        assert_eq!(h.client.closed, 1, "janela inexistente deve gerar closed");
        assert_eq!(h.client.configured, 0);
        assert_eq!(h.data.state.frames.len(), 1);
    }

    #[test]
    fn canvas_tracks_insets_from_initial_associate() {
        let mut h = Harness::new(true);
        let output = smithay::output::Output::new(
            "test-02p".to_owned(),
            smithay::output::PhysicalProperties {
                size: (0, 0).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "AeroShell".to_owned(),
                model: "test".to_owned(),
            },
        );
        output.change_current_state(
            Some(smithay::output::Mode {
                size: (1280, 800).into(),
                refresh: 60_000,
            }),
            None,
            None,
            None,
        );
        h.data.state.outputs = vec![output];
        h.create_surface();
        assert!(h.associate(0).is_none());
        let frame = h.client.frames.first().unwrap().clone();
        let surface = h.data.state.associated_frames.first().unwrap().0.clone();
        assert_eq!(
            h.data.state.frame_canvases.get(&surface).unwrap().size,
            (1280, 800),
            "pré-condição: configure do associate ocorre antes dos insets"
        );

        frame.set_insets(6, 30, 6, 6);
        h.pump();
        assert_eq!(
            h.data.state.frame_canvases.get(&surface).unwrap().size,
            (1292, 836),
            "canvas do compositor deve coincidir com output + insets do shell"
        );
    }

    #[test]
    fn frame_painted_only_completes_matching_growth_serial() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.associate(0).is_none());
        let frame = h.client.frames.first().unwrap().clone();
        let surface = h.data.state.associated_frames.first().unwrap().0.clone();
        h.data.state.frame_canvases.insert(
            surface.clone(),
            crate::state::FrameCanvas {
                size: (100, 100),
                pending_growth: Some(crate::state::CanvasGrowth {
                    serial: 42,
                    size: (200, 220),
                }),
            },
        );

        frame.frame_painted(41);
        h.pump();
        let canvas = h.data.state.frame_canvases.get(&surface).unwrap();
        assert_eq!(canvas.size, (100, 100));
        assert!(canvas.pending_growth.is_some(), "serial antigo foi aceito");

        frame.frame_painted(42);
        h.pump();
        let canvas = h.data.state.frame_canvases.get(&surface).unwrap();
        assert_eq!(canvas.size, (200, 220));
        assert!(canvas.pending_growth.is_none());

        frame.frame_painted(42);
        h.pump();
        let canvas = h.data.state.frame_canvases.get(&surface).unwrap();
        assert_eq!(canvas.size, (200, 220));
        assert!(
            canvas.pending_growth.is_none(),
            "paint duplicado recriou growth"
        );
    }

    #[test]
    fn move_with_fake_serial_is_ignored() {
        let mut h = Harness::new(true);
        h.create_surface();
        assert!(h.associate(0).is_none());
        let seat = h.client.seat.clone().unwrap();
        let frame = h.client.frames.first().unwrap().clone();
        frame._move(&seat, 0xDEAD);
        h.pump();
        assert!(h.data.state.last_geometry_action.is_none());
    }

    #[test]
    fn move_grab_actually_moves_the_client_window() {
        let mut h = Harness::new(true);
        // Cliente real: toplevel xdg mapeado no space.
        h.create_surface();
        let shell = h.client.shell.clone().unwrap();
        let surface = h.client.surface.clone().unwrap();
        let xdg_surface = shell.get_xdg_surface(&surface, &h.queue.handle(), ());
        let _toplevel = xdg_surface.get_toplevel(&h.queue.handle(), ());
        h.pump();
        let window = h.data.state.space.elements().next().unwrap().clone();
        let initial = h.data.state.space.element_location(&window).unwrap();

        // Superfície do frame (sem role) associada à janela 0.
        let compositor = h.client.compositor.clone().unwrap();
        let frame_surface = compositor.create_surface(&h.queue.handle(), ());
        h.client.surface = Some(frame_surface);
        h.pump();
        assert!(h.associate(0).is_none());

        // Clique real -> serial válido; move do frame inicia o grab.
        h.data.state.inject_button(true);
        let serial = h.data.state.last_button_serial.unwrap();
        let seat = h.client.seat.clone().unwrap();
        let frame = h.client.frames.last().unwrap().clone();
        frame._move(&seat, serial);
        h.pump();
        assert!(
            h.data.state.seat.get_pointer().unwrap().is_grabbed(),
            "grab de move deveria estar ativo"
        );

        // Motion durante o grab move a janela cliente.
        h.data.state.inject_motion(300.0, 300.0);
        h.pump();
        let moved = h.data.state.space.element_location(&window).unwrap();
        assert_ne!(moved, initial, "janela deveria ter movido com o grab");

        // Release encerra o grab.
        h.data.state.inject_button(false);
        h.pump();
        assert!(!h.data.state.seat.get_pointer().unwrap().is_grabbed());
    }
}
