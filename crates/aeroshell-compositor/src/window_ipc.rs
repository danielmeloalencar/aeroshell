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
//! Cada request é despachado no event loop (canal calloop) e respondido por um
//! oneshot mpsc, para que `list` veja o estado atual do space sem snapshot obsoleto.

use std::{
    io::{self, BufRead, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{mpsc, Mutex, OnceLock},
    time::Duration,
};

use aeroshell_ipc::types::{AeroEvent, DecorationPolicy, Snapshot, WindowInfo};
use calloop::channel;
use serde_json::{json, Value};
use smithay::{
    desktop::Window,
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::SERIAL_COUNTER,
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

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

pub type Command = (String, mpsc::Sender<String>);

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
        AeroEvent::Csd { id, client_frame } => {
            json!({ "Csd": { "id": id, "client_frame": client_frame } })
        }
        AeroEvent::FramePainted { id } => json!({ "FramePainted": { "id": id } }),
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
        AeroEvent::Csd { .. } => "csd",
        AeroEvent::FramePainted { .. } => "frame_painted",
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
        let id = parts.next().and_then(|v| v.parse::<usize>().ok());
        match op {
            // Comando INTERNO do subscribe do stream: publica o estado atual
            // para o assinante que acabou de conectar (não é protocolo público).
            // Além do snapshot, reenvia as decisões já tomadas: o painel é um
            // WebView que só instala `__aeroEvent` depois de carregar o bundle,
            // então os DecorationApplied do map das primeiras janelas chegam
            // antes dele e se perdiam (indicador ficava "default" até editar).
            "__subscribe__" => {
                self.emit_snapshot();
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
                let mut ids: Vec<u32> = self.decorations.keys().copied().collect();
                for id in self.pending_policies.keys().copied() {
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
                let Some(window) = id.and_then(|i| self.window_for_id(i as u32).cloned()) else {
                    return "ERR janela inexistente".to_owned();
                };
                self.space.raise_element(&window, true);
                if let Some(toplevel) = window.toplevel() {
                    let serial = SERIAL_COUNTER.next_serial();
                    self.seat.get_keyboard().unwrap().set_focus(
                        self,
                        Some(toplevel.wl_surface().clone()),
                        serial,
                    );
                }
                "OK".to_owned()
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
            "maximize" => {
                // Mesma mecânica da 02r, agora compartilhada com o pedido
                // xdg_toplevel.set_maximized (F1-xdg-toplevel-states).
                let Some(id) = id else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(window) = self.window_for_id(id as u32) else {
                    return "ERR janela inexistente".to_owned();
                };
                let target = !window
                    .toplevel()
                    .map(|t| {
                        t.current_state()
                            .states
                            .contains(xdg_toplevel::State::Maximized)
                    })
                    .unwrap_or(false);
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
    use super::base64_encode;

    #[test]
    fn base64_sem_dependencia() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(&[0, 0, 255, 255]), "AAD//w==");
    }
}
