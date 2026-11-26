//! IPC de controle de janelas (mecanismo em Rust; política em TS via SDK).
//! Socket Unix separado do Wayland, protocolo de texto por linha:
//!   list            -> uma linha `W id x y w h <titulo-json>` por janela, terminada por `.`
//!                      (titulo-json é uma string JSON, para preservar espaços/UTF-8/newlines)
//!   focus <id>      -> OK
//!   close <id>      -> OK
//!   maximize <id>   -> OK (alterna maximizado)
//!   icon <id>       -> ICON <id> <w> <h> <scale> <formato> <base64-dos-pixels> | NONE
//!   csd <id>        -> CSD <id> 1|0  (1 = cliente pediu client-side decorations)
//!   policy-ready    -> OK (uso interno: runtime Bun conectado)
//!   policy <id> ... -> OK (uso interno: decisão de decoração)
//!   policy-reload   -> OK (uso interno: reemite DecorationRequest às janelas decididas)
//!   launch <app_id> [workspace_id] -> OK (lança ou foca uma instância existente)
//!   surface-create <json> -> OK (declara superfície web e publica SurfaceCreated)
//!   surface-list          -> uma linha `S <json>` por superfície, terminada por `.`
//!   surface-update <json> -> OK (mescla campos e publica SurfaceUpdated)
//!   surface-destroy <id>  -> OK (publica SurfaceDestroyed)
//!   notify <titulo-json> <corpo-json> [icone-json] [timeout-ms] [id-json]
//!                       -> OK (forma pública: notify <titulo> <corpo>)
//!   dismiss-notification <id-json> -> OK
//! Cada request é despachado no event loop (canal calloop) e respondido por um
//! oneshot mpsc, para que `list` veja o estado atual do space sem snapshot obsoleto.

use std::{
    io::{self, BufRead, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
    time::Duration,
};

use aeroshell_ipc::types::{
    AeroEvent, App, DecorationPolicy, Notification, OutputGeometry, OutputInfo, OutputMode,
    Snapshot, SurfaceAction, SurfaceAnchors, SurfaceInputRegion, SurfaceKeyboardMode, SurfaceKind,
    SurfaceLayer, SurfaceMargins, SurfaceSpec, WindowInfo, Workspace,
};
use calloop::channel;
use serde_json::{json, Value};
use smithay::{
    desktop::Window,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::SERIAL_COUNTER,
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use crate::input::{parse_key_binding, RegisteredKeyBind};
use crate::protocols::toplevel_icon::IconPixels;
use crate::protocols::toplevel_states::{app_id_for, frame_state_for};
use crate::protocols::xdg_shell::client_draws_frame;
use crate::state::AeroState;

/// Título atual do toplevel, lido dos atributos da superfície no event loop.
/// Fonte: smithay 0.7.0 `wayland/shell/xdg/mod.rs:364` (`XdgToplevelSurfaceRoleAttributes.title`).
pub fn window_title(window: &Window) -> String {
    let Some(toplevel) = window.toplevel() else {
        return String::new();
    };
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok().and_then(|attrs| attrs.title.clone()))
    })
    .unwrap_or_default()
}

/// Base64 padrão com padding, para levar bytes na linha do IPC. Implementação
/// pequena de propósito: nenhuma dependência nova (mesmo invariante do projeto
/// de referência `domicile-engine/packages/domicile-host/src/base64.rs`).
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let word = chunk.iter().enumerate().fold(0u32, |word, (at, byte)| {
            word | u32::from(*byte) << (16 - 8 * at)
        });
        for at in 0..4 {
            out.push(if at <= chunk.len() {
                char::from(ALPHABET[((word >> (18 - 6 * at)) & 0x3f) as usize])
            } else {
                '='
            });
        }
    }
    out
}

/// Pixels do ícone copiados no `add_buffer` pelo nosso handler
/// (`protocols/toplevel_icon.rs`). Nenhum `wl_buffer` é retido, então o destroy
/// do cliente (sequência do Qt) não pode matar a conexão.
/// Devolve (largura, altura, escala, formato, BGRA em linhas tightly-packed).
fn icon_pixels(surface: &WlSurface) -> Option<(i32, i32, i32, &'static str, Vec<u8>)> {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<IconPixels>();
        cached.current().biggest.clone()
    })
}

/// Nome de ícone do tema, quando o cliente não enviou buffers (ex.: `foot`).
fn icon_name(surface: &WlSurface) -> Option<String> {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<IconPixels>();
        cached.current().name.clone()
    })
}

fn focus_window(state: &mut AeroState, window_id: u32) -> Result<(), String> {
    let window = state
        .window_for_id(window_id)
        .cloned()
        .ok_or_else(|| "ERR janela inexistente".to_owned())?;
    state.space.raise_element(&window, true);
    if let Some(toplevel) = window.toplevel() {
        if let Some(keyboard) = state.seat.get_keyboard() {
            keyboard.set_focus(
                state,
                Some(toplevel.wl_surface().clone()),
                SERIAL_COUNTER.next_serial(),
            );
        }
    }
    Ok(())
}

fn activate_workspace(state: &mut AeroState, workspace_id: u32) -> Result<(), String> {
    if state.workspaces.active_id() != workspace_id {
        state.workspaces.activate(workspace_id)?;
        emit_event(AeroEvent::WorkspaceActivated { id: workspace_id });
    }
    state.refresh_workspace_focus();
    Ok(())
}

pub type Command = (String, mpsc::Sender<String>);

/// IDs das notificações criadas pelo comando público (sem estado persistente).
static NOTIFICATION_SEQ: AtomicU64 = AtomicU64::new(1);

/// Conexões assinantes do stream PUSH (uma linha JSON por evento).
static SUBSCRIBERS: OnceLock<Mutex<Vec<UnixStream>>> = OnceLock::new();

fn subscribers() -> &'static Mutex<Vec<UnixStream>> {
    SUBSCRIBERS.get_or_init(|| Mutex::new(Vec::new()))
}

/// O stream deriva do IPC existente: `<ipc>.events`. Nenhuma env nova.
pub fn event_socket_path(ipc_path: &str) -> PathBuf {
    let mut path = PathBuf::from(ipc_path);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("aero-ipc")
        .to_owned();
    path.set_file_name(format!("{name}.events"));
    path
}

/// Aceita assinantes numa thread própria; o event loop só publica eventos.
pub fn spawn_event_stream(ipc_path: &str, sender: channel::Sender<Command>) -> io::Result<()> {
    let path = event_socket_path(ipc_path);
    // Socket stale de uma execução anterior não é estado do compositor atual.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    if let Ok(mut list) = subscribers().lock() {
                        list.push(stream);
                    }
                    // Replay do estado atual para quem acabou de conectar:
                    // eventos anteriores ao subscribe não podem ser recuperados.
                    let (tx, _rx) = mpsc::channel();
                    let _ = sender.send(("__subscribe__".to_owned(), tx));
                }
                Err(err) => tracing::warn!(%err, "event stream accept falhou"),
            }
        }
    });
    Ok(())
}

fn workspace_json(workspace: &Workspace) -> Value {
    json!({
        "id": workspace.id,
        "name": workspace.name,
        "window_ids": workspace.window_ids,
    })
}

fn output_json(output: &OutputInfo) -> Value {
    json!({
        "id": output.id,
        "name": output.name,
        "geometry": {
            "x": output.geometry.x,
            "y": output.geometry.y,
            "width": output.geometry.width,
            "height": output.geometry.height,
        },
        "scale": output.scale,
        "mode": {
            "width": output.mode.width,
            "height": output.mode.height,
            "refresh_millihz": output.mode.refresh_millihz,
        },
    })
}

/// Serialização manual no formato tagged-object que o ts-rs gera para `AeroEvent`.
fn event_json(event: &AeroEvent) -> Value {
    match event {
        AeroEvent::Snapshot { snapshot } => {
            // WindowInfo/Snapshot não derivam serde: montamos o valor no formato
            // que o ts-rs gera, sem exigir dependência nova na crate de IPC.
            let windows: Vec<Value> = snapshot
                .windows
                .iter()
                .map(|w| {
                    json!({
                        "id": w.id, "title": w.title, "x": w.x, "y": w.y,
                        "width": w.width, "height": w.height
                    })
                })
                .collect();
            json!({ "Snapshot": { "snapshot": {
                "revision": snapshot.revision, "windows": windows
            } } })
        }
        AeroEvent::Title { id, title } => json!({ "Title": { "id": id, "title": title } }),
        AeroEvent::Icon {
            id,
            name,
            has_pixels,
        } => {
            json!({ "Icon": { "id": id, "name": name, "has_pixels": has_pixels } })
        }
        AeroEvent::Focus { id } => json!({ "Focus": { "id": id } }),
        AeroEvent::Geometry {
            id,
            x,
            y,
            width,
            height,
        } => json!({
            "Geometry": { "id": id, "x": x, "y": y, "width": width, "height": height }
        }),
        AeroEvent::Minimized { id, minimized } => {
            json!({ "Minimized": { "id": id, "minimized": minimized } })
        }
        AeroEvent::Maximized { id, maximized } => {
            json!({ "Maximized": { "id": id, "maximized": maximized } })
        }
        AeroEvent::Fullscreen { id, fullscreen } => {
            json!({ "Fullscreen": { "id": id, "fullscreen": fullscreen } })
        }
        AeroEvent::Csd { id, client_frame } => {
            json!({ "Csd": { "id": id, "client_frame": client_frame } })
        }
        AeroEvent::FramePainted { id } => json!({ "FramePainted": { "id": id } }),
        AeroEvent::WorkspaceCreated { workspace } => {
            json!({ "WorkspaceCreated": { "workspace": workspace_json(workspace) } })
        }
        AeroEvent::WorkspaceActivated { id } => {
            json!({ "WorkspaceActivated": { "id": id } })
        }
        AeroEvent::WorkspaceRenamed { id, name } => {
            json!({ "WorkspaceRenamed": { "id": id, "name": name } })
        }
        AeroEvent::WorkspaceDestroyed { id } => {
            json!({ "WorkspaceDestroyed": { "id": id } })
        }
        AeroEvent::WindowMovedToWorkspace {
            window_id,
            workspace_id,
        } => {
            json!({ "WindowMovedToWorkspace": {
                "window_id": window_id, "workspace_id": workspace_id
            } })
        }
        AeroEvent::AppListUpdated { apps } => {
            let apps: Vec<Value> = apps.iter().map(app_json).collect();
            json!({ "AppListUpdated": { "apps": apps } })
        }
        AeroEvent::OutputAdded { output } => json!({
            "OutputAdded": { "output": output_json(output) }
        }),
        AeroEvent::OutputChanged { output } => json!({
            "OutputChanged": { "output": output_json(output) }
        }),
        AeroEvent::OutputRemoved { id } => json!({ "OutputRemoved": { "id": id } }),
        AeroEvent::KeyBind { id, keys } => json!({ "KeyBind": { "id": id, "keys": keys } }),
        AeroEvent::SurfaceCreated { surface } => json!({
            "SurfaceCreated": { "surface": surface_json(surface) }
        }),
        AeroEvent::SurfaceUpdated { surface } => json!({
            "SurfaceUpdated": { "surface": surface_json(surface) }
        }),
        AeroEvent::SurfaceDestroyed { id } => json!({
            "SurfaceDestroyed": { "id": id }
        }),
        AeroEvent::NotificationShown { notification } => json!({
            "NotificationShown": { "notification": notification_json(notification) }
        }),
        AeroEvent::NotificationDismissed { id } => json!({
            "NotificationDismissed": { "id": id }
        }),
        AeroEvent::DecorationRequest {
            id,
            app_id,
            title,
            mode,
            client_frame,
        } => json!({
            "DecorationRequest": {
                "id": id, "app_id": app_id, "title": title,
                "mode": mode, "client_frame": client_frame
            }
        }),
        AeroEvent::DecorationApplied {
            id,
            decision,
            fallback,
            reason,
        } => json!({
            "DecorationApplied": {
                "id": id, "decision": policy_name(*decision),
                "fallback": fallback, "reason": reason
            }
        }),
    }
}

fn app_json(app: &App) -> Value {
    json!({
        "id": app.id,
        "name": app.name,
        "icon": app.icon,
        "exec": app.exec,
        "categories": app.categories,
    })
}

fn surface_json(surface: &SurfaceSpec) -> Value {
    let rects = |regions: &Option<Vec<SurfaceInputRegion>>| {
        regions.as_ref().map(|rects| {
            rects
                .iter()
                .map(|r| json!({ "x": r.x, "y": r.y, "width": r.width, "height": r.height }))
                .collect::<Vec<Value>>()
        })
    };
    json!({
        "id": surface.id,
        "kind": surface_kind_name(surface.kind),
        "output": surface.output,
        "anchors": {
            "left": surface.anchors.left, "right": surface.anchors.right,
            "top": surface.anchors.top, "bottom": surface.anchors.bottom,
        },
        "margins": [
            surface.margins.left, surface.margins.right,
            surface.margins.top, surface.margins.bottom,
        ],
        "exclusive_zone": surface.exclusive_zone,
        "layer": surface_layer_name(surface.layer),
        "keyboard_mode": surface_keyboard_name(surface.keyboard_mode),
        "input_region": rects(&surface.input_region),
        "blur_region": rects(&surface.blur_region),
        "width": surface.width,
        "height": surface.height,
        "html": surface.html,
        "url": surface.url,
        "actions": surface.actions.as_ref().map(|actions| {
            actions
                .iter()
                .map(|action| match action {
                    SurfaceAction::LaunchApp => json!("LaunchApp"),
                })
                .collect::<Vec<Value>>()
        }),
    })
}

fn surface_kind_name(kind: SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::Desktop => "desktop",
        SurfaceKind::Panel => "panel",
        SurfaceKind::Overlay => "overlay",
        SurfaceKind::Frame => "frame",
    }
}

fn surface_layer_name(layer: SurfaceLayer) -> &'static str {
    match layer {
        SurfaceLayer::Background => "background",
        SurfaceLayer::Bottom => "bottom",
        SurfaceLayer::Top => "top",
        SurfaceLayer::Overlay => "overlay",
    }
}

fn surface_keyboard_name(mode: SurfaceKeyboardMode) -> &'static str {
    match mode {
        SurfaceKeyboardMode::None => "none",
        SurfaceKeyboardMode::OnDemand => "ondemand",
        SurfaceKeyboardMode::Exclusive => "exclusive",
    }
}

fn field_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("campo {field} ausente ou não-string"))
}

fn field_bool(value: &Value, field: &str, default: bool) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(default)
}

fn field_i32(value: &Value, field: &str, default: i32) -> Result<i32, String> {
    match value.get(field) {
        None => Ok(default),
        Some(Value::Number(number)) => number
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| format!("campo {field} fora da faixa i32")),
        Some(_) => Err(format!("campo {field} deveria ser número")),
    }
}

fn field_u32(value: &Value, field: &str, default: u32) -> Result<u32, String> {
    match value.get(field) {
        None => Ok(default),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| format!("campo {field} fora da faixa u32")),
        Some(_) => Err(format!("campo {field} deveria ser número")),
    }
}

fn optional_u32(value: &Value, field: &str) -> Result<Option<u32>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| format!("campo {field} fora da faixa u32")),
        Some(_) => Err(format!("campo {field} deveria ser número")),
    }
}

fn surface_regions(value: &Value, field: &str) -> Result<Option<Vec<SurfaceInputRegion>>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(items)) => {
            let mut rects = Vec::with_capacity(items.len());
            for item in items {
                rects.push(SurfaceInputRegion {
                    x: field_i32(item, "x", 0)?,
                    y: field_i32(item, "y", 0)?,
                    width: field_u32(item, "width", 0)?,
                    height: field_u32(item, "height", 0)?,
                });
            }
            Ok(Some(rects))
        }
        Some(_) => Err(format!("{field} deveria ser lista")),
    }
}

fn valid_dev_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://127.0.0.1:")
        .filter(|port| !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|port| port.parse::<u16>().ok())
        .is_some_and(|port| port > 0)
}

fn surface_url_allowed(address: &str, dev_origin: Option<&str>) -> bool {
    if address.starts_with("aero://") {
        return true;
    }
    let Some(origin) = dev_origin.filter(|origin| valid_dev_origin(origin)) else {
        return false;
    };
    address == origin
        || address
            .strip_prefix(origin)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

/// Converte o JSON de `surface-create/update` no tipo gerado por ts-rs.
/// Nenhum serde é adicionado à crate de IPC: a wire continua montada à mão.
fn parse_surface_spec(value: &Value) -> Result<SurfaceSpec, String> {
    let id = field_string(value, "id")?;
    if id.is_empty() || id.len() > 128 {
        return Err("id de superfície inválido".to_owned());
    }
    let kind = match field_string(value, "kind")?.as_str() {
        "desktop" => SurfaceKind::Desktop,
        "panel" => SurfaceKind::Panel,
        "overlay" => SurfaceKind::Overlay,
        "frame" => SurfaceKind::Frame,
        other => return Err(format!("kind desconhecido: {other}")),
    };
    let layer = match value
        .get("layer")
        .and_then(Value::as_str)
        .unwrap_or("background")
    {
        "background" => SurfaceLayer::Background,
        "bottom" => SurfaceLayer::Bottom,
        "top" => SurfaceLayer::Top,
        "overlay" => SurfaceLayer::Overlay,
        other => return Err(format!("layer desconhecido: {other}")),
    };
    let keyboard_mode = match value
        .get("keyboard_mode")
        .and_then(Value::as_str)
        .unwrap_or("none")
    {
        "none" => SurfaceKeyboardMode::None,
        "ondemand" => SurfaceKeyboardMode::OnDemand,
        "exclusive" => SurfaceKeyboardMode::Exclusive,
        other => return Err(format!("keyboard_mode desconhecido: {other}")),
    };
    let anchors = value.get("anchors");
    let margin_values = value
        .get("margins")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let margin_at = |index: usize| -> Result<i32, String> {
        margin_values
            .get(index)
            .and_then(Value::as_i64)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| "margins deveria ter 4 inteiros".to_owned())
    };
    let margins = SurfaceMargins {
        left: margin_at(0)?,
        right: margin_at(1)?,
        top: margin_at(2)?,
        bottom: margin_at(3)?,
    };
    let input_region = surface_regions(value, "input_region")?;
    let blur_region = surface_regions(value, "blur_region")?;
    let html = value.get("html").and_then(Value::as_str).map(str::to_owned);
    let url = value.get("url").and_then(Value::as_str).map(str::to_owned);
    if html.is_some() && url.is_some() {
        return Err("html e url são mutuamente exclusivos".to_owned());
    }
    if let Some(address) = url.as_deref() {
        let dev_origin = std::env::var("AERO_DEV_ORIGIN").ok();
        if !surface_url_allowed(address, dev_origin.as_deref()) {
            return Err("url externa não é permitida".to_owned());
        }
    }
    let actions = match value.get("actions") {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|item| match item.as_str() {
                    Some("LaunchApp") => Ok(SurfaceAction::LaunchApp),
                    _ => Err("action desconhecida".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(Value::Null) | None => None,
        Some(_) => return Err("actions deve ser um array".to_owned()),
    };
    Ok(SurfaceSpec {
        id,
        kind,
        output: optional_u32(value, "output")?,
        anchors: SurfaceAnchors {
            left: anchors
                .map(|a| field_bool(a, "left", false))
                .unwrap_or(false),
            right: anchors
                .map(|a| field_bool(a, "right", false))
                .unwrap_or(false),
            top: anchors
                .map(|a| field_bool(a, "top", false))
                .unwrap_or(false),
            bottom: anchors
                .map(|a| field_bool(a, "bottom", false))
                .unwrap_or(false),
        },
        margins,
        exclusive_zone: field_u32(value, "exclusive_zone", 0)?,
        layer,
        keyboard_mode,
        input_region,
        blur_region,
        width: optional_u32(value, "width")?,
        height: optional_u32(value, "height")?,
        html,
        url,
        actions,
    })
}

fn notification_json(notification: &Notification) -> Value {
    json!({
        "id": notification.id,
        "title": notification.title,
        "body": notification.body,
        "icon": notification.icon,
        "timeout_ms": notification.timeout_ms,
    })
}

fn next_json_string(
    values: &mut impl Iterator<Item = Result<Value, serde_json::Error>>,
    field: &str,
) -> Result<String, String> {
    match values.next() {
        Some(Ok(Value::String(value))) => Ok(value),
        Some(Ok(other)) => Err(format!("{field} deveria ser string: {other}")),
        Some(Err(err)) => Err(format!("{field} inválido: {err}")),
        None => Err(format!("falta {field}")),
    }
}

/// A forma do SDK usa valores JSON sequenciais (espaços dentro do texto não
/// quebram o parse); a forma pública aceita `notify <titulo> <corpo>` com o
/// resto da linha como corpo.
fn notification_from_command(raw: &str, default_id: String) -> Result<Notification, String> {
    let mut values = serde_json::Deserializer::from_str(raw).into_iter::<Value>();
    let title = match next_json_string(&mut values, "titulo") {
        Ok(value) => value,
        Err(_) => {
            let mut plain = raw.splitn(2, |c: char| c.is_whitespace());
            return Ok(Notification {
                id: default_id,
                title: plain.next().unwrap_or("").trim().to_owned(),
                body: plain.next().unwrap_or("").trim().to_owned(),
                icon: None,
                timeout_ms: 5000,
            });
        }
    };
    let body = next_json_string(&mut values, "corpo")?;
    let icon = match values.next() {
        Some(Ok(Value::String(value))) => Some(value),
        Some(Ok(Value::Null)) | None => None,
        Some(Ok(other)) => return Err(format!("icone deveria ser string ou null: {other}")),
        Some(Err(err)) => return Err(format!("icone inválido: {err}")),
    };
    let timeout_ms = match values.next() {
        Some(Ok(Value::Number(number))) => number
            .as_u64()
            .ok_or_else(|| "timeout-ms deveria ser inteiro positivo".to_owned())?,
        Some(Ok(other)) => return Err(format!("timeout-ms deveria ser número: {other}")),
        Some(Err(err)) => return Err(format!("timeout-ms inválido: {err}")),
        None => 5000,
    };
    let id = match values.next() {
        Some(Ok(Value::String(value))) => value,
        Some(Ok(other)) => return Err(format!("id deveria ser string: {other}")),
        Some(Err(err)) => return Err(format!("id inválido: {err}")),
        None => default_id,
    };
    if values.next().is_some() {
        return Err("notify recebeu argumentos demais".to_owned());
    }
    Ok(Notification {
        id,
        title,
        body,
        icon,
        timeout_ms,
    })
}

fn notification_id_arg(raw: &str) -> String {
    match serde_json::from_str::<String>(raw) {
        Ok(id) => id,
        Err(_) => raw.trim().to_owned(),
    }
}

fn policy_name(policy: DecorationPolicy) -> &'static str {
    match policy {
        DecorationPolicy::Server => "server",
        DecorationPolicy::Client => "client",
        DecorationPolicy::None => "none",
    }
}

/// Publica um evento para TODOS os assinantes conectados (sem bloquear o loop).
pub fn emit_event(event: AeroEvent) {
    let line = match serde_json::to_string(&event_json(&event)) {
        Ok(line) => line,
        Err(err) => {
            tracing::warn!(%err, "serialização de evento falhou");
            return;
        }
    };
    #[cfg(feature = "bench")]
    {
        let t = crate::frame_diagnostics::now_ns();
        eprintln!(
            "EVENT_PUSH t={t} kind={} bytes={}",
            event_name(&event),
            line.len()
        );
    }
    if let Ok(mut list) = subscribers().lock() {
        list.retain(|stream| {
            let mut clone = match stream.try_clone() {
                Ok(clone) => clone,
                Err(_) => return false,
            };
            writeln!(clone, "{line}").is_ok() && clone.flush().is_ok()
        });
    }
}

#[cfg(feature = "bench")]
fn event_name(event: &AeroEvent) -> &'static str {
    match event {
        AeroEvent::Snapshot { .. } => "snapshot",
        AeroEvent::Title { .. } => "title",
        AeroEvent::Icon { .. } => "icon",
        AeroEvent::Focus { .. } => "focus",
        AeroEvent::Geometry { .. } => "geometry",
        AeroEvent::Minimized { .. } => "minimized",
        AeroEvent::Maximized { .. } => "maximized",
        AeroEvent::Fullscreen { .. } => "fullscreen",
        AeroEvent::Csd { .. } => "csd",
        AeroEvent::FramePainted { .. } => "frame_painted",
        AeroEvent::WorkspaceCreated { .. } => "workspace_created",
        AeroEvent::WorkspaceActivated { .. } => "workspace_activated",
        AeroEvent::WorkspaceRenamed { .. } => "workspace_renamed",
        AeroEvent::WorkspaceDestroyed { .. } => "workspace_destroyed",
        AeroEvent::WindowMovedToWorkspace { .. } => "window_moved_to_workspace",
        AeroEvent::AppListUpdated { .. } => "app_list_updated",
        AeroEvent::OutputAdded { .. } => "output_added",
        AeroEvent::OutputChanged { .. } => "output_changed",
        AeroEvent::OutputRemoved { .. } => "output_removed",
        AeroEvent::KeyBind { .. } => "key_bind",
        AeroEvent::SurfaceCreated { .. } => "surface_created",
        AeroEvent::SurfaceUpdated { .. } => "surface_updated",
        AeroEvent::SurfaceDestroyed { .. } => "surface_destroyed",
        AeroEvent::NotificationShown { .. } => "notification_shown",
        AeroEvent::NotificationDismissed { .. } => "notification_dismissed",
        AeroEvent::DecorationRequest { .. } => "decoration_request",
        AeroEvent::DecorationApplied { .. } => "decoration_applied",
    }
}

/// Lança a thread do socket IPC. `sender` entrega os comandos ao event loop.
pub fn spawn(path: &str, sender: channel::Sender<Command>) -> io::Result<()> {
    let listener = UnixListener::bind(path)?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let Ok(mut writer) = stream.try_clone() else {
                continue;
            };
            for line in std::io::BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
            {
                let (tx, rx) = mpsc::channel();
                if sender.send((line, tx)).is_err() {
                    break;
                }
                let resp = rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap_or_else(|_| "ERR timeout".to_owned());
                let _ = writeln!(writer, "{resp}");
                let _ = writer.flush();
            }
        }
    });
    Ok(())
}

impl AeroState {
    /// Estado tipado de um output, usando o Space e o modo físico atuais.
    pub fn output_info(&self, id: u32) -> Option<OutputInfo> {
        let output = self.outputs.get(id as usize)?;
        let geometry = self.space.output_geometry(output)?;
        let mode = output.current_mode()?;
        Some(OutputInfo {
            id,
            name: output.name(),
            geometry: OutputGeometry {
                x: geometry.loc.x,
                y: geometry.loc.y,
                width: geometry.size.w.max(0) as u32,
                height: geometry.size.h.max(0) as u32,
            },
            scale: output.current_scale().fractional_scale(),
            mode: OutputMode {
                width: mode.size.w.max(0) as u32,
                height: mode.size.h.max(0) as u32,
                refresh_millihz: mode.refresh.max(0) as u32,
            },
        })
    }

    /// Snapshot tipado dos dados básicos das janelas (mesma fonte do `list`).
    pub fn snapshot(&self) -> Snapshot {
        let mut ids: Vec<u32> = self.window_surfaces.keys().copied().collect();
        ids.sort_unstable();
        let mut windows = Vec::new();
        for id in ids {
            let Some(window) = self.window_for_id(id) else {
                continue;
            };
            let Some(geo) = self.space.element_geometry(window) else {
                continue;
            };
            windows.push(WindowInfo {
                id,
                title: window_title(window),
                x: geo.loc.x,
                y: geo.loc.y,
                width: geo.size.w.max(0) as u32,
                height: geo.size.h.max(0) as u32,
            });
        }
        Snapshot {
            revision: self.event_revision,
            windows,
        }
    }

    /// Publica o estado atual de UMA janela (no `associate` do frame, que
    /// chega depois dos eventos de map/title — sem pedir `list` de novo).
    pub fn emit_window_state(&self, id: u32) {
        if let Some(window) = self.window_for_id(id) {
            emit_event(AeroEvent::Title {
                id,
                title: window_title(window),
            });
        }
        if let Some(surface) = self.window_surfaces.get(&id) {
            let name = icon_name(surface);
            let has_pixels = icon_pixels(surface).is_some();
            if name.is_some() || has_pixels {
                emit_event(AeroEvent::Icon {
                    id,
                    name,
                    has_pixels,
                });
            }
            emit_event(AeroEvent::Csd {
                id,
                client_frame: client_draws_frame(surface),
            });
        }
    }

    /// Publica o snapshot no stream, com revisão monotônica.
    pub fn emit_snapshot(&mut self) {
        self.event_revision = self.event_revision.saturating_add(1);
        let snapshot = self.snapshot();
        emit_event(AeroEvent::Snapshot { snapshot });
    }

    /// Executa um comando de janela no event loop e devolve a resposta textual.
    pub fn window_command(&mut self, cmd: &str) -> String {
        let mut parts = cmd.split_whitespace();
        let op = parts.next().unwrap_or("");
        // `launch` é a única operação cujo segundo token é textual; consumi-lo
        // no parser numérico comum faria o app_id virar o workspace (ou sumir).
        let id = if op == "launch" {
            None
        } else {
            parts.next().and_then(|v| v.parse::<usize>().ok())
        };
        match op {
            // Comando INTERNO do subscribe do stream: publica o estado atual
            // para o assinante que acabou de conectar (não é protocolo público).
            // Além do snapshot, reenvia as decisões já tomadas: o painel é um
            // WebView que só instala `__aeroEvent` depois de carregar o bundle,
            // então os DecorationApplied do map das primeiras janelas chegam
            // antes dele e se perdiam (indicador ficava "default" até editar).
            "__subscribe__" => {
                self.emit_snapshot();
                let mut window_ids: Vec<_> = self.window_surfaces.keys().copied().collect();
                window_ids.sort_unstable();
                for id in window_ids {
                    let state = frame_state_for(self, id);
                    emit_event(AeroEvent::Minimized {
                        id,
                        minimized: self.minimized_windows.contains(&id),
                    });
                    emit_event(AeroEvent::Maximized {
                        id,
                        maximized: state
                            .contains(crate::aero_frame_protocol::aero_frame_v1::State::Maximized),
                    });
                    emit_event(AeroEvent::Fullscreen {
                        id,
                        fullscreen: state
                            .contains(crate::aero_frame_protocol::aero_frame_v1::State::Fullscreen),
                    });
                }
                for workspace in self.workspaces.list() {
                    emit_event(AeroEvent::WorkspaceCreated {
                        workspace: workspace.clone(),
                    });
                }
                emit_event(AeroEvent::WorkspaceActivated {
                    id: self.workspaces.active_id(),
                });
                emit_event(AeroEvent::AppListUpdated {
                    apps: self
                        .installed_apps
                        .iter()
                        .map(|app| app.info.clone())
                        .collect(),
                });
                for id in 0..self.outputs.len() as u32 {
                    if let Some(output) = self.output_info(id) {
                        emit_event(AeroEvent::OutputAdded { output });
                    }
                }
                // Quem subscribe depois (host/painel) precisa reconstruir as
                // superfícies já declaradas, mesmo padrão do replay de decoração.
                for surface in self.surfaces.values() {
                    emit_event(AeroEvent::SurfaceCreated {
                        surface: surface.clone(),
                    });
                }
                for (&id, &decision) in self.decorations.iter() {
                    emit_event(AeroEvent::DecorationApplied {
                        id,
                        decision,
                        fallback: false,
                        reason: None,
                    });
                }
                "OK".to_owned()
            }
            // F2-decoration-policy: runtime Bun opcional informa que está vivo e
            // responde `policy <id> server|client|none` no MESMO socket IPC.
            "policy-ready" => {
                self.policy_runtime = true;
                tracing::info!("POLICY runtime conectado no IPC");
                "OK".to_owned()
            }
            // F2-decoration-policy-hotreload: as regras TS mudaram. Reemitimos
            // `DecorationRequest` para as janelas já decididas; o runtime
            // responde com o MESMO `policy <id>` de sempre (sem nova rota).
            "policy-reload" => {
                if !self.policy_runtime {
                    return "ERR runtime de política não conectado".to_owned();
                }
                // TODAS as janelas existentes, não só `decorations`: uma decisão
                // que expirou (reason=timeout) cai em `policy_fallback` e NÃO
                // entra no mapa — com o código antigo ela nunca mais era
                // reconsultada e ficava sem decoração para sempre (juiz visual).
                let mut ids: Vec<u32> = self.window_surfaces.keys().copied().collect();
                for id in self.pending_policies.keys().copied() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                for id in self.decorations.keys().copied() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
                ids.sort_unstable();
                for id in &ids {
                    self.decorations.remove(id);
                    self.pending_policies.remove(id);
                    self.request_decoration_policy(*id);
                }
                tracing::info!(count = ids.len(), "POLICY reload: janelas reconsultadas");
                "OK".to_owned()
            }
            "policy" => {
                let (Some(id), Some(raw)) = (id, parts.next()) else {
                    return "ERR uso: policy <id> <server|client|none>".to_owned();
                };
                let decision = match raw {
                    "server" => DecorationPolicy::Server,
                    "client" => DecorationPolicy::Client,
                    "none" => DecorationPolicy::None,
                    other => return format!("ERR decisão desconhecida: {other}"),
                };
                if self.pending_policies.contains_key(&(id as u32)) {
                    self.apply_decoration_policy(id as u32, decision);
                    "OK".to_owned()
                } else {
                    "ERR janela sem pedido de política pendente".to_owned()
                }
            }
            "workspace-list" => {
                let mut out = String::new();
                for workspace in self.workspaces.list() {
                    let windows = workspace
                        .window_ids
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    let name = serde_json::to_string(&workspace.name)
                        .unwrap_or_else(|_| "\"Workspace\"".to_owned());
                    out.push_str(&format!(
                        "WS {} active={} windows={} {}\n",
                        workspace.id,
                        if workspace.id == self.workspaces.active_id() {
                            1
                        } else {
                            0
                        },
                        windows,
                        name,
                    ));
                }
                out.push('.');
                out
            }
            "workspace-create" => {
                let raw = cmd.strip_prefix("workspace-create").unwrap_or("").trim();
                let name = if raw.is_empty() {
                    None
                } else {
                    Some(serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned()))
                };
                let workspace = self.workspaces.create(name);
                let id = workspace.id;
                emit_event(AeroEvent::WorkspaceCreated { workspace });
                format!("WORKSPACE {id}")
            }
            "workspace-rename" => {
                let Some(id) = id.map(|value| value as u32) else {
                    return "ERR uso: workspace-rename <id> <nome>".to_owned();
                };
                let raw = cmd.splitn(3, ' ').nth(2).unwrap_or("").trim();
                let name = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned());
                match self.workspaces.rename(id, name.clone()) {
                    Ok(()) => {
                        emit_event(AeroEvent::WorkspaceRenamed { id, name });
                        "OK".to_owned()
                    }
                    Err(err) => err,
                }
            }
            "workspace-activate" => {
                let Some(id) = id.map(|value| value as u32) else {
                    return "ERR workspace inexistente".to_owned();
                };
                match self.workspaces.activate(id) {
                    Ok(()) => {
                        emit_event(AeroEvent::WorkspaceActivated { id });
                        self.refresh_workspace_focus();
                        "OK".to_owned()
                    }
                    Err(err) => err,
                }
            }
            "window-workspace" => {
                let (Some(window_id), Some(workspace_id)) = (
                    id.map(|value| value as u32),
                    parts.next().and_then(|value| value.parse::<u32>().ok()),
                ) else {
                    return "ERR uso: window-workspace <window_id> <workspace_id>".to_owned();
                };
                if self.window_for_id(window_id).is_none() {
                    return "ERR janela inexistente".to_owned();
                }
                match self.workspaces.move_window(window_id, workspace_id) {
                    Ok(()) => {
                        emit_event(AeroEvent::WindowMovedToWorkspace {
                            window_id,
                            workspace_id,
                        });
                        self.refresh_workspace_focus();
                        "OK".to_owned()
                    }
                    Err(err) => err,
                }
            }
            "workspace-destroy" => {
                let Some(id) = id.map(|value| value as u32) else {
                    return "ERR workspace inexistente".to_owned();
                };
                match self.workspaces.destroy(id) {
                    Ok((_, activated)) => {
                        emit_event(AeroEvent::WorkspaceDestroyed { id });
                        if let Some(active_id) = activated {
                            emit_event(AeroEvent::WorkspaceActivated { id: active_id });
                        }
                        self.refresh_workspace_focus();
                        "OK".to_owned()
                    }
                    Err(err) => err,
                }
            }
            "launch" => {
                let Some(app_key) = parts.next() else {
                    return "ERR uso: launch <app_id> [workspace_id]".to_owned();
                };
                let requested_workspace = match parts.next() {
                    Some(value) => match value.parse::<u32>() {
                        Ok(value) => Some(value),
                        Err(_) => return "ERR uso: launch <app_id> [workspace_id]".to_owned(),
                    },
                    None => None,
                };
                if parts.next().is_some() {
                    return "ERR uso: launch <app_id> [workspace_id]".to_owned();
                }
                let Some(app) = self
                    .installed_apps
                    .iter()
                    .find(|app| app.info.id == app_key)
                    .cloned()
                else {
                    return format!("ERR aplicativo inexistente: {app_key}");
                };
                if let Some(workspace_id) = requested_workspace {
                    if !self
                        .workspaces
                        .list()
                        .iter()
                        .any(|workspace| workspace.id == workspace_id)
                    {
                        return "ERR workspace inexistente".to_owned();
                    }
                }

                let mut ids: Vec<u32> = self.window_surfaces.keys().copied().collect();
                ids.sort_unstable();
                let existing = ids.into_iter().find(|window_id| {
                    self.window_for_id(*window_id)
                        .map(|window| app.matches_window_app_id(&app_id_for(window)))
                        .unwrap_or(false)
                });

                if let Some(window_id) = existing {
                    let current = self.workspaces.workspace_of_window(window_id);
                    let target = requested_workspace
                        .or(current)
                        .unwrap_or_else(|| self.workspaces.active_id());
                    if current != Some(target) {
                        if let Err(err) = self.workspaces.move_window(window_id, target) {
                            return err;
                        }
                        emit_event(AeroEvent::WindowMovedToWorkspace {
                            window_id,
                            workspace_id: target,
                        });
                    }
                    if let Err(err) = activate_workspace(self, target) {
                        return err;
                    }
                    return match focus_window(self, window_id) {
                        Ok(()) => {
                            tracing::info!(app_id = %app.info.id, window_id, target, "APP_FOCUS");
                            "OK".to_owned()
                        }
                        Err(err) => err,
                    };
                }

                let target = requested_workspace.unwrap_or_else(|| self.workspaces.active_id());
                match crate::supervisor::spawn_public_client(&self.socket_name, &app.info.exec) {
                    Ok(pid) => match activate_workspace(self, target) {
                        Ok(()) => {
                            tracing::info!(app_id = %app.info.id, pid, target, "APP_LAUNCH");
                            "OK".to_owned()
                        }
                        Err(err) => err,
                    },
                    Err(err) => format!("ERR falha ao lançar {}: {err}", app.info.id),
                }
            }
            "output-list" => {
                let mut out = String::new();
                for id in 0..self.outputs.len() as u32 {
                    if let Some(output) = self.output_info(id) {
                        out.push_str(&format!("O {}\n", output_json(&output)));
                    }
                }
                out.push('.');
                out
            }
            "output-get" => {
                let Some(id) = id.and_then(|id| u32::try_from(id).ok()) else {
                    return "ERR uso: output-get <id>".to_owned();
                };
                match self.output_info(id) {
                    Some(output) => format!("O {}\nOK", output_json(&output)),
                    None => format!("ERR output inexistente: {id}"),
                }
            }
            "input-bind" => {
                let raw = cmd.strip_prefix("input-bind").unwrap_or("").trim();
                let value: Value = match serde_json::from_str(raw) {
                    Ok(value) => value,
                    Err(err) => return format!("ERR input-bind: {err}"),
                };
                let (id, keys) = match (field_string(&value, "id"), field_string(&value, "keys")) {
                    (Ok(id), Ok(keys)) if !id.trim().is_empty() => (id, keys),
                    _ => return "ERR input-bind exige id não vazio e keys".to_owned(),
                };
                let chord = match parse_key_binding(&keys) {
                    Ok(chord) => chord,
                    Err(err) => return format!("ERR input-bind: {err}"),
                };
                if self.key_binds.contains_key(&id) {
                    return format!("ERR keybind já existe: {id}");
                }
                if self
                    .key_binds
                    .values()
                    .any(|binding| binding.chord == chord)
                {
                    return format!("ERR atalho já registrado: {keys}");
                }
                self.key_binds.insert(id, RegisteredKeyBind { keys, chord });
                "OK".to_owned()
            }
            "input-unbind" => {
                let raw = cmd.strip_prefix("input-unbind").unwrap_or("").trim();
                let id = notification_id_arg(raw);
                if id.is_empty() {
                    return "ERR uso: input-unbind <id-json>".to_owned();
                }
                if self.key_binds.remove(&id).is_none() {
                    return format!("ERR keybind inexistente: {id}");
                }
                "OK".to_owned()
            }
            "input-list" => {
                let mut ids: Vec<_> = self.key_binds.keys().cloned().collect();
                ids.sort();
                let mut out = String::new();
                for id in ids {
                    if let Some(binding) = self.key_binds.get(&id) {
                        out.push_str(&format!(
                            "B {}\n",
                            json!({ "id": id, "keys": binding.keys })
                        ));
                    }
                }
                out.push('.');
                out
            }
            // F2-surface-host: o runtime declara superfícies por JSON numa linha;
            // o host de superfícies consome os eventos no stream PUSH existente.
            "surface-list" => {
                let mut ids: Vec<_> = self.surfaces.keys().cloned().collect();
                ids.sort();
                let mut out = String::new();
                for id in ids {
                    if let Some(surface) = self.surfaces.get(&id) {
                        out.push_str(&format!("S {}\n", surface_json(surface)));
                    }
                }
                out.push('.');
                out
            }
            "surface-create" => {
                let raw = cmd.strip_prefix("surface-create").unwrap_or("").trim();
                let value: Value = match serde_json::from_str(raw) {
                    Ok(value) => value,
                    Err(err) => return format!("ERR surface-create: {err}"),
                };
                let spec = match parse_surface_spec(&value) {
                    Ok(spec) => spec,
                    Err(err) => return format!("ERR surface-create: {err}"),
                };
                if self.surfaces.contains_key(&spec.id) {
                    return format!("ERR superfície já existe: {}", spec.id);
                }
                if spec
                    .output
                    .is_some_and(|id| id as usize >= self.outputs.len())
                {
                    return format!("ERR output inexistente: {:?}", spec.output);
                }
                self.surfaces.insert(spec.id.clone(), spec.clone());
                emit_event(AeroEvent::SurfaceCreated { surface: spec });
                "OK".to_owned()
            }
            "surface-update" => {
                let raw = cmd.strip_prefix("surface-update").unwrap_or("").trim();
                let value: Value = match serde_json::from_str(raw) {
                    Ok(value) => value,
                    Err(err) => return format!("ERR surface-update: {err}"),
                };
                let Some(id) = value.get("id").and_then(Value::as_str) else {
                    return "ERR surface-update: falta id".to_owned();
                };
                let Some(existing) = self.surfaces.get(id).cloned() else {
                    return format!("ERR superfície inexistente: {id}");
                };
                let Some(incoming) = value.as_object() else {
                    return "ERR surface-update: esperado objeto JSON".to_owned();
                };
                let mut merged = match surface_json(&existing).as_object().cloned() {
                    Some(map) => map,
                    None => return "ERR falha ao mesclar superfície".to_owned(),
                };
                for (key, field) in incoming {
                    merged.insert(key.clone(), field.clone());
                }
                let spec = match parse_surface_spec(&Value::Object(merged)) {
                    Ok(spec) => spec,
                    Err(err) => return format!("ERR surface-update: {err}"),
                };
                if spec
                    .output
                    .is_some_and(|id| id as usize >= self.outputs.len())
                {
                    return format!("ERR output inexistente: {:?}", spec.output);
                }
                self.surfaces.insert(id.to_owned(), spec.clone());
                emit_event(AeroEvent::SurfaceUpdated { surface: spec });
                "OK".to_owned()
            }
            "surface-destroy" => {
                let id = cmd
                    .strip_prefix("surface-destroy")
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"');
                if self.surfaces.remove(id).is_none() {
                    return format!("ERR superfície inexistente: {id}");
                }
                emit_event(AeroEvent::SurfaceDestroyed { id: id.to_owned() });
                "OK".to_owned()
            }
            // F2-notifications: o compositor é apenas o mecanismo. O estado ativo
            // vive no SDK/painel; aqui só publicamos shown/dismiss no stream.
            "notify" => {
                let raw = cmd.strip_prefix("notify").unwrap_or("").trim();
                let generated = format!("ipc-{}", NOTIFICATION_SEQ.fetch_add(1, Ordering::Relaxed));
                match notification_from_command(raw, generated) {
                    Ok(notification) => {
                        emit_event(AeroEvent::NotificationShown { notification });
                        "OK".to_owned()
                    }
                    Err(err) => format!("ERR uso: notify: {err}"),
                }
            }
            "dismiss-notification" => {
                let raw = cmd
                    .strip_prefix("dismiss-notification")
                    .unwrap_or("")
                    .trim();
                if raw.is_empty() {
                    return "ERR uso: dismiss-notification <id>".to_owned();
                }
                emit_event(AeroEvent::NotificationDismissed {
                    id: notification_id_arg(raw),
                });
                "OK".to_owned()
            }
            "list" => {
                let mut out = String::new();
                // `space.elements()` é ordem z e muda a cada raise_element: usar o
                // enumerate aqui faria `--close <id>` fechar a janela errada (02b).
                // Iteramos o mapa lógico e resolvemos via window_for_id.
                let mut ids: Vec<u32> = self.window_surfaces.keys().copied().collect();
                ids.sort_unstable();
                for id in ids {
                    let Some(window) = self.window_for_id(id) else {
                        continue;
                    };
                    let Some(geo) = self.space.element_geometry(window) else {
                        continue;
                    };
                    let title = serde_json::to_string(&window_title(window))
                        .unwrap_or_else(|_| "\"\"".to_owned());
                    let app_id = serde_json::to_string(&app_id_for(window))
                        .unwrap_or_else(|_| "\"\"".to_owned());
                    let transient = window.toplevel().and_then(|t| t.parent()).is_some();
                    // Campos novos no FIM da linha: leitores antigos usam
                    // splitn(7, ' ') e continuam funcionando (F2 consumirá estado).
                    let state = frame_state_for(self, id);
                    let mode = if state
                        .contains(crate::aero_frame_protocol::aero_frame_v1::State::Fullscreen)
                    {
                        "fullscreen"
                    } else if state
                        .contains(crate::aero_frame_protocol::aero_frame_v1::State::Maximized)
                    {
                        "maximized"
                    } else {
                        "normal"
                    };
                    out.push_str(&format!(
                        "W {} {} {} {} {} {} {} csd={} transient={} mode={}\n",
                        id,
                        geo.loc.x,
                        geo.loc.y,
                        geo.size.w,
                        geo.size.h,
                        title,
                        app_id,
                        if window
                            .toplevel()
                            .map(|t| client_draws_frame(t.wl_surface()))
                            .unwrap_or(false)
                        {
                            1
                        } else {
                            0
                        },
                        if transient { 1 } else { 0 },
                        mode
                    ));
                }
                out.push('.');
                out
            }
            "focus" => {
                let Some(window_id) = id.map(|value| value as u32) else {
                    return "ERR janela inexistente".to_owned();
                };
                if self.window_for_id(window_id).is_none() {
                    return "ERR janela inexistente".to_owned();
                }
                if !self.workspaces.window_visible(window_id) {
                    return "ERR janela fora do workspace ativo".to_owned();
                }
                self.set_window_minimized(window_id, false);
                match focus_window(self, window_id) {
                    Ok(()) => "OK".to_owned(),
                    Err(err) => err,
                }
            }
            "close" => {
                let Some(window) = id.and_then(|i| self.window_for_id(i as u32).cloned()) else {
                    return "ERR janela inexistente".to_owned();
                };
                if let Some(toplevel) = window.toplevel() {
                    toplevel.send_close();
                }
                "OK".to_owned()
            }
            "minimize" => {
                let Some(window_id) = id.map(|value| value as u32) else {
                    return "ERR janela inexistente".to_owned();
                };
                if self.set_window_minimized(window_id, true) {
                    "OK".to_owned()
                } else {
                    "ERR janela inexistente".to_owned()
                }
            }
            "fullscreen" => {
                let Some(window_id) = id.map(|value| value as u32) else {
                    return "ERR janela inexistente".to_owned();
                };
                if self.window_for_id(window_id).is_none() {
                    return "ERR janela inexistente".to_owned();
                }
                let target = !frame_state_for(self, window_id)
                    .contains(crate::aero_frame_protocol::aero_frame_v1::State::Fullscreen);
                if crate::protocols::toplevel_states::apply_fullscreen(self, window_id, target) {
                    "OK".to_owned()
                } else {
                    "ERR sem output para fullscreen".to_owned()
                }
            }
            "move" => {
                let (Some(window_id), Some(x), Some(y)) = (
                    id.map(|value| value as u32),
                    parts.next().and_then(|value| value.parse::<i32>().ok()),
                    parts.next().and_then(|value| value.parse::<i32>().ok()),
                ) else {
                    return "ERR uso: move <id> <x> <y>".to_owned();
                };
                let Some(window) = self.window_for_id(window_id).cloned() else {
                    return "ERR janela inexistente".to_owned();
                };
                self.space.map_element(window, (x, y), false);
                self.dirty = true;
                self.emit_window_geometry(window_id);
                "OK".to_owned()
            }
            "resize" => {
                let (Some(window_id), Some(width), Some(height)) = (
                    id.map(|value| value as u32),
                    parts.next().and_then(|value| value.parse::<i32>().ok()),
                    parts.next().and_then(|value| value.parse::<i32>().ok()),
                ) else {
                    return "ERR uso: resize <id> <width> <height>".to_owned();
                };
                if width <= 0 || height <= 0 {
                    return "ERR resize exige dimensões positivas".to_owned();
                }
                let Some(window) = self.window_for_id(window_id).cloned() else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(toplevel) = window.toplevel() else {
                    return "ERR sem toplevel".to_owned();
                };
                let size = crate::protocols::toplevel_states::clamp_size(
                    toplevel.wl_surface(),
                    (width, height).into(),
                    false,
                );
                toplevel.with_pending_state(|state| state.size = Some(size));
                let _ = toplevel.send_configure();
                self.dirty = true;
                "OK".to_owned()
            }
            "begin-move" | "begin-resize" => {
                let Some(window_id) = id.map(|value| value as u32) else {
                    return "ERR janela inexistente".to_owned();
                };
                if self.window_for_id(window_id).is_none() || !self.window_visible(window_id) {
                    return "ERR janela inexistente ou invisível".to_owned();
                }
                if !self.primary_button_pressed {
                    return "ERR begin-move/begin-resize exige botão primário pressionado"
                        .to_owned();
                }
                let Some(serial) = self.last_button_serial else {
                    return "ERR serial de botão ausente".to_owned();
                };
                if self
                    .seat
                    .get_pointer()
                    .is_some_and(|pointer| pointer.is_grabbed() && !pointer.has_grab(serial.into()))
                {
                    return "ERR já existe um grab de ponteiro ativo".to_owned();
                }
                let resize = if op == "begin-move" {
                    None
                } else {
                    use crate::aero_frame_protocol::aero_frame_v1::ResizeEdge;
                    match parts.next() {
                        Some("top") => Some(ResizeEdge::Top),
                        Some("bottom") => Some(ResizeEdge::Bottom),
                        Some("left") => Some(ResizeEdge::Left),
                        Some("right") => Some(ResizeEdge::Right),
                        Some("top-left") => Some(ResizeEdge::TopLeft),
                        Some("bottom-left") => Some(ResizeEdge::BottomLeft),
                        Some("top-right") => Some(ResizeEdge::TopRight),
                        Some("bottom-right") => Some(ResizeEdge::BottomRight),
                        _ => return "ERR edge inválido para begin-resize".to_owned(),
                    }
                };
                if crate::grab::start_geometry_grab(self, window_id, serial, resize) {
                    self.dirty = true;
                    "OK".to_owned()
                } else {
                    "ERR não foi possível iniciar grab".to_owned()
                }
            }
            "thumbnail" => "ERR thumbnail deferido para screencopy/F7".to_owned(),
            "maximize" => {
                // Mesma mecânica da 02r, agora compartilhada com o pedido
                // xdg_toplevel.set_maximized (F1-xdg-toplevel-states).
                let Some(id) = id else {
                    return "ERR janela inexistente".to_owned();
                };
                if self.window_for_id(id as u32).is_none() {
                    return "ERR janela inexistente".to_owned();
                }
                let target = !frame_state_for(self, id as u32)
                    .contains(crate::aero_frame_protocol::aero_frame_v1::State::Maximized);
                if crate::protocols::toplevel_states::apply_maximize(self, id as u32, target) {
                    "OK".to_owned()
                } else {
                    "ERR sem área útil".to_owned()
                }
            }
            "icon" => {
                let Some(id) = id else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(window) = self.window_for_id(id as u32).cloned() else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(toplevel) = window.toplevel() else {
                    return "ERR sem toplevel".to_owned();
                };
                let surface = toplevel.wl_surface();
                match icon_pixels(surface) {
                    Some((w, h, scale, format, pixels)) => format!(
                        "ICON {id} {w} {h} {scale} {format} {}",
                        base64_encode(&pixels)
                    ),
                    None => match icon_name(surface) {
                        Some(name) => format!(
                            "ICON {id} NAME {}",
                            serde_json::to_string(&name).unwrap_or_else(|_| "\"\"".to_owned())
                        ),
                        None => "NONE".to_owned(),
                    },
                }
            }
            "csd" => {
                let Some(id) = id else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(window) = self.window_for_id(id as u32).cloned() else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(toplevel) = window.toplevel() else {
                    return "ERR sem toplevel".to_owned();
                };
                let explicit = client_draws_frame(toplevel.wl_surface());
                // GTK4/gedit NÃO pede ClientSide, mas desenha sombra fora da
                // window geometry (Domicile: window_geometry.rs). Origem != 0
                // é o sinal observável no caso real — SÓ enquanto não há decisão
                // explícita da política (F2). O shell pergunta `csd` para decidir
                // se esconde a barra: se a geometria vencesse, um `Client→Server`
                // ao vivo nunca restaurava a moldura (juiz visual 2026-11-24).
                let geo = window.geometry();
                let from_geometry =
                    self.decoration(id as u32).is_none() && (geo.loc.x != 0 || geo.loc.y != 0);
                #[cfg(feature = "bench")]
                eprintln!(
                    "CSD_GEOM t={} window_id={id} loc=({},{}) size=({},{}) explicit={explicit} from_geometry={from_geometry}",
                    crate::frame_diagnostics::now_ns(),
                    geo.loc.x,
                    geo.loc.y,
                    geo.size.w,
                    geo.size.h,
                );
                #[cfg(feature = "bench")]
                if from_geometry && !explicit {
                    eprintln!(
                        "CSD_DETECT t={} window_id={id} client_frame=true reason=geometry loc=({},{})",
                        crate::frame_diagnostics::now_ns(),
                        geo.loc.x,
                        geo.loc.y
                    );
                }
                format!("CSD {id} {}", if explicit || from_geometry { 1 } else { 0 })
            }
            _ => "ERR comando desconhecido".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{base64_encode, parse_surface_spec, surface_json, surface_url_allowed};

    #[test]
    fn parse_surface_spec_valida_kind_e_rejeita_url_externa() {
        let value: serde_json::Value = serde_json::from_str(
            r#"{"id":"desk","kind":"desktop","html":"<body></body>",
                "margins":[0,0,0,0],"anchors":{"left":true,"right":true,"top":true,"bottom":true}}"#,
        )
        .unwrap();
        let spec = parse_surface_spec(&value).unwrap();
        assert_eq!(spec.id, "desk");
        assert_eq!(surface_kind_name_local(spec.kind), "desktop");
        assert_eq!(spec.margins.left, 0);
        assert_eq!(spec.blur_region, None);
        assert_eq!(spec.width, None);

        let with_regions: serde_json::Value = serde_json::from_str(
            r#"{"id":"panel","kind":"panel","height":56,"margins":[0,0,0,0],
                "input_region":[{"x":0,"y":0,"width":640,"height":56}],
                "blur_region":[{"x":0,"y":0,"width":640,"height":56}]}"#,
        )
        .unwrap();
        let spec = parse_surface_spec(&with_regions).unwrap();
        assert_eq!(spec.height, Some(56));
        assert_eq!(spec.input_region.unwrap()[0].width, 640);
        assert_eq!(spec.blur_region.unwrap()[0].height, 56);

        let external: serde_json::Value =
            serde_json::from_str(r#"{"id":"x","kind":"overlay","url":"https://example.com"}"#)
                .unwrap();
        assert!(parse_surface_spec(&external).is_err());
    }

    #[test]
    fn parse_surface_spec_aceita_apenas_actions_conhecidas() {
        let base = r#"{"id":"menu","kind":"overlay","margins":[0,0,0,0],"actions":["#;
        let allowed: serde_json::Value =
            serde_json::from_str(&format!("{base}\"LaunchApp\"]}}")).unwrap();
        assert_eq!(
            parse_surface_spec(&allowed).unwrap().actions.unwrap().len(),
            1
        );

        let unknown: serde_json::Value =
            serde_json::from_str(&format!("{base}\"FocusWindow\"]}}")).unwrap();
        assert!(parse_surface_spec(&unknown).is_err());

        let absent: serde_json::Value = serde_json::from_str(
            r#"{"id":"panel","kind":"panel","margins":[0,0,0,0],"actions":null}"#,
        )
        .unwrap();
        assert_eq!(parse_surface_spec(&absent).unwrap().actions, None);

        let spec = parse_surface_spec(&allowed).unwrap();
        assert_eq!(surface_json(&spec)["actions"][0], "LaunchApp");
    }

    fn surface_kind_name_local(kind: aeroshell_ipc::types::SurfaceKind) -> &'static str {
        use aeroshell_ipc::types::SurfaceKind;
        match kind {
            SurfaceKind::Desktop => "desktop",
            SurfaceKind::Panel => "panel",
            SurfaceKind::Overlay => "overlay",
            SurfaceKind::Frame => "frame",
        }
    }

    #[test]
    fn url_dev_so_aceita_loopback_configurado() {
        let dev = Some("http://127.0.0.1:4173");
        assert!(surface_url_allowed("aero://surface/panel/", None));
        assert!(!surface_url_allowed("http://127.0.0.1:4173/panel", None));
        assert!(surface_url_allowed("http://127.0.0.1:4173/panel?v=2", dev));
        for url in [
            "http://127.0.0.1:4174/panel",
            "http://127.0.0.1:4173.evil.test/panel",
            "http://localhost:4173/panel",
            "https://127.0.0.1:4173/panel",
            "https://example.com/",
        ] {
            assert!(!surface_url_allowed(url, dev), "aceitou {url}");
        }
        assert!(!surface_url_allowed(
            "http://127.0.0.1:4173/panel",
            Some("https://example.com")
        ));
    }

    #[test]
    fn base64_sem_dependencia() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(&[0, 0, 255, 255]), "AAD//w==");
    }
}
