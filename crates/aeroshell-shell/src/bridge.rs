//! Ponte `window.webkit.messageHandlers.aero.postMessage` ⇄ host.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
};

use gtk4::prelude::*;
use webkit6::prelude::WebViewExt;
use webkit6::{UserContentManager, WebView};

fn ipc_socket_path() -> Option<std::ffi::OsString> {
    std::env::var_os("AERO_WINDOW_IPC_SOCK")
}

/// `<ipc>.events`: o mesmo socket derivado pelo compositor, sem env nova.
fn event_socket_path() -> Option<std::path::PathBuf> {
    let ipc = ipc_socket_path()?;
    let mut path = std::path::PathBuf::from(ipc);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("aero-ipc")
        .to_owned();
    path.set_file_name(format!("{name}.events"));
    Some(path)
}

/// Consome o stream PUSH numa thread e entrega cada evento ao WebView no main loop.
/// Não há `timeout`: o evento chega quando o compositor publica.
pub fn install_event_stream(webview: &WebView) {
    let Some(path) = event_socket_path() else {
        tracing::warn!("AERO_WINDOW_IPC_SOCK ausente; stream de eventos desligado");
        return;
    };
    // A thread ACORDA no socket; o main loop só drena o canal (1 ms, sem
    // consultar o compositor nem fazer polling de 100 ms/1 s).
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let view = webview.clone();
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(1), move || {
        while let Ok(event) = rx.try_recv() {
            let script = format!("window.__aeroEvent && window.__aeroEvent({event});");
            view.evaluate_javascript(
                &script,
                None,
                None,
                None::<&webkit6::gio::Cancellable>,
                |result| {
                    if let Err(err) = result {
                        tracing::warn!(%err, "evaluate_javascript do evento falhou");
                    }
                },
            );
            tracing::info!(event = %event, "BRIDGE aeroEvent");
        }
        gtk4::glib::ControlFlow::Continue
    });
    std::thread::spawn(move || {
        let stream = match UnixStream::connect(&path) {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!(%err, ?path, "falha ao conectar no stream de eventos");
                return;
            }
        };
        tracing::info!(?path, "BRIDGE stream de eventos conectado");
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if line.is_empty() {
                continue;
            }
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

/// Encaminha um comando ao compositor e devolve as linhas da resposta.
/// Uma conexão por comando: o protocolo IPC é textual e não mantém sessão.
fn forward(command: &str) -> Vec<String> {
    let Some(path) = ipc_socket_path() else {
        return vec!["ERR AERO_WINDOW_IPC_SOCK ausente".to_owned()];
    };
    let Ok(mut stream) = UnixStream::connect(path) else {
        return vec!["ERR falha ao conectar no IPC".to_owned()];
    };
    if writeln!(stream, "{command}").is_err() || stream.flush().is_err() {
        return vec!["ERR falha ao enviar comando".to_owned()];
    }
    // O IPC responde "OK" para focus/close e "." apenas para list. Sem
    // shutdown do lado de escrita, o compositor mantém a conexão aberta
    // esperando outro comando e o BufReader abaixo bloqueia para sempre,
    // congelando o main loop do shell (botões mortos na tela).
    let _ = stream.shutdown(std::net::Shutdown::Write);
    BufReader::new(stream)
        .lines()
        .map_while(Result::ok)
        .take_while(|line| line != ".")
        .collect()
}

fn reply(webview: &WebView, seq: u64, lines: &[String]) {
    let Ok(json) = serde_json::to_string(lines) else {
        return;
    };
    let script = format!("window.__aeroReply && window.__aeroReply({seq}, {json});");
    webview.evaluate_javascript(
        &script,
        None,
        None,
        None::<&webkit6::gio::Cancellable>,
        |result| {
            if let Err(err) = result {
                tracing::warn!(%err, "evaluate_javascript falhou");
            }
        },
    );
}

/// Registra o handler `aero`, conecta input regions e encaminha comandos IPC.
pub fn install(window: &gtk4::ApplicationWindow, webview: &WebView) -> UserContentManager {
    let weak_window = window.downgrade();
    let weak_webview = webview.downgrade();
    // Registrar no manager REAL do WebView (webkit6 0.6.1:
    // web_view.rs:1050): criar um novo e descartá-lo não instala o handler.
    let manager = webview
        .user_content_manager()
        .expect("WebView sem UserContentManager");
    manager.register_script_message_handler("aero", None);
    manager.connect_script_message_received(Some("aero"), move |_, value| {
        let json = value.to_json(0).unwrap_or_default().to_string();
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&json) else {
            return;
        };
        // A WebView pode enviar a mensagem como string JSON (JSCValue).
        let message = message
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(message);

        match message.get("type").and_then(|v| v.as_str()) {
            Some("ipc") => {
                let (Some(webview), Some(command), Some(seq)) = (
                    weak_webview.upgrade(),
                    message.get("command").and_then(|v| v.as_str()),
                    message.get("seq").and_then(|v| v.as_u64()),
                ) else {
                    return;
                };
                let lines = forward(command);
                if command == "list" {
                    let summary: Vec<String> = lines
                        .iter()
                        .filter_map(|line| line.strip_prefix("W "))
                        .map(|line| {
                            // "id x y w h <titulo-json>" (window_ipc.rs:92).
                            let mut parts = line.splitn(6, ' ');
                            let id = parts.next().unwrap_or("?");
                            let title = parts.nth(4).unwrap_or("\"\"");
                            format!("{id} {title}")
                        })
                        .collect();
                    tracing::info!(command, lista = %summary.join(" | "), "painel React");
                }
                reply(&webview, seq, &lines);
            }
            _ => {
                let Some(window) = weak_window.upgrade() else {
                    return;
                };
                if let Some(rects) = crate::input_region::parse(&json) {
                    crate::input_region::apply(&window, &rects);
                } else {
                    tracing::info!(%json, "mensagem da WebView");
                }
            }
        }
    });
    manager
}
