//! Spike do frame REAL em WebView (gtk4-layer-shell + WebKitGTK + associate).
//! Barra de título em HTML (fechar/maximizar) sobre a janela cliente; arrastar
//! a titlebar move e a alça à direita redimensiona (grabs do compositor). Os
//! botões fechar/maximizar falam com o IPC de janelas do compositor (AERO_WINDOW_IPC_SOCK).
//!
//! Posse da conexão: igual ao frame_spike — derivamos o Connection do backend
//! do wl_surface do GTK (mesmo guest), sem roundtrip bloqueante.

use gdk4_wayland::prelude::*;
use gdk4_wayland::{wayland_client, WaylandSurface};
use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;
use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::os::unix::net::UnixStream;
use wayland_client::{protocol::wl_registry, Dispatch, Proxy, QueueHandle};
use webkit6::prelude::*;
use webkit6::{UserContentManager, WebView};

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

const TITLEBAR_H: i32 = 30;
/// Largura da borda de resize em torno do conteúdo (esquerda/direita/baixo).
const BORDER: i32 = 6;
/// Largura reservada à direita da titlebar para botões (não dispara move).
const RIGHT_CONTROLS: i32 = 96;

struct FrameEntry {
    frame: AeroFrameV1,
    wl_surface: aero::wl_surface::WlSurface,
    window: gtk4::ApplicationWindow,
    insets: (i32, i32, i32, i32),
    total: (i32, i32),
    window_id: u32,
}

#[derive(Default)]
struct SpikeState {
    manager: Option<AeroFrameManagerV1>,
    configures: usize,
    closed: usize,
    entries: Vec<FrameEntry>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    active: usize,
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
                eprintln!("FRAMEWEB: manager global={name} vinculado");
            }
            if interface == "wl_seat" {
                state.seat = Some(registry.bind::<wl_seat::WlSeat, (), SpikeState>(
                    name,
                    version.min(7),
                    qh,
                    (),
                ));
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
                    eprintln!("FRAMEWEB: button sem entry ativo (active={})", state.active);
                    return;
                };
                let (tw, th) = entry.total;
                eprintln!("FRAMEWEB: button local=({x:.0},{y:.0}) total={tw}x{th} serial={serial}");
                let frame = entry.frame.clone();
                let Some(seat) = state.seat.clone() else {
                    return;
                };
                // Bordas/cantos (faixa de BORDER px) -> resize no edge correspondente.
                let b = BORDER as f64;
                let on_left = x < b;
                let on_right = x >= (tw - BORDER) as f64;
                let on_top = y < b;
                let on_bottom = y >= (th - BORDER) as f64;
                let edge = match (on_top, on_bottom, on_left, on_right) {
                    (true, false, true, false) => Some(ResizeEdge::TopLeft),
                    (true, false, false, true) => Some(ResizeEdge::TopRight),
                    (false, true, true, false) => Some(ResizeEdge::BottomLeft),
                    (false, true, false, true) => Some(ResizeEdge::BottomRight),
                    (true, false, false, false) => Some(ResizeEdge::Top),
                    (false, true, false, false) => Some(ResizeEdge::Bottom),
                    (false, false, true, false) => Some(ResizeEdge::Left),
                    (false, false, false, true) => Some(ResizeEdge::Right),
                    _ => None,
                };
                if let Some(edge) = edge {
                    eprintln!("FRAMEWEB: borda/canto -> resize {edge:?} serial={serial}");
                    frame.resize(&seat, serial, edge);
                } else if y < TITLEBAR_H as f64 && x < (tw - RIGHT_CONTROLS) as f64 {
                    eprintln!("FRAMEWEB: titlebar -> move serial={serial}");
                    frame._move(&seat, serial);
                }
                // zona dos botões (tw-RIGHT_CONTROLS..tw): o JS da WebView trata.
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
                let Some(entry) = state.entries.iter_mut().find(|e| &e.frame == frame) else {
                    return;
                };
                let (l, t, r, b) = entry.insets;
                let total_w = width + l + r;
                let total_h = height + t + b;
                entry.total = (total_w, total_h);
                eprintln!(
                    "FRAMEWEB: configure serial={serial} conteudo={width}x{height} -> {total_w}x{total_h}"
                );
                entry.window.set_default_size(total_w, total_h);
                frame.ack_configure(serial);
            }
            FrameEvent::Closed => {
                state.closed += 1;
                eprintln!("FRAMEWEB: closed recebido");
            }
        }
    }
}

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
            let _ = rt.queue.dispatch_pending(&mut rt.state);
            let _ = rt.queue.flush();
        }
    });
}

/// Envia um comando ao IPC de janelas do compositor e loga a resposta.
fn window_ipc(cmd: &str) {
    let Some(path) = std::env::var_os("AERO_WINDOW_IPC_SOCK") else {
        eprintln!("FRAMEWEB: AERO_WINDOW_IPC_SOCK ausente; comando '{cmd}' ignorado");
        return;
    };
    match UnixStream::connect(&path) {
        Ok(mut stream) => {
            let _ = writeln!(stream, "{cmd}");
            let _ = stream.flush();
            let mut line = String::new();
            let _ = std::io::BufReader::new(stream).read_line(&mut line);
            eprintln!("FRAMEWEB: IPC '{cmd}' -> {}", line.trim());
        }
        Err(err) => eprintln!("FRAMEWEB: IPC '{cmd}' falhou: {err}"),
    }
}

const TITLEBAR_HTML: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
  html,body{margin:0;height:100%;background:transparent;overflow:hidden;
    font-family:sans-serif;user-select:none;}
  #border{position:absolute;inset:0;box-sizing:border-box;pointer-events:none;
    border:6px solid #2b2b2b;border-top:0;}
  #bar{position:absolute;top:0;left:0;right:0;height:30px;display:flex;
    align-items:center;background:#2b2b2b;color:#eee;}
  #title{flex:1;padding:0 8px;font-size:13px;white-space:nowrap;overflow:hidden;}
  button{width:30px;height:30px;border:0;background:transparent;color:#eee;
    font-size:14px;cursor:pointer;}
  button:hover{background:#444;}
  #close:hover{background:#c0392b;}
</style></head>
<body>
  <div id="border"></div>
  <div id="bar" data-aero-interactive>
    <span id="title">janela</span>
    <button id="max" title="maximizar">&#9633;</button>
    <button id="close" title="fechar">&#10005;</button>
  </div>
  <script>
    const send = (action) => window.webkit.messageHandlers.aero.postMessage({ action });
    document.getElementById('close').addEventListener('click', () => send('close'));
    document.getElementById('max').addEventListener('click', () => send('maximize'));
  </script>
</body></html>"#;

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
        frame.set_insets(BORDER, TITLEBAR_H, BORDER, BORDER);
        let _ = rt.queue.flush();
        rt.state.entries.push(FrameEntry {
            frame,
            wl_surface,
            window,
            insets: (BORDER, TITLEBAR_H, BORDER, BORDER),
            total: (0, 0),
            window_id,
        });
        eprintln!("FRAMEWEB: associate(window_id={window_id}) + set_insets({BORDER},{TITLEBAR_H},{BORDER},{BORDER})");
    });
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.aeroshell.frame-webview")
        .build();
    app.connect_activate(move |app| {
        // Fundo do frame: sem fundo de tema, a área recém-exposta durante resize
        // mostra o que está atrás em vez de branco (a decoração é pintada só pelo HTML).
        let css = gtk4::CssProvider::new();
        css.load_from_string(".aero-frame { background-color: rgba(0, 0, 0, 0); }");
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("display GTK ausente"),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("frame webview")
            .build();
        window.add_css_class("aero-frame");
        window.init_layer_shell();
        window.set_namespace(Some("aeroshell-frame-webview"));
        // Teclado NUNCA no frame: interatividade none (só ponteiro na decoração).
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

        // WebView transparente com a barra de título HTML.
        let manager = UserContentManager::new();
        manager.register_script_message_handler("aero", None);
        manager.connect_script_message_received(Some("aero"), move |_, value| {
            let json = value.to_json(0).unwrap_or_default().to_string();
            let action = serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .and_then(|v| v.get("action").and_then(|a| a.as_str()).map(str::to_owned));
            let window_id = RUNTIME.with(|cell| {
                cell.borrow()
                    .as_ref()
                    .and_then(|rt| rt.state.entries.first().map(|e| e.window_id))
                    .unwrap_or(0)
            });
            match action.as_deref() {
                Some("close") => window_ipc(&format!("close {window_id}")),
                Some("maximize") => window_ipc(&format!("maximize {window_id}")),
                _ => eprintln!("FRAMEWEB: mensagem da WebView: {json}"),
            }
        });
        let webview = WebView::builder().user_content_manager(&manager).build();
        webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        webview.load_html(TITLEBAR_HTML, Some("aero://frame/"));
        window.set_child(Some(&webview));

        window.connect_realize(|w| {
            let gdk_surface = w.surface().expect("janela sem GdkSurface");
            let ws = gdk_surface
                .downcast_ref::<WaylandSurface>()
                .expect("GdkSurface não é WaylandSurface");
            let Some(wl_surface) = ws.wl_surface() else {
                eprintln!("FRAMEWEB: wl_surface=None no realize");
                return;
            };
            let backend = wl_surface.backend().upgrade().expect("backend sumiu");
            let connection = wayland_client::Connection::from_backend(backend);
            let mut queue: wayland_client::EventQueue<SpikeState> = connection.new_event_queue();
            let qh = queue.handle();
            connection.display().get_registry(&qh, ());
            let mut state = SpikeState::default();
            for _ in 0..30 {
                gtk4::glib::MainContext::default().iteration(false);
                let _ = queue.dispatch_pending(&mut state);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            eprintln!("FRAMEWEB: manager no guest = {}", state.manager.is_some());
            if state.manager.is_some() {
                RUNTIME.with(|cell| {
                    *cell.borrow_mut() = Some(Runtime { queue, state });
                });
                associate_frame(wl_surface, w.clone(), 0);
            }
        });

        window.connect_map(|_| eprintln!("FRAMEWEB map: layer surface criada"));
        window.present();

        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(20), || {
            dispatch_nonblocking();
            gtk4::glib::ControlFlow::Continue
        });
    });
    app.run();
}
