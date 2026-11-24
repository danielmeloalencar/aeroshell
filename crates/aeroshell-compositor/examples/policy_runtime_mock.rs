//! F2-decoration-policy: runtime Bun MOCK. Conecta no IPC, anuncia `policy-ready`
//! e responde `policy <id> none` para `app_id=aero-no-deco` no stream de eventos.
//! Sem dependência nova: serde_json já é dependência do compositor.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

fn ipc_command(path: &std::path::Path, command: &str) -> Option<String> {
    let mut stream = UnixStream::connect(path).ok()?;
    writeln!(stream, "{command}").ok()?;
    stream.flush().ok()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    BufReader::new(stream).lines().next()?.ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ipc = std::path::PathBuf::from(
        std::env::var_os("AERO_WINDOW_IPC_SOCK").ok_or("AERO_WINDOW_IPC_SOCK ausente")?,
    );
    let mut events = ipc.clone();
    let name = events.file_name().and_then(|n| n.to_str()).unwrap_or("ipc");
    events.set_file_name(format!("{name}.events"));
    let ignore = std::env::var_os("AERO_POLICY_IGNORE").is_some();
    let reply = ipc_command(&ipc, "policy-ready").unwrap_or_else(|| "ERR sem resposta".to_owned());
    eprintln!("POLICY_MOCK READY ignore={ignore} ready_reply={reply}");

    let stream = UnixStream::connect(&events)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if Instant::now() > deadline {
            break;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(request) = value.get("DecorationRequest") else {
            continue;
        };
        let id = request
            .get("id")
            .and_then(|v| v.as_u64())
            .unwrap_or(u32::MAX as u64);
        let app_id = request
            .get("app_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned();
        eprintln!("POLICY_MOCK REQUEST id={id} app_id={app_id}");
        if ignore || app_id != "aero-no-deco" {
            continue;
        }
        let response =
            ipc_command(&ipc, &format!("policy {id} none")).unwrap_or_else(|| "ERR".to_owned());
        eprintln!("POLICY_MOCK REPLY id={id} response={response}");
    }
    Ok(())
}
