//! IPC de controle de janelas (mecanismo em Rust; política em TS via SDK).
//! Socket Unix separado do Wayland, protocolo de texto por linha:
//!   list            -> uma linha `W id x y w h <titulo-json>` por janela, terminada por `.`
//!                      (titulo-json é uma string JSON, para preservar espaços/UTF-8/newlines)
//!   focus <id>      -> OK
//!   close <id>      -> OK
//!   maximize <id>   -> OK (alterna maximizado)
//!   icon <id>       -> ICON <id> <w> <h> <scale> <formato> <base64-dos-pixels> | NONE
//!   csd <id>        -> CSD <id> 1|0  (1 = cliente pediu client-side decorations)
//! Cada request é despachado no event loop (canal calloop) e respondido por um
//! oneshot mpsc, para que `list` veja o estado atual do space sem snapshot obsoleto.

use std::{
    io::{self, BufRead, Write},
    os::unix::net::UnixListener,
    sync::mpsc,
    time::Duration,
};

use calloop::channel;
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
fn window_title(window: &Window) -> String {
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
    /// Executa um comando de janela no event loop e devolve a resposta textual.
    pub fn window_command(&mut self, cmd: &str) -> String {
        let mut parts = cmd.split_whitespace();
        let op = parts.next().unwrap_or("");
        let id = parts.next().and_then(|v| v.parse::<usize>().ok());
        match op {
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
                // é o sinal observável no caso real.
                let geo = window.geometry();
                let from_geometry = geo.loc.x != 0 || geo.loc.y != 0;
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
