//! aeroshell-compositor — dono do estado e do input.

mod backend;
mod cli;
mod input;
mod protocols;
mod state;

use std::error::Error;

use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};

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
    event_loop.run(None, &mut data, |_| {})?;

    Ok(())
}
