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
        pub use wayland_client::protocol::{wl_pointer, wl_seat, wl_surface};
        wayland_scanner::generate_interfaces!("../../protocols/aero_frame_v1.xml");
        wayland_scanner::generate_client_code!("../../protocols/aero_frame_v1.xml");
    }
    pub use generated::*;
}

use aero::aero_frame_manager_v1::AeroFrameManagerV1;
use aero::aero_frame_v1::{AeroFrameV1, Event as FrameEvent, ResizeEdge};
use aero::{wl_pointer, wl_seat};

/// Um frame associado: proxy aero + superfície + janela GTK + geometria declarada.
struct FrameEntry {
    frame: AeroFrameV1,
    wl_surface: aero::wl_surface::WlSurface,
    window: gtk4::ApplicationWindow,
    insets: (i32, i32, i32, i32),
    shadow: (i32, i32, i32, i32),
    total: (i32, i32),
}

#[derive(Default)]
struct SpikeState {
    manager: Option<AeroFrameManagerV1>,
    configures: usize,
    closed: usize,
    /// Frames associados (um por janela cliente). O tamanho de cada janela GTK
    /// é dirigido pelo aero_frame.configure do seu próprio frame.
    entries: Vec<FrameEntry>,
    /// Seat/pointer do compositor para enviar move/resize com serial real.
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    /// Índice do frame cuja superfície recebeu o último enter do ponteiro.
    active: usize,
    /// Última posição local do ponteiro na superfície do frame ativo.
    last_local: (f64, f64),
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
            if interface == "wl_seat" {
                state.seat = Some(registry.bind::<wl_seat::WlSeat, (), SpikeState>(
                    name,
                    version.min(7),
                    qh,
                    (),
                ));
                eprintln!("SPIKE: wl_seat global={name} vinculado");
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for SpikeState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            if let Ok(capabilities) = capabilities.into_result() {
                if capabilities.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                    state.pointer = Some(seat.get_pointer(qh, ()));
                    eprintln!("SPIKE: wl_pointer obtido do seat");
                }
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for SpikeState {
    fn event(
        state: &mut Self,
        _pointer: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                if let Some(i) = state.entries.iter().position(|e| e.wl_surface == surface) {
                    state.active = i;
                }
                state.last_local = (surface_x, surface_y);
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.last_local = (surface_x, surface_y);
            }
            wl_pointer::Event::Button {
                serial, state: btn, ..
            } => {
                if !matches!(btn.into_result(), Ok(wl_pointer::ButtonState::Pressed)) {
                    return;
                }
                let (x, y) = state.last_local;
                let Some(entry) = state.entries.get(state.active) else {
                    return;
                };
                let (tw, _th) = entry.total;
                let frame = entry.frame.clone();
                let Some(seat) = state.seat.clone() else {
                    return;
                };
                // Titlebar (faixa superior de 30 px): esquerda -> move; últimos
                // 16 px à direita -> resize. Com insets só no topo, o canto
                // inferior direito do frame cai DENTRO do conteúdo do cliente
                // (que roteia o input primeiro), então a alça de resize vive na
                // titlebar. Serial real deste clique.
                if y < 30.0 {
                    if x > (tw - 16) as f64 {
                        eprintln!("SPIKE: clique na alça de resize ({x:.0},{y:.0}) -> resize serial={serial}");
                        frame.resize(&seat, serial, ResizeEdge::BottomRight);
                    } else {
                        eprintln!(
                            "SPIKE: clique na titlebar ({x:.0},{y:.0}) -> move serial={serial}"
                        );
                        frame._move(&seat, serial);
                    }
                }
            }
            _ => {}
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
                let Some(entry) = state.entries.iter_mut().find(|e| &e.frame == frame) else {
                    return;
                };
                let (l, t, r, b) = entry.insets;
                let (sl, st, sr, sb) = entry.shadow;
                let total_w = width + l + r + sl + sr;
                let total_h = height + t + b + st + sb;
                entry.total = (total_w, total_h);
                eprintln!(
                    "SPIKE: configure serial={serial} conteudo={width}x{height} -> set_default_size {total_w}x{total_h}"
                );
                entry.window.set_default_size(total_w, total_h);
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

/// Associa a superfície de uma janela GTK a um window_id no MESMO guest/queue.
/// Pressupõe RUNTIME já inicializado (manager vinculado). Titlebar de 30 px.
fn associate_frame(
    wl_surface: aero::wl_surface::WlSurface,
    window: gtk4::ApplicationWindow,
    window_id: u32,
) {
    RUNTIME.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let rt = borrow.as_mut().expect("RUNTIME não inicializado");
        let qh = rt.queue.handle();
        let manager = rt.state.manager.clone().expect("manager ausente");
        let frame = manager.associate(&wl_surface, window_id, &qh, ());
        frame.set_insets(0, 30, 0, 0);
        let _ = rt.queue.flush();
        rt.state.entries.push(FrameEntry {
            frame,
            wl_surface,
            window,
            insets: (0, 30, 0, 0),
            shadow: (0, 0, 0, 0),
            total: (0, 0),
        });
        eprintln!("SPIKE: associate(window_id={window_id}) + set_insets(0,30,0,0) ENVIADOS");
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
            // AERO_SPIKE_BLUE_N>1 abre várias janelas no mesmo cliente (sanidade
            // de 2 janelas empilhadas, cada uma com seu frame).
            let n: usize = std::env::var("AERO_SPIKE_BLUE_N")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            for i in 0..n {
                let client = gtk4::ApplicationWindow::builder()
                    .application(app)
                    .default_width(320)
                    .default_height(240)
                    .title(format!("cliente azul {i}"))
                    .build();
                client.present();
                eprintln!("SPIKE: cliente AZUL {i} apresentado (xdg_toplevel, sem layer-shell)");
                // Gate de resize: o cliente muda de tamanho sozinho para provar que
                // o frame SEGUE o conteúdo (AERO_SPIKE_RESIZE=WxH).
                if let Ok(size) = std::env::var("AERO_SPIKE_RESIZE") {
                    if let Some((w, h)) = size.split_once('x') {
                        if let (Ok(w), Ok(h)) = (w.parse::<i32>(), h.parse::<i32>()) {
                            gtk4::glib::timeout_add_seconds_local_once(5, move || {
                                eprintln!("SPIKE: cliente AZUL {i} resize -> {w}x{h}");
                                client.set_default_size(w, h);
                            });
                        }
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
            let mut state = SpikeState::default();

            // Startup: deixar o GLib bombear o read e só então esvaziar a fila.
            for _ in 0..30 {
                gtk4::glib::MainContext::default().iteration(false);
                let _ = queue.dispatch_pending(&mut state);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            eprintln!("SPIKE: manager no guest = {}", state.manager.is_some());
            if state.manager.is_some() {
                RUNTIME.with(|cell| {
                    *cell.borrow_mut() = Some(Runtime { queue, state });
                });
                associate_frame(wl_surface, w.clone(), 0);

                // Sanidade de 2 janelas: um segundo frame para window_id=1,
                // criado só depois que o RUNTIME (conexão/manager) existe.
                if std::env::var_os("AERO_SPIKE_FRAME2").is_some() {
                    let w0 = w.clone();
                    gtk4::glib::idle_add_local_once(move || {
                        let app = w0.application().expect("janela sem application");
                        let win1 = gtk4::ApplicationWindow::builder()
                            .application(&app)
                            .title("frame spike 2")
                            .build();
                        win1.init_layer_shell();
                        win1.set_namespace(Some("aeroshell-frame-spike-2"));
                        win1.connect_realize(|w1| {
                            let Some(s1) = w1.surface() else { return };
                            let Some(ws1) = s1.downcast_ref::<WaylandSurface>() else {
                                return;
                            };
                            let Some(wl1) = ws1.wl_surface() else { return };
                            associate_frame(wl1, w1.clone(), 1);
                        });
                        win1.present();
                        eprintln!("SPIKE: segundo frame (window_id=1) apresentado");
                    });
                }
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
                    rt.state.entries.clear();
                    eprintln!(
                        "SPIKE: frames destruídos; configures={} closed={}",
                        rt.state.configures, rt.state.closed
                    );
                }
            });
            app_quit.quit();
        });
    });
    app.run();
}
