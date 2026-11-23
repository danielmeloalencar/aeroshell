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
        wayland_server::{protocol::wl_shm::Format, protocol::wl_surface::WlSurface, Resource},
    },
    utils::{Logical, Point, Size, SERIAL_COUNTER},
    wayland::{
        compositor::with_states, shell::xdg::XdgToplevelSurfaceData, shm::with_buffer_contents,
        xdg_toplevel_icon::ToplevelIconCachedState,
    },
};

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

/// Cópia dos pixels do ícone do cached state da superfície do toplevel.
/// Devolve (largura, altura, escala, formato, RGB/BGRA em linhas tightly-packed).
fn icon_pixels(surface: &WlSurface) -> Option<(i32, i32, i32, &'static str, Vec<u8>)> {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<ToplevelIconCachedState>();
        let mut chosen: Option<(i32, i32, i32, &'static str, Vec<u8>)> = None;
        for (buffer, scale) in cached.current().buffers() {
            if !buffer.is_alive() {
                continue;
            }
            let Ok(Some(candidate)) = with_buffer_contents(buffer, |ptr, len, info| {
                let (w, h, stride) = (info.width, info.height, info.stride);
                let format = match info.format {
                    Format::Argb8888 => "Argb8888",
                    Format::Xrgb8888 => "Xrgb8888",
                    _ => return None,
                };
                if w <= 0 || h <= 0 || stride < w * 4 {
                    return None;
                }
                let row = w as usize * 4;
                let needed = stride as usize * h as usize;
                if needed > len {
                    return None;
                }
                let mut pixels = vec![0u8; row * h as usize];
                // SAFETY: `ptr` é válido enquanto este closure executa (documentado
                // em smithay/wayland/shm/mod.rs:219-239). Copiamos na hora para um
                // Vec próprio, linha a linha dentro do stride informado: nenhuma
                // referência ao shm sobrevive ao closure, mesmo que o cliente o
                // mute depois. `row*h` foi validado contra `len` acima.
                unsafe {
                    for y in 0..h as usize {
                        std::ptr::copy_nonoverlapping(
                            ptr.add(y * stride as usize),
                            pixels[y * row..].as_mut_ptr(),
                            row,
                        );
                    }
                }
                Some((w, h, *scale, format, pixels))
            }) else {
                continue;
            };
            let bigger = chosen
                .as_ref()
                .map(|old| old.0 * old.1 < candidate.0 * candidate.1)
                .unwrap_or(true);
            if bigger {
                chosen = Some(candidate);
            }
        }
        chosen
    })
}

/// Nome de ícone do tema, quando o cliente não enviou buffers (ex.: `foot`).
fn icon_name(surface: &WlSurface) -> Option<String> {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<ToplevelIconCachedState>();
        cached.current().icon_name().map(str::to_owned)
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
                    out.push_str(&format!(
                        "W {} {} {} {} {} {}\n",
                        id, geo.loc.x, geo.loc.y, geo.size.w, geo.size.h, title
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
                let Some(id) = id else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(window) = self.window_for_id(id as u32).cloned() else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(toplevel) = window.toplevel() else {
                    return "ERR sem toplevel".to_owned();
                };
                // O toggle usa o estado JÁ COMITADO pelo cliente (current_state),
                // não o pending: um duplo clique antes do commit não inverte o
                // estado. Fonte: Smithay 0.7.0 xdg/mod.rs:1735-1747.
                if toplevel
                    .current_state()
                    .states
                    .contains(xdg_toplevel::State::Maximized)
                {
                    if let Some((loc, size)) = self.maximized_restore.remove(&(id as u32)) {
                        // map_element reposiciona um elemento já mapeado e preserva
                        // seus outputs (Smithay 0.7.0 space/mod.rs:96-119).
                        self.space.map_element(window.clone(), loc, false);
                        toplevel.with_pending_state(|state| {
                            state.states.unset(xdg_toplevel::State::Maximized);
                            state.size = Some(size);
                        });
                    } else {
                        // Maximizado sem geometria salva (cliente pediu o estado
                        // por conta própria): só limpa o estado, sem inventar loc.
                        toplevel.with_pending_state(|state| {
                            state.states.unset(xdg_toplevel::State::Maximized);
                            state.size = None;
                        });
                    }
                } else {
                    let Some(area) = self.work_area() else {
                        return "ERR sem área útil".to_owned();
                    };
                    if let Some(geo) = self.space.element_geometry(&window) {
                        self.maximized_restore
                            .insert(id as u32, (geo.loc, geo.size));
                    }
                    let fgeo = self.frame_geometry_for_window(id as u32);
                    let (dx, dy) = fgeo.offset();
                    // frame_loc = cliente + offset; queremos frame_loc == origem
                    // da área útil (nem um pixel embaixo do painel).
                    let client_loc = Point::<i32, Logical>::new(area.loc.x - dx, area.loc.y - dy);
                    // frame_target = conteúdo + extras; queremos == tamanho da
                    // área útil, nunca o tamanho bruto do output.
                    let client_size = Size::<i32, Logical>::new(
                        (area.size.w - fgeo.extra_w()).max(1),
                        (area.size.h - fgeo.extra_h()).max(1),
                    );
                    self.space.map_element(window.clone(), client_loc, false);
                    toplevel.with_pending_state(|state| {
                        state.states.set(xdg_toplevel::State::Maximized);
                        state.size = Some(client_size);
                    });
                }
                toplevel.send_configure();
                "OK".to_owned()
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
