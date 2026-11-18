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
    /// Janela GTK do frame: o tamanho é dirigido pelo aero_frame.configure.
    window: Option<gtk4::ApplicationWindow>,
    /// insets (left, top, right, bottom) que este shell declarou via set_insets.
    insets: (i32, i32, i32, i32),
    /// shadow_extents (left, top, right, bottom) declarados via set_shadow_extents.
    shadow: (i32, i32, i32, i32),
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
        frame: &AeroFrameV1,
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
                // O shell governa o tamanho: total = conteúdo + insets + shadow.
                // NÃO dependemos de o GTK honrar o zwlr_layer_surface.configure
                // (a lib C só o aplica em eixos esticados; ver layer-surface.c
                // v1.3.0). set_default_size dispara notify::default-width/height
                // → a lib reconfigura a superfície com o tamanho preferido.
                let (l, t, r, b) = state.insets;
                let (sl, st, sr, sb) = state.shadow;
                let total_w = width + l + r + sl + sr;
                let total_h = height + t + b + st + sb;
                eprintln!(
                    "SPIKE: configure serial={serial} conteudo={width}x{height} -> set_default_size {total_w}x{total_h}"
                );
                if let Some(window) = &state.window {
                    window.set_default_size(total_w, total_h);
                }
                frame.ack_configure(serial);
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
    let blue_client = std::env::var_os("AERO_SPIKE_BLUE_CLIENT").is_some();
    let app_id = if blue_client {
        "dev.aeroshell.blue-client"
    } else {
        "dev.aeroshell.frame-spike"
    };
    let app = gtk4::Application::builder().application_id(app_id).build();
    app.connect_activate(move |app| {
        // Teste controlado: frame VERMELHO / cliente AZUL para medir por pixel.
        let provider = gtk4::CssProvider::new();
        let css = if blue_client {
            "window { background-color: #0000ff; }"
        } else {
            "window { background-color: #ff0000; }"
        };
        provider.load_from_string(css);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        if blue_client {
            let client = gtk4::ApplicationWindow::builder()
                .application(app)
                .default_width(320)
                .default_height(240)
                .title("cliente azul")
                .build();
            client.present();
            eprintln!("SPIKE: cliente AZUL apresentado (xdg_toplevel, sem layer-shell)");
            // Gate de resize: o cliente muda de tamanho sozinho para provar que
            // o frame SEGUE o conteúdo (AERO_SPIKE_RESIZE=WxH).
            if let Ok(size) = std::env::var("AERO_SPIKE_RESIZE") {
                if let Some((w, h)) = size.split_once('x') {
                    if let (Ok(w), Ok(h)) = (w.parse::<i32>(), h.parse::<i32>()) {
                        gtk4::glib::timeout_add_seconds_local_once(5, move || {
                            eprintln!("SPIKE: cliente AZUL resize -> {w}x{h}");
                            client.set_default_size(w, h);
                        });
                    }
                }
            }
            return;
        }

        // Sem tamanho fixo: a janela é dimensionada pelo aero_frame.configure
        // (total = conteúdo + insets + shadow). Um default fixo aqui era a causa
        // do buffer 320x200 ignorar o configure 320x270.
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
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
            let mut state = SpikeState {
                window: Some(w.clone()),
                insets: (0, 30, 0, 0),
                ..Default::default()
            };

            // Startup: deixar o GLib bombear o read e só então esvaziar a fila.
            for _ in 0..30 {
                gtk4::glib::MainContext::default().iteration(false);
                let _ = queue.dispatch_pending(&mut state);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            eprintln!("SPIKE: manager no guest = {}", state.manager.is_some());
            if let Some(manager) = &state.manager {
                let frame = manager.associate(&wl_surface, 0, &qh, ());
                // Titlebar de 30 px: o frame deve aparecer SÓ nessa faixa.
                frame.set_insets(0, 30, 0, 0);
                let _ = queue.flush();
                eprintln!("SPIKE: associate(window_id=0) + set_insets(0,30,0,0) ENVIADOS");
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
