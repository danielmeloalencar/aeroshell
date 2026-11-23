//! Spike do frame REAL em WebView (gtk4-layer-shell + WebKitGTK + associate).
//! Barra de título em HTML (fechar/maximizar) sobre a janela cliente; arrastar
//! a titlebar move e a alça à direita redimensiona (grabs do compositor). Os
//! botões fechar/maximizar falam com o IPC de janelas do compositor (AERO_WINDOW_IPC_SOCK).
//!
//! Posse da conexão: igual ao frame_spike — derivamos o Connection do backend
//! do wl_surface do GTK (mesmo guest), sem roundtrip bloqueante.

use gdk4_wayland::prelude::*;
use gdk4_wayland::{wayland_client, WaylandSurface};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
use gtk4_layer_shell::LayerShell;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::rc::Rc;
use std::time::{Duration, Instant};
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
/// 02l: largura fixa da zona do título, declarada ao compositor.
const TITLE_TEXT_W: i32 = 120;
const POOL_TARGET: usize = 2;

/// Relógio de parede compartilhado com o `t=` das sondas do compositor.
/// Instrumentação permanente de startup (os logs FRAMEWEB já existem); nenhuma
/// decisão de runtime depende destes números.
fn bench_t() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

struct FrameEntry {
    frame: AeroFrameV1,
    wl_surface: aero::wl_surface::WlSurface,
    window: gtk4::ApplicationWindow,
    webview: WebView,
    insets: (i32, i32, i32, i32),
    /// Target apresentado (conteúdo + insets); não dimensiona a WebView.
    total: (i32, i32),
    /// Tamanho real do buffer WebView; só muda em crescimento excepcional.
    canvas: (i32, i32),
    presented: bool,
    window_id: u32,
    /// Último título aplicado ao HTML; evita reenviar JS a cada poll do IPC.
    last_title: Option<String>,
    /// Assinatura (base64) do último ícone aplicado; ícone ausente = None.
    last_icon: Option<String>,
    /// Já escondemos a barra HTML deste frame por CSD (pedido explícito ou
    /// detectado depois pela window_geometry, como no gedit real).
    csd_active: bool,
    /// Até quando vale a pena consultar `csd <id>` a cada 100 ms nesta janela.
    csd_probe_until: Instant,
}

struct PoolItem {
    id: usize,
    window: gtk4::ApplicationWindow,
    webview: WebView,
    window_id: Rc<Cell<Option<u32>>>,
    ready: bool,
}

#[derive(Default)]
struct SpikeState {
    manager: Option<AeroFrameManagerV1>,
    configures: usize,
    closed: usize,
    entries: Vec<FrameEntry>,
    /// F1: entradas aposentadas no `Closed`, fora do mapa ativo. Mantidas vivas
    /// para não disparar o `invalid_size` do layer-surface no Smithay 0.7.
    retired: Vec<FrameEntry>,
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
                let manager = registry.bind::<AeroFrameManagerV1, (), SpikeState>(
                    name,
                    version.min(1),
                    qh,
                    (),
                );
                // Primeira classe: o compositor precisa da geometria da decoração
                // ANTES de qualquer associate, para desenhar o placeholder de uma
                // janela que mapeou enquanto o WebView ainda não existe.
                manager.set_default_insets(BORDER, TITLEBAR_H, BORDER, BORDER);
                eprintln!(
                    "FRAMEWEB: manager global={name} vinculado; default_insets=({BORDER},{TITLEBAR_H},{BORDER},{BORDER})"
                );
                state.manager = Some(manager);
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
                let (tw, th) = entry.canvas;
                eprintln!("FRAMEWEB: button local=({x:.0},{y:.0}) canvas={tw}x{th} target={}x{} serial={serial}", entry.total.0, entry.total.1);
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
                // zona dos botões (canvas_w-RIGHT_CONTROLS..canvas_w): o JS trata.
            }
            _ => {}
        }
    }
}

impl Dispatch<AeroFrameManagerV1, ()> for SpikeState {
    fn event(
        state: &mut Self,
        _proxy: &AeroFrameManagerV1,
        event: aero::aero_frame_manager_v1::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let aero::aero_frame_manager_v1::Event::FrameRequested {
            window_id,
            width,
            height,
            ..
        } = event;
        eprintln!("FRAMEWEB: frame_requested window_id={window_id} conteudo={width}x{height}");
        // O associate acontece no realize, sem apresentar um buffer 200x200.
        // O frame_requested enviado pelo próprio associate traz o output e só
        // então fixa o canvas inicial e apresenta a janela (sem deadlock 02m).
        if let Some(entry) = state
            .entries
            .iter_mut()
            .find(|entry| entry.window_id == window_id)
        {
            if !entry.presented && width > 0 && height > 0 {
                let (l, t, r, b) = entry.insets;
                entry.canvas = (width + l + r, height + t + b);
                entry
                    .window
                    .set_default_size(entry.canvas.0, entry.canvas.1);
                entry.presented = true;
                eprintln!(
                    "FRAMEWEB: canvas inicial window_id={window_id} {}x{}",
                    entry.canvas.0, entry.canvas.1
                );
                entry.window.present();
            } else {
                eprintln!("FRAMEWEB: window_id={window_id} já apresentado; ignorado");
            }
            return;
        }
        let initial_canvas = state
            .entries
            .first()
            .map(|entry| entry.canvas)
            .unwrap_or((0, 0));
        // Cria na próxima idle: este handler roda dentro do borrow do RUNTIME
        // (dispatch) e não pode reassumir o RUNTIME para associar a superfície.
        glib::idle_add_local(move || {
            let client_frame = ipc_csd(window_id);
            request_frame(window_id, initial_canvas.0, initial_canvas.1, client_frame);
            glib::ControlFlow::Break
        });
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
                if !state.entries.iter().any(|e| &e.frame == frame) {
                    // O configure pode chegar antes do associate terminar:
                    // guardamos e aplicamos assim que a entry existe.
                    PENDING_CONFIGURES.with(|cell| {
                        cell.borrow_mut()
                            .push((frame.clone(), serial, width, height));
                    });
                    return;
                }
                apply_configure(state, frame, serial, width, height);
            }
            FrameEvent::Closed => {
                state.closed += 1;
                // Correção F1: aposentar a entrada e a janela GTK. Sem isso o
                // frame_requested do novo foot (com id novo) encontrava a WebView
                // antiga e caía em "já apresentado; ignorado".
                let Some(pos) = state.entries.iter().position(|entry| entry.frame == *frame) else {
                    eprintln!("FRAMEWEB: closed para frame desconhecido");
                    return;
                };
                let entry = state.entries.remove(pos);
                let window_id = entry.window_id;
                // Destruir o proxy do frame ANTES da janela GTK: se a janela for
                // fechada primeiro, o layer-surface é derrubado com largura 0 e
                // sem âncoras, e o compositor responde protocol error matando o
                // shell (medido no baseline: zwlr_layer_surface_v1 code 1).
                // TODO(F1): tanto `frame.destroy()` quanto `window.close()`/
                // `set_visible(false)` derrubam o layer-surface com `set_size(0,0)`
                // e âncoras zeradas no mesmo commit; o Smithay 0.7 responde
                // `invalid_size` e mata o shell (medido em 3 variantes). Aqui só
                // aposentamos a entrada lógica — o compositor já enviou `closed`
                // e não reutiliza mais o id. Destruição segura fica em §6.
                state.retired.push(entry);
                PENDING_FRAMES.with(|cell| {
                    cell.borrow_mut().retain(|(id, _, _, _)| *id != window_id);
                });
                eprintln!(
                    "FRAMEWEB: closed recebido window_id={window_id} entradas_restantes={}",
                    state.entries.len()
                );
            }
        }
    }
}

/// Aplica um configure a uma entry já existente. Também usado para configures
/// que chegaram antes do associate (guardados em PENDING_CONFIGURES).
fn apply_configure(
    state: &mut SpikeState,
    frame: &AeroFrameV1,
    serial: u32,
    width: i32,
    height: i32,
) {
    let Some(entry) = state.entries.iter_mut().find(|e| &e.frame == frame) else {
        return;
    };
    let (l, t, r, b) = entry.insets;
    let total_w = width + l + r;
    let total_h = height + t + b;
    entry.total = (total_w, total_h);
    let grows = total_w > entry.canvas.0 || total_h > entry.canvas.1;
    if grows {
        entry.canvas = (entry.canvas.0.max(total_w), entry.canvas.1.max(total_h));
        entry
            .window
            .set_default_size(entry.canvas.0, entry.canvas.1);
    }
    eprintln!(
        "FRAMEWEB: configure serial={serial} conteudo={width}x{height} target={total_w}x{total_h} canvas={}x{} grows={grows}",
        entry.canvas.0, entry.canvas.1
    );
    frame.ack_configure(serial);
    let script = format!("window.aeroFramePainted && window.aeroFramePainted({serial});");
    entry.webview.evaluate_javascript(
        &script,
        None,
        None,
        None::<&webkit6::gio::Cancellable>,
        |result| {
            if let Err(err) = result {
                eprintln!("FRAMEWEB: aeroFramePainted falhou: {err}");
            }
        },
    );
}

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
    static APP: RefCell<Option<Rc<gtk4::Application>>> = const { RefCell::new(None) };
    static ACTIVATED: Cell<bool> = const { Cell::new(false) };
    static ANCHOR: RefCell<Option<(gtk4::ApplicationWindow, WebView)>> = const { RefCell::new(None) };
    /// Janela 1×1 sem WebView: serve só para derivar a conexão Wayland e anunciar
    /// as insets padrão ANTES de iniciar o WebProcess da âncora.
    static BOOTSTRAP_WINDOW: RefCell<Option<gtk4::ApplicationWindow>> = const { RefCell::new(None) };
    static POOL: RefCell<Vec<PoolItem>> = const { RefCell::new(Vec::new()) };
    static PENDING_FRAMES: RefCell<VecDeque<(u32, i32, i32, bool)>> = const { RefCell::new(VecDeque::new()) };
    /// Configures que chegaram antes da entry do frame existir.
    static PENDING_CONFIGURES: RefCell<Vec<(AeroFrameV1, u32, i32, i32)>> = const { RefCell::new(Vec::new()) };
    static NEXT_POOL_ID: Cell<usize> = const { Cell::new(0) };
    static POOL_FILL_SCHEDULED: Cell<bool> = const { Cell::new(false) };
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

/// Lê `list` até o `.` (o `window_ipc` acima lê só a primeira linha) e devolve
/// o título do window_id pedido. O IPC já expõe o título real do toplevel
/// (`window_ipc.rs:30-41,80-102`); este é só o cliente do shell.
fn ipc_title(window_id: u32) -> Option<String> {
    let path = std::env::var_os("AERO_WINDOW_IPC_SOCK")?;
    let mut stream = UnixStream::connect(path).ok()?;
    writeln!(stream, "list").ok()?;
    stream.flush().ok()?;
    // Sem shutdown da escrita o compositor espera outro comando e o read bloqueia.
    stream.shutdown(Shutdown::Write).ok()?;
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if line == "." {
            break;
        }
        // "W id x y w h <titulo-json>" — splitn(7) mantém o JSON (com espaços) inteiro.
        let mut parts = line.splitn(7, ' ');
        if parts.next() != Some("W") {
            continue;
        }
        let Some(id) = parts.next().and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        if id == window_id {
            return parts.nth(4).and_then(|raw| serde_json::from_str(raw).ok());
        }
    }
    None
}

/// Fonte do ícone: pixels crus do protocolo ou nome do tema (ex.: `foot` envia nome).
enum IconSource {
    Pixels {
        w: i32,
        h: i32,
        format: String,
        b64: String,
    },
    Name(String),
}

/// Pergunta ao IPC se este window_id pediu client-side decorations.
fn ipc_csd(window_id: u32) -> bool {
    let Some(path) = std::env::var_os("AERO_WINDOW_IPC_SOCK") else {
        return false;
    };
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    if writeln!(stream, "csd {window_id}").is_err() || stream.flush().is_err() {
        return false;
    }
    let _ = stream.shutdown(Shutdown::Write);
    let Some(Ok(line)) = BufReader::new(stream).lines().next() else {
        return false;
    };
    let mut parts = line.splitn(3, ' ');
    parts.next() == Some("CSD")
        && parts.next().and_then(|v| v.parse::<u32>().ok()) == Some(window_id)
        && parts.next() == Some("1")
}

/// Resolve um nome do tema GTK para uma data URI SVG/PNG. Nenhuma dependência
/// nova: `gtk4::IconTheme` já é dependência do shell e `glib::base64_encode` vem
/// do glib. Same invariant as the reference project: bytes on the wire in
/// base64, decoded into a data URI that the WebView can draw without fetching.
fn theme_icon_uri(name: &str) -> Option<String> {
    let display = gtk4::gdk::Display::default()?;
    let theme = IconTheme::for_display(&display);
    let paintable = theme.lookup_icon(
        name,
        &[],
        16,
        1,
        TextDirection::Ltr,
        IconLookupFlags::empty(),
    );
    let path = paintable.file()?.path()?;
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => return None,
    };
    let bytes = std::fs::read(&path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        glib::base64_encode(&bytes)
    ))
}

/// Lê `icon <id>` (uma linha, sem `.`) do IPC. O compositor responde com pixels
/// (`ICON <id> <w> <h> <scale> <formato> <base64>`) ou nome (`ICON <id> NAME <json>`).
fn ipc_icon(window_id: u32) -> Option<IconSource> {
    let path = std::env::var_os("AERO_WINDOW_IPC_SOCK")?;
    let mut stream = UnixStream::connect(path).ok()?;
    writeln!(stream, "icon {window_id}").ok()?;
    stream.flush().ok()?;
    stream.shutdown(Shutdown::Write).ok()?;
    let line = BufReader::new(stream).lines().next()?.ok()?;
    let mut parts = line.splitn(7, ' ');
    if parts.next() != Some("ICON") {
        return None;
    }
    if parts.next()?.parse::<u32>().ok()? != window_id {
        return None;
    }
    let first = parts.next()?;
    if first == "NAME" {
        return Some(IconSource::Name(serde_json::from_str(parts.next()?).ok()?));
    }
    let w = first.parse().ok()?;
    let h = parts.next()?.parse().ok()?;
    let _scale = parts.next()?; // informativo; o JS desenha na resolução recebida
    let format = parts.next()?.to_owned();
    let b64 = parts.next()?.to_owned();
    Some(IconSource::Pixels { w, h, format, b64 })
}

/// Aplica título e ícone vindos do IPC. Chamado no associate e a cada 1 s.
fn refresh_titles() {
    if std::env::var_os("AERO_WINDOW_IPC_SOCK").is_none() {
        return;
    }
    let targets: Vec<(u32, WebView)> = RUNTIME.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|rt| {
                rt.state
                    .entries
                    .iter()
                    .map(|entry| (entry.window_id, entry.webview.clone()))
                    .collect()
            })
            .unwrap_or_default()
    });
    for (window_id, webview) in targets {
        if let Some(title) = ipc_title(window_id) {
            let changed = RUNTIME.with(|cell| {
                let mut borrow = cell.borrow_mut();
                let Some(rt) = borrow.as_mut() else {
                    return false;
                };
                let Some(entry) = rt
                    .state
                    .entries
                    .iter_mut()
                    .find(|e| e.window_id == window_id)
                else {
                    return false;
                };
                if entry.last_title.as_deref() == Some(title.as_str()) {
                    return false;
                }
                entry.last_title = Some(title.clone());
                true
            });
            if changed {
                let quoted = serde_json::to_string(&title).unwrap_or_else(|_| "\"\"".to_owned());
                let script = format!("window.aeroSetTitle && window.aeroSetTitle({quoted});");
                webview.evaluate_javascript(
                    &script,
                    None,
                    None,
                    None::<&webkit6::gio::Cancellable>,
                    |result| {
                        if let Err(err) = result {
                            eprintln!("FRAMEWEB: aeroSetTitle falhou: {err}");
                        }
                    },
                );
                eprintln!("FRAMEWEB: title_set window_id={window_id} titulo={title}");
            }
        }

        // Ícone: assinatura = bytes (ou nome) atualmente publicados pelo
        // compositor. Relido a cada poll; só reenviamos ao DOM quando muda.
        let icon = ipc_icon(window_id);
        let signature = match &icon {
            Some(IconSource::Pixels { b64, .. }) => Some(b64.clone()),
            Some(IconSource::Name(name)) => Some(format!("name:{name}")),
            None => None,
        };
        let changed = RUNTIME.with(|cell| {
            let mut borrow = cell.borrow_mut();
            let Some(rt) = borrow.as_mut() else {
                return false;
            };
            let Some(entry) = rt
                .state
                .entries
                .iter_mut()
                .find(|e| e.window_id == window_id)
            else {
                return false;
            };
            if entry.last_icon == signature {
                return false;
            }
            entry.last_icon = signature;
            true
        });
        // NÃO usar `continue` aqui: isso pularia a checagem de CSD abaixo
        // quando o ícone não muda (o caso do GTK4 no tty3).
        if changed {
            let (script, log) = match icon {
                Some(IconSource::Pixels { w, h, format, b64 }) => {
                    let payload = serde_json::json!({
                        "w": w, "h": h, "format": format, "b64": b64,
                    });
                    let payload =
                        serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_owned());
                    (
                        format!("window.aeroSetIcon && window.aeroSetIcon({payload});"),
                        format!(
                        "FRAMEWEB: titlebar_icon window_id={window_id} w={w} h={h} source=protocolo"
                    ),
                    )
                }
                Some(IconSource::Name(name)) => match theme_icon_uri(&name) {
                    Some(uri) => {
                        let payload = serde_json::json!({ "uri": uri });
                        let payload =
                            serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_owned());
                        (
                            format!("window.aeroSetIcon && window.aeroSetIcon({payload});"),
                            format!(
                            "FRAMEWEB: titlebar_icon window_id={window_id} source=nome name={name}"
                        ),
                        )
                    }
                    None => (
                        "window.aeroSetIcon && window.aeroSetIcon(null);".to_owned(),
                        format!("FRAMEWEB: titlebar_icon window_id={window_id} source=fallback"),
                    ),
                },
                None => (
                    "window.aeroSetIcon && window.aeroSetIcon(null);".to_owned(),
                    format!("FRAMEWEB: titlebar_icon window_id={window_id} source=fallback"),
                ),
            };
            webview.evaluate_javascript(
                &script,
                None,
                None,
                None::<&webkit6::gio::Cancellable>,
                |result| {
                    if let Err(err) = result {
                        eprintln!("FRAMEWEB: aeroSetIcon falhou: {err}");
                    }
                },
            );
            eprintln!("{log}");
        }
    }
}

/// Checagem rápida de CSD: o GTK4 só expõe a window_geometry depois do
/// primeiro commit, então um poll de 1 s deixava a moldura aparecer por
/// ~0,5 s. Aqui a sonda roda a cada 100 ms nos primeiros 5 s da janela.
fn refresh_csd() {
    if std::env::var_os("AERO_WINDOW_IPC_SOCK").is_none() {
        return;
    }
    let targets: Vec<u32> = RUNTIME.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|rt| {
                rt.state
                    .entries
                    .iter()
                    .filter(|entry| !entry.csd_active && Instant::now() < entry.csd_probe_until)
                    .map(|entry| entry.window_id)
                    .collect()
            })
            .unwrap_or_default()
    });
    for window_id in targets {
        // O gedit não pede ClientSide: a sombra aparece na window_geometry
        // só depois do primeiro commit. Por isso CSD também é consultado no
        // polling (como título e ícone), não apenas no frame_requested.
        let detected = ipc_csd(window_id);
        if detected {
            let state_change = RUNTIME.with(|cell| {
                let mut borrow = cell.borrow_mut();
                let rt = borrow.as_mut()?;
                let entry = rt
                    .state
                    .entries
                    .iter_mut()
                    .find(|entry| entry.window_id == window_id)?;
                if entry.csd_active {
                    return None;
                }
                entry.csd_active = true;
                entry.insets = (0, 0, 0, 0);
                Some((entry.frame.clone(), entry.webview.clone()))
            });
            if let Some((frame, view)) = state_change {
                frame.set_insets(0, 0, 0, 0);
                frame.set_titlebar_slices(0, 0);
                view.evaluate_javascript(
                    "window.aeroHideFrame && window.aeroHideFrame();",
                    None,
                    None,
                    None::<&webkit6::gio::Cancellable>,
                    |result| {
                        if let Err(err) = result {
                            eprintln!("FRAMEWEB: aeroHideFrame falhou: {err}");
                        }
                    },
                );
                eprintln!(
                "FRAMEWEB: csd_late window_id={window_id} insets=(0,0,0,0) reason=geometry t={}",
                bench_t()
            );
            }
        }
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
  /* 02l: largura FIXA do texto; o intervalo até os botões fica em branco e
     é a única parte da barra que o compositor pode esticar no resize. */
  #title{flex:1 1 auto;min-width:0;font-size:13px;
    white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
  /* 16px dentro da zona de texto já reservada: sem mudar insets/slices. */
  #icon{width:16px;height:16px;margin-right:6px;display:none;flex:0 0 16px;}
  #slot{display:flex;align-items:center;flex:0 0 120px;width:120px;padding:0 8px;
    font-size:13px;white-space:nowrap;overflow:hidden;}
  #max{margin-left:auto;}
  button{width:30px;height:30px;border:0;background:transparent;color:#eee;
    font-size:14px;cursor:pointer;}
  button:hover{background:#444;}
  #close:hover{background:#c0392b;}
</style></head>
<body>
  <div id="border"></div>
  <div id="bar" data-aero-interactive>
    <div id="slot"><img id="icon" alt=""><span id="title">janela</span></div>
    <button id="max" title="maximizar">&#9633;</button>
    <button id="close" title="fechar">&#10005;</button>
  </div>
  <script>
    const send = (action) => window.webkit.messageHandlers.aero.postMessage({ action });
    // O host chama com o título vindo do IPC; devolvemos o valor LIDO DO DOM
    // para a sonda provar que o #title foi realmente atualizado.
    // GTK4/gedit desenham a própria moldura: escondemos a barra HTML e
    // devolvemos o valor lido do DOM como prova (sem barra dupla).
    window.aeroHideFrame = () => {
      const bar = document.getElementById('bar');
      const border = document.getElementById('border');
      bar.style.display = 'none';
      border.style.display = 'none';
      window.webkit.messageHandlers.aero.postMessage({
        action: 'csd_applied', client_frame: true,
        bar: getComputedStyle(bar).display,
        border: getComputedStyle(border).display
      });
    };
    window.aeroSetTitle = (title) => {
      const el = document.getElementById('title');
      el.textContent = title;
      el.title = title;
      window.webkit.messageHandlers.aero.postMessage({
        action: 'title_applied', title: el.textContent
      });
    };
    // Pixels vêm do IPC em base64 (o compositor não tem encoder de imagem).
    // O canvas faz a conversão para PNG e o browser gera a data URI: sem
    // dependência nova e sem rede — o WebView continua só em aero:// + data:.
    window.aeroSetIcon = (payload) => {
      const img = document.getElementById('icon');
      if (payload && payload.uri) {
        img.src = payload.uri;
        img.style.display = 'block';
        window.webkit.messageHandlers.aero.postMessage({
          action: 'icon_applied', src: img.src.slice(0, 22), visible: 'block'
        });
        return;
      }
      if (!payload || !payload.b64) {
        img.removeAttribute('src');
        img.style.display = 'none';
      } else {
        const raw = Uint8Array.from(atob(payload.b64), (c) => c.charCodeAt(0));
        const canvas = document.createElement('canvas');
        canvas.width = payload.w;
        canvas.height = payload.h;
        const ctx = canvas.getContext('2d');
        const image = ctx.createImageData(payload.w, payload.h);
        for (let i = 0; i < image.data.length; i += 4) {
          if (payload.format === 'Argb8888') {
            image.data[i] = raw[i + 2];
            image.data[i + 1] = raw[i + 1];
            image.data[i + 2] = raw[i];
            image.data[i + 3] = raw[i + 3];
          } else if (payload.format === 'Xrgb8888') {
            image.data[i] = raw[i + 2];
            image.data[i + 1] = raw[i + 1];
            image.data[i + 2] = raw[i];
            image.data[i + 3] = 255;
          }
        }
        ctx.putImageData(image, 0, 0);
        img.src = canvas.toDataURL('image/png');
        img.style.display = 'block';
      }
      // Devolvemos só o prefixo (o base64 do PNG é grande) como prova do DOM.
      window.webkit.messageHandlers.aero.postMessage({
        action: 'icon_applied', src: img.src.slice(0, 22), visible: img.style.display
      });
    };
    window.aeroFramePainted = (serial) => {
      requestAnimationFrame(() => requestAnimationFrame(() => {
        window.webkit.messageHandlers.aero.postMessage({ action: 'frame_painted', serial });
      }));
    };
    document.getElementById('close').addEventListener('click', () => send('close'));
    document.getElementById('max').addEventListener('click', () => send('maximize'));
  </script>
</body></html>"#;

/// View-âncora construct-only do WebKitGTK (ADR 0001): nunca é apresentada e
/// permanece viva para que TODAS as views de frame compartilhem um WebProcess.
fn create_anchor() {
    let app = APP
        .with(|cell| cell.borrow().clone())
        .expect("APP não inicializado");
    let window = gtk4::ApplicationWindow::builder()
        .application(&*app)
        .title("aero frame anchor (hidden)")
        .default_width(1)
        .default_height(1)
        .build();
    let webview = WebView::builder().build();
    webview.load_html(TITLEBAR_HTML, Some("aero://frame-anchor/"));
    window.set_child(Some(&webview));
    // Realize cria o GdkSurface/wl_surface, mas sem present não há janela visível
    // nem associate: a âncora não entra no LayerMap nem recebe window_id.
    gtk4::prelude::WidgetExt::realize(&window);
    ANCHOR.with(|cell| *cell.borrow_mut() = Some((window, webview)));
    eprintln!(
        "FRAMEWEB: anchor viva (oculta, related_view em todos os frames) anchor_viva t={}",
        bench_t()
    );
}

/// Inicializa o runtime Wayland com uma janela 1×1 SEM WebView. Isso permite
/// anunciar as insets padrão antes de pagar o cold-start do WebProcess da
/// âncora: a decoração sólida do compositor já nasce na geometria correta.
fn bootstrap_at_startup() {
    let app = APP
        .with(|cell| cell.borrow().clone())
        .expect("APP não inicializado");
    let window = gtk4::ApplicationWindow::builder()
        .application(&*app)
        .title("aero bootstrap (hidden)")
        .default_width(1)
        .default_height(1)
        .build();
    gtk4::prelude::WidgetExt::realize(&window);
    BOOTSTRAP_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    let gdk_surface = window.surface();
    let Some(gdk_surface) = gdk_surface else {
        eprintln!("FRAMEWEB: âncora sem GdkSurface; runtime não iniciado");
        return;
    };
    let Some(wayland) = gdk_surface.downcast_ref::<WaylandSurface>() else {
        eprintln!("FRAMEWEB: GdkSurface da âncora não é WaylandSurface");
        return;
    };
    let Some(wl_surface) = wayland.wl_surface() else {
        eprintln!("FRAMEWEB: wl_surface=None na âncora");
        return;
    };
    bootstrap_runtime(&wl_surface);
}

fn associate_frame(
    wl_surface: aero::wl_surface::WlSurface,
    window: gtk4::ApplicationWindow,
    webview: WebView,
    canvas: (i32, i32),
    window_id: u32,
    client_frame: bool,
) {
    let hide_view = webview.clone();
    // GTK3/gedit desenham a própria moldura: SEM titlebar e SEM bordas.
    let (left, top, right, bottom) = if client_frame {
        (0, 0, 0, 0)
    } else {
        (BORDER, TITLEBAR_H, BORDER, BORDER)
    };
    let (title_left, title_right) = if client_frame {
        (0, 0)
    } else {
        (TITLE_TEXT_W, RIGHT_CONTROLS)
    };
    RUNTIME.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let rt = borrow.as_mut().expect("RUNTIME não inicializado");
        let qh = rt.queue.handle();
        let manager = rt.state.manager.clone().expect("manager ausente");
        let frame = manager.associate(&wl_surface, window_id, &qh, ());
        frame.set_insets(left, top, right, bottom);
        frame.set_titlebar_slices(title_left, title_right);
        let _ = rt.queue.flush();
        rt.state.entries.push(FrameEntry {
            frame: frame.clone(),
            wl_surface,
            window,
            webview,
            insets: (left, top, right, bottom),
            total: (0, 0),
            canvas,
            presented: false,
            window_id,
            last_title: None,
            last_icon: None,
            csd_active: client_frame,
            csd_probe_until: Instant::now() + Duration::from_secs(5),
        });
        eprintln!("FRAMEWEB: associate(window_id={window_id}) + set_insets({left},{top},{right},{bottom}) \
                   + set_titlebar_slices({title_left},{title_right}) client_frame={client_frame} t={}", bench_t());
        let pending = PENDING_CONFIGURES.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
        for (pending_frame, serial, width, height) in pending {
            if pending_frame == frame {
                apply_configure(&mut rt.state, &pending_frame, serial, width, height);
            }
        }
    });
    if client_frame {
        hide_view.evaluate_javascript(
            "window.aeroHideFrame && window.aeroHideFrame();",
            None,
            None,
            None::<&webkit6::gio::Cancellable>,
            |result| {
                if let Err(err) = result {
                    eprintln!("FRAMEWEB: aeroHideFrame falhou: {err}");
                }
            },
        );
    }
    // Fora do borrow do RUNTIME: o refresh consulta o estado de novo.
    glib::idle_add_local(|| {
        refresh_titles();
        glib::ControlFlow::Break
    });
}

fn take_ready_frame() -> Option<(PoolItem, usize)> {
    POOL.with(|cell| {
        let mut pool = cell.borrow_mut();
        let position = pool.iter().position(|item| item.ready)?;
        let item = pool.remove(position);
        let available = pool.iter().filter(|item| item.ready).count();
        Some((item, available))
    })
}

/// Cria no máximo uma view por idle. O carregamento é assíncrono; a reposição
/// não bloqueia o take que acabou de consumir uma view pronta.
fn ensure_pool_fill() {
    if POOL_FILL_SCHEDULED.with(|flag| flag.replace(true)) {
        return;
    }
    glib::idle_add_local(|| {
        POOL_FILL_SCHEDULED.with(|flag| flag.set(false));
        if POOL.with(|cell| cell.borrow().len()) < POOL_TARGET {
            prewarm_frame();
        }
        if POOL.with(|cell| cell.borrow().len()) < POOL_TARGET {
            ensure_pool_fill();
        }
        glib::ControlFlow::Break
    });
}

fn request_frame(window_id: u32, canvas_w: i32, canvas_h: i32, client_frame: bool) {
    if let Some((item, available)) = take_ready_frame() {
        eprintln!("FRAMEWEB: pool_take window_id={window_id} available={available}");
        open_frame_window(window_id, canvas_w, canvas_h, item, client_frame);
    } else {
        eprintln!("FRAMEWEB: pool_vazio window_id={window_id}; request enfileirado");
        PENDING_FRAMES.with(|cell| {
            cell.borrow_mut()
                .push_back((window_id, canvas_w, canvas_h, client_frame));
        });
    }
    ensure_pool_fill();
}

fn drain_pending_frames() {
    loop {
        let request = PENDING_FRAMES.with(|cell| cell.borrow_mut().pop_front());
        let Some((window_id, canvas_w, canvas_h, client_frame)) = request else {
            break;
        };
        if let Some((item, available)) = take_ready_frame() {
            eprintln!("FRAMEWEB: pool_take window_id={window_id} available={available}");
            open_frame_window(window_id, canvas_w, canvas_h, item, client_frame);
        } else {
            PENDING_FRAMES.with(|cell| {
                cell.borrow_mut()
                    .push_front((window_id, canvas_w, canvas_h, client_frame));
            });
            break;
        }
    }
    ensure_pool_fill();
}

fn pool_mark_ready(pool_id: usize) {
    let available = POOL.with(|cell| {
        let mut pool = cell.borrow_mut();
        let item = pool.iter_mut().find(|item| item.id == pool_id)?;
        if item.ready {
            return None;
        }
        item.ready = true;
        Some(pool.iter().filter(|item| item.ready).count())
    });
    if let Some(available) = available {
        eprintln!("FRAMEWEB: pool_ready available={available} t={}", bench_t());
        drain_pending_frames();
    }
}

/// Pré-aquece window + WebView relacionada sem realizá-la nem associá-la.
/// A autorização da via A define ready = HTML carregado; o placeholder cobre
/// o paint que acontece depois do take, quando a surface é apresentada.
fn prewarm_frame() {
    let pool_id = NEXT_POOL_ID.with(|cell| {
        let id = cell.get();
        cell.set(id + 1);
        id
    });
    let app = APP
        .with(|cell| cell.borrow().clone())
        .expect("APP não inicializado");
    let window = gtk4::ApplicationWindow::builder()
        .application(&*app)
        .title(format!("frame webview pool {pool_id}"))
        .build();
    window.add_css_class("aero-frame");
    window.init_layer_shell();
    window.set_namespace(Some("aeroshell-frame-webview"));
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

    let window_id = Rc::new(Cell::new(None));
    let callback_window_id = Rc::clone(&window_id);
    let manager = UserContentManager::new();
    manager.register_script_message_handler("aero", None);
    manager.connect_script_message_received(Some("aero"), move |_, value| {
        let json = value.to_json(0).unwrap_or_default().to_string();
        let parsed =
            serde_json::from_str::<serde_json::Value>(&json).unwrap_or(serde_json::Value::Null);
        let action = parsed
            .get("action")
            .and_then(|a| a.as_str())
            .map(str::to_owned);
        match action.as_deref() {
            Some("close") => {
                if let Some(window_id) = callback_window_id.get() {
                    window_ipc(&format!("close {window_id}"));
                }
            }
            Some("maximize") => {
                if let Some(window_id) = callback_window_id.get() {
                    window_ipc(&format!("maximize {window_id}"));
                }
            }
            Some("csd_applied") => {
                let bar = parsed.get("bar").and_then(|v| v.as_str()).unwrap_or("");
                let border = parsed.get("border").and_then(|v| v.as_str()).unwrap_or("");
                let Some(window_id) = callback_window_id.get() else {
                    return;
                };
                eprintln!(
                    "FRAMEWEB: csd_applied t={} window_id={window_id} client_frame=true bar={} border={}",
                    bench_t(),
                    if bar == "none" { "hidden" } else { bar },
                    if border == "none" { "hidden" } else { border }
                );
            }
            Some("title_applied") => {
                let title = parsed.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let Some(window_id) = callback_window_id.get() else {
                    return;
                };
                eprintln!("FRAMEWEB: title_applied window_id={window_id} titulo={title}");
            }
            Some("icon_applied") => {
                let src = parsed.get("src").and_then(|v| v.as_str()).unwrap_or("");
                let visible = parsed.get("visible").and_then(|v| v.as_str()).unwrap_or("");
                let Some(window_id) = callback_window_id.get() else {
                    return;
                };
                eprintln!(
                    "FRAMEWEB: icon_applied window_id={window_id} src={src} visible={visible}"
                );
            }
            Some("frame_painted") => {
                let Some(serial) = parsed
                    .get("serial")
                    .and_then(|serial| serial.as_u64())
                    .and_then(|serial| u32::try_from(serial).ok())
                else {
                    eprintln!("FRAMEWEB: frame_painted com serial inválido: {json}");
                    return;
                };
                let Some(window_id) = callback_window_id.get() else {
                    return;
                };
                RUNTIME.with(|cell| {
                    let mut borrow = cell.borrow_mut();
                    let Some(rt) = borrow.as_mut() else {
                        return;
                    };
                    let frame = rt
                        .state
                        .entries
                        .iter()
                        .find(|entry| entry.window_id == window_id)
                        .map(|entry| entry.frame.clone());
                    if let Some(frame) = frame {
                        frame.frame_painted(serial);
                        let _ = rt.queue.flush();
                        eprintln!(
                            "FRAMEWEB: frame_painted window_id={window_id} serial={serial} \
                             html_painted t={} ",
                            bench_t()
                        );
                    }
                });
            }
            _ => eprintln!("FRAMEWEB: mensagem da WebView: {json}"),
        }
    });
    let anchor = ANCHOR
        .with(|cell| cell.borrow().as_ref().map(|(_, view)| view.clone()))
        .expect("WebView-âncora ausente");
    let webview = WebView::builder()
        .related_view(&anchor)
        .user_content_manager(&manager)
        .build();
    webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    window.set_child(Some(&webview));
    webview.connect_load_changed(move |_, event| {
        if event == webkit6::LoadEvent::Finished {
            eprintln!("FRAMEWEB: html_ready t={} pool_id={pool_id}", bench_t());
            pool_mark_ready(pool_id);
        }
    });
    POOL.with(|cell| {
        cell.borrow_mut().push(PoolItem {
            id: pool_id,
            window,
            webview: webview.clone(),
            window_id,
            ready: false,
        });
    });
    webview.load_html(TITLEBAR_HTML, Some("aero://frame/"));
}

/// Consome uma view já carregada. O associate ocorre no realize e o present
/// espera o frame_requested do output para fixar o canvas inicial.
fn open_frame_window(
    window_id: u32,
    canvas_w: i32,
    canvas_h: i32,
    item: PoolItem,
    client_frame: bool,
) {
    let PoolItem {
        window,
        webview,
        window_id: assigned_window_id,
        ..
    } = item;
    assigned_window_id.set(Some(window_id));
    if canvas_w > 0 && canvas_h > 0 {
        window.set_default_size(canvas_w, canvas_h);
    }
    window.connect_realize(move |w| {
        let gdk_surface = w.surface().expect("janela sem GdkSurface");
        let ws = gdk_surface
            .downcast_ref::<WaylandSurface>()
            .expect("GdkSurface não é WaylandSurface");
        let Some(wl_surface) = ws.wl_surface() else {
            eprintln!("FRAMEWEB: wl_surface=None no realize");
            return;
        };
        if RUNTIME.with(|cell| cell.borrow().is_none()) {
            bootstrap_runtime(&wl_surface);
        }
        if RUNTIME.with(|cell| cell.borrow().is_some()) {
            associate_frame(
                wl_surface,
                w.clone(),
                webview.clone(),
                (canvas_w, canvas_h),
                window_id,
                client_frame,
            );
        }
    });
    window.connect_map(move |_| eprintln!("FRAMEWEB map: layer surface criada (id={window_id})"));
    gtk4::prelude::WidgetExt::realize(&window);
}

/// Primeira WebView: deriva o Connection do backend do wl_surface do GTK
/// (mesmo guest) e espera o registry sem roundtrip bloqueante.
fn bootstrap_runtime(wl_surface: &aero::wl_surface::WlSurface) {
    let backend = wl_surface.backend().upgrade().expect("backend sumiu");
    let connection = wayland_client::Connection::from_backend(backend);
    let mut queue: wayland_client::EventQueue<SpikeState> = connection.new_event_queue();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let _ = queue.flush();
    let mut state = SpikeState::default();
    // Sem sleep e sem contagem de iterações cega: `blocking_dispatch` espera o
    // próximo evento do registry que nós mesmos acabamos de pedir. O teto de 4
    // dispatchs é só defesa contra um servidor que nunca responde; não há atraso
    // fixo nem janela presa num número arbitrário de iterações vazias.
    for _ in 0..4 {
        if state.manager.is_some() {
            break;
        }
        let _ = queue.blocking_dispatch(&mut state);
    }
    eprintln!(
        "FRAMEWEB: manager no guest = {} bootstrap t={}",
        state.manager.is_some(),
        bench_t()
    );
    if state.manager.is_some() {
        RUNTIME.with(|cell| *cell.borrow_mut() = Some(Runtime { queue, state }));
    }
}

fn main() {
    let app = Rc::new(
        gtk4::Application::builder()
            .application_id("dev.aeroshell.frame-webview")
            // 02s: sem esta flag o GApplication registra o nome no D-Bus de
            // sessão; uma segunda instância só reativa a primeira e sai sem
            // executar o connect_activate — o shell privilegiado cai e a janela
            // fica sem decoração. Cada processo deve ser independente.
            // Fonte: gtk4-0.11.5/src/auto/application.rs:72 (builder.flags) e
            // gio-0.22.11/src/auto/flags.rs:112-113 (NON_UNIQUE).
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build(),
    );
    APP.with(|cell| *cell.borrow_mut() = Some(Rc::clone(&app)));

    app.connect_activate(move |_app| {
        // GApplication pode ser ativado novamente por outro processo com o mesmo
        // application_id. Reabrir window_id=0 causaria already_constructed e
        // derrubaria a conexão privilegiada (e todas as decorações).
        if ACTIVATED.with(|flag| flag.replace(true)) {
            eprintln!("FRAMEWEB: activate repetido ignorado");
            return;
        }
        // Fundo do frame: sem fundo de tema, a área recém-exposta durante resize
        // mostra o que está atrás em vez de branco (a decoração é pintada só pelo HTML).
        let css = gtk4::CssProvider::new();
        css.load_from_string(".aero-frame { background-color: rgba(0, 0, 0, 0); }");
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("display GTK ausente"),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // A âncora é separada e oculta. O runtime é iniciado AQUI (não no
        // primeiro associate) para anunciar as insets padrão no startup, e o
        // pool é pré-aquecido antes de qualquer cliente mapear. Não existe mais
        // "frame da janela 0" criado por iniciativa do shell: o compositor pede
        // um frame para cada janela mapeada via frame_requested.
        bootstrap_at_startup();
        create_anchor();
        ensure_pool_fill();

        glib::timeout_add_local(std::time::Duration::from_millis(20), || {
            dispatch_nonblocking();
            glib::ControlFlow::Continue
        });
        // O IPC de janelas é request/response; o título muda é descoberto por
        // polling curto. Evento de mudança de título é melhoria F2 (streaming).
        glib::timeout_add_local(Duration::from_secs(1), || {
            refresh_titles();
            glib::ControlFlow::Continue
        });
        // CSD é descoberto por protocolo (window_geometry/KDE/XDG), e o GTK4
        // só o expõe após o primeiro commit. Sonda rápida nos primeiros 5 s.
        glib::timeout_add_local(Duration::from_millis(100), || {
            refresh_csd();
            glib::ControlFlow::Continue
        });
    });
    app.run();
}
