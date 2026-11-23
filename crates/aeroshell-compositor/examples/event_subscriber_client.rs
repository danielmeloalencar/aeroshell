//! F2-event-stream: assinante sintético do stream de eventos do compositor.
//! Conecta em `<AERO_WINDOW_IPC_SOCK>.events` e imprime cada evento recebido.
//! Sem dependência nova: só std.

use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ipc = std::env::var_os("AERO_WINDOW_IPC_SOCK").ok_or("AERO_WINDOW_IPC_SOCK ausente")?;
    let mut path = std::path::PathBuf::from(ipc);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("ipc");
    path.set_file_name(format!("{name}.events"));
    let stream = match UnixStream::connect(&path) {
        Ok(stream) => stream,
        Err(err) => {
            eprintln!("EVENT_CLIENT CONECTA_FALHOU path={path:?} err={err}");
            return Err(err.into());
        }
    };
    eprintln!("EVENT_CLIENT CONECTADO path={path:?}");
    let mut reader = BufReader::new(stream);
    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        let mut line = String::new();
        // read_line bloqueia; o harness encerra o cliente após a janela de teste.
        let _ = reader.read_line(&mut line)?;
        if line.is_empty() {
            break;
        }
        let line = line.trim_end().to_owned();
        eprintln!("EVENT_CLIENT EVENT {line}");
    }
    eprintln!("EVENT_CLIENT OK");
    Ok(())
}
