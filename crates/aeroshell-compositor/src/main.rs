//! aeroshell-compositor — dono do estado e do input.

mod aero_frame_protocol;
mod backend;
mod cli;
mod input;
mod protocols;
mod state;

use std::{error::Error, io::BufRead, os::unix::net::UnixListener};

use calloop::channel;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};
pub use wayland_server;
pub use wayland_server::protocol::{wl_seat, wl_surface};

pub use state::AeroState;

pub struct CalloopData {
    state: AeroState,
    display_handle: DisplayHandle,
}

fn main() -> Result<(), Box<dyn Error>> {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().compact().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().compact().init();
    }

    let opts = cli::parse();
    let selected_backend = backend::current(&opts);
    tracing::info!(
        backend = selected_backend.name(),
        seat = %selected_backend.seat_name(),
        socket = %opts.socket,
        "iniciando AeroShell"
    );

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;
    let display: Display<AeroState> = Display::new()?;
    let display_handle = display.handle();
    let state = AeroState::new(&mut event_loop, display, &opts)?;

    // O env do host NÃO é alterado: só o processo filho recebe o socket do AeroShell.
    let socket_name = state.socket_name.clone();
    if let Some(command) = &opts.command {
        // `sh -c` permite `--command "zenity --info --text=..."` com argumentos.
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("WAYLAND_DISPLAY", &socket_name)
            // Sem remover DISPLAY, GTK3 abre no X11 do host em vez do nosso compositor.
            .env_remove("DISPLAY")
            .env("GDK_BACKEND", "wayland")
            .spawn();
        match status {
            Ok(mut child) => {
                tracing::info!(?command, pid = child.id(), "cliente inicial iniciado");
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(err) => tracing::warn!(?command, %err, "falha ao iniciar cliente"),
        }
    }

    let mut data = CalloopData {
        state,
        display_handle,
    };
    backend::init(&opts, &mut event_loop, &mut data)?;

    // Injeção de ponteiro sintético (S3): `motion x y`, `button press|release`.
    if let Some(path) = std::env::var_os("AERO_TEST_INPUT_SOCK") {
        let (sender, source) = channel::channel::<String>();
        let listener = UnixListener::bind(&path)?;
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                for line in std::io::BufReader::new(stream).lines().map_while(Result::ok) {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        });
        event_loop.handle().insert_source(source, |event, _, data| {
            let channel::Event::Msg(msg) = event else { return };
            let mut parts = msg.split_whitespace();
            match parts.next() {
                Some("motion") => {
                    let x = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let y = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    data.state.inject_motion(x, y);
                }
                Some("button") => match parts.next() {
                    Some("press") => data.state.inject_button(true),
                    Some("release") => data.state.inject_button(false),
                    _ => {}
                },
                _ => {}
            }
        })?;
        tracing::info!(?path, "injeção de ponteiro sintético ativa");
    }

    event_loop.run(None, &mut data, |_| {})?;

    Ok(())
}
