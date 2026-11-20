//! IPC de controle de janelas (mecanismo em Rust; política em TS via SDK).
//! Socket Unix separado do Wayland, protocolo de texto por linha:
//!   list            -> uma linha `W id x y w h <titulo-json>` por janela, terminada por `.`
//!                      (titulo-json é uma string JSON, para preservar espaços/UTF-8/newlines)
//!   focus <id>      -> OK
//!   close <id>      -> OK
//!   maximize <id>   -> OK (alterna maximizado)
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
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    utils::SERIAL_COUNTER,
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

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
                let Some(window) = id.and_then(|i| self.window_for_id(i as u32).cloned()) else {
                    return "ERR janela inexistente".to_owned();
                };
                let Some(toplevel) = window.toplevel() else {
                    return "ERR sem toplevel".to_owned();
                };
                let output_size = self
                    .outputs
                    .first()
                    .and_then(|o| o.current_mode())
                    .map(|m| m.size.to_logical(1))
                    .unwrap_or((1280, 800).into());
                toplevel.with_pending_state(|state| {
                    if state.states.contains(xdg_toplevel::State::Maximized) {
                        state.states.unset(xdg_toplevel::State::Maximized);
                        state.size = None;
                    } else {
                        state.states.set(xdg_toplevel::State::Maximized);
                        state.size = Some(output_size);
                    }
                });
                toplevel.send_configure();
                "OK".to_owned()
            }
            _ => "ERR comando desconhecido".to_owned(),
        }
    }
}
