//! Spike B.3 (sem WebKit, sem Bun): gtk::Window + gtk4-layer-shell + `associate`.
//! Sequência: realize → wl_surface() → associate → present/map → configure/ack.
//!
//! MODELO DE POSSE (B.3.2): o GTK/GDK é dono do read/dispatch da conexão. Aqui
//! derivamos um `Connection` do backend JÁ EXISTENTE do `wl_surface` (guest
//! singleton de `gdk4-wayland`), nunca criamos outro guest. Nunca chamamos
//! `roundtrip`/`blocking_dispatch`: sincronizamos no startup com
//! `MainContext::iteration` + `dispatch_pending` não bloqueante, e fazemos
//! `flush()` após cada envio. Os objetos aero são destruídos ANTES do drop da
//! connection (guest descarta eventos após drop — doc de `from_foreign_display`).

use gdk4_wayland::prelude::*;
use gdk4_wayland::{wayland_client, WaylandSurface};
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use wayland_client::{protocol::wl_registry, Dispatch, Proxy, QueueHandle};

mod aero {
    mod core_interfaces {
        wayland_scanner::generate_interfaces!("/usr/share/wayland/wayland.xml");
    }
    mod generated {
        #[allow(unused_imports)]
        use super::core_interfaces::*;
        #[allow(clippy::single_component_path_imports)]
        use crate::wayland_client;
        #[allow(clippy::single_component_path_imports)]
        use wayland_backend;
        pub use wayland_client::protocol::{wl_seat, wl_surface};
        wayland_scanner::generate_interfaces!("../../protocols/aero_frame_v1.xml");
        wayland_scanner::generate_client_code!("../../protocols/aero_frame_v1.xml");
    }
    pub use generated::*;
}

use aero::aero_frame_manager_v1::AeroFrameManagerV1;
use aero::aero_frame_v1::{AeroFrameV1, Event as FrameEvent};

#[derive(Default)]
struct SpikeState {
    manager: Option<AeroFrameManagerV1>,
    configures: usize,
    closed: usize,
}

impl Dispatch<wl_registry::WlRegistry, ()> for SpikeState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if interface == "aero_frame_manager_v1" {
                state.manager = Some(registry.bind::<AeroFrameManagerV1, (), SpikeState>(
                    name,
                    version.min(1),
                    qh,
                    (),
                ));
                eprintln!("SPIKE: manager global={name} vinculado no guest do GTK");
            }
        }
    }
}

impl Dispatch<AeroFrameManagerV1, ()> for SpikeState {
    fn event(
        _state: &mut Self,
        _proxy: &AeroFrameManagerV1,
        _event: aero::aero_frame_manager_v1::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<AeroFrameV1, ()> for SpikeState {
    fn event(
        state: &mut Self,
        _frame: &AeroFrameV1,
        event: FrameEvent,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            FrameEvent::Configure {
                serial,
                width,
                height,
                ..
            } => {
                state.configures += 1;
                eprintln!("SPIKE: configure serial={serial} {width}x{height}");
            }
            FrameEvent::Closed => {
                state.closed += 1;
                eprintln!("SPIKE: closed recebido");
            }
        }
    }
}

// Estado partilhado com o callback de timeout (a main loop do GTK é quem bombeia).
thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

struct Runtime {
    queue: wayland_client::EventQueue<SpikeState>,
    state: SpikeState,
    frame: Option<AeroFrameV1>,
}

fn dispatch_nonblocking() {
    RUNTIME.with(|cell| {
        if let Some(rt) = cell.borrow_mut().as_mut() {
            // NÃO bloquear: GTK é dono do read. Apenas esvazia o que já chegou.
            let _ = rt.queue.dispatch_pending(&mut rt.state);
            let _ = rt.queue.flush();
        }
    });
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.aeroshell.frame-spike")
        .build();
    app.connect_activate(|app| {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .default_width(320)
            .default_height(200)
            .title("frame spike — associate")
            .build();

        // init ANTES de realize (gtk4-layer-shell lib.rs:170-172).
        window.init_layer_shell();
        window.set_namespace(Some("aeroshell-frame-spike"));

        window.connect_realize(|w| {
            let gdk_surface = w.surface().expect("janela sem GdkSurface");
            let ws = gdk_surface
                .downcast_ref::<WaylandSurface>()
                .expect("GdkSurface não é WaylandSurface");
            let Some(wl_surface) = ws.wl_surface() else {
                eprintln!("BLOQUEIO B.3.1: wl_surface=None no realize");
                return;
            };
            eprintln!("SPIKE realize: wl_surface=Some (proxy no guest do GTK)");

            // Derivar o Connection do backend do próprio wl_surface: MESMO guest.
            let backend = wl_surface
                .backend()
                .upgrade()
                .expect("backend do GTK sumiu");
            let connection = wayland_client::Connection::from_backend(backend);
            let mut queue: wayland_client::EventQueue<SpikeState> = connection.new_event_queue();
            let qh = queue.handle();
            connection.display().get_registry(&qh, ());
            let mut state = SpikeState::default();

            // Startup: deixar o GLib bombear o read e só então esvaziar a fila.
            for _ in 0..30 {
                gtk4::glib::MainContext::default().iteration(false);
                let _ = queue.dispatch_pending(&mut state);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            eprintln!("SPIKE: manager no guest = {}", state.manager.is_some());
            if let Some(manager) = &state.manager {
                let frame = manager.associate(&wl_surface, 0, &qh, ());
                let _ = queue.flush();
                eprintln!("SPIKE: associate(window_id=0) ENVIADO no realize");
                RUNTIME.with(|cell| {
                    *cell.borrow_mut() = Some(Runtime {
                        queue,
                        state,
                        frame: Some(frame),
                    });
                });
            }
        });

        window.connect_map(|_| eprintln!("SPIKE map: layer surface (role) criada"));
        window.present();
        eprintln!("SPIKE present chamado");

        // Bombeia o queue do guest sem bloquear, pela main loop do GTK.
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(20), || {
            dispatch_nonblocking();
            gtk4::glib::ControlFlow::Continue
        });

        if std::env::var_os("AERO_SPIKE_STAY").is_some() {
            eprintln!("SPIKE: modo manual (AERO_SPIKE_STAY) — não encerrar sozinho");
            return;
        }
        let app_quit = app.clone();
        gtk4::glib::timeout_add_seconds_local_once(6, move || {
            // Destruir objetos aero ANTES de dropar a connection guest.
            RUNTIME.with(|cell| {
                if let Some(rt) = cell.borrow_mut().as_mut() {
                    rt.frame.take();
                    eprintln!(
                        "SPIKE: frame destruído; configures={} closed={}",
                        rt.state.configures, rt.state.closed
                    );
                }
            });
            app_quit.quit();
        });
    });
    app.run();
}
