//! Ponte `window.webkit.messageHandlers.aero.postMessage` ⇄ host.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use gtk4::prelude::*;
use webkit6::prelude::WebViewExt;
use webkit6::{UserContentManager, WebView};

fn ipc_socket_path() -> Option<std::ffi::OsString> {
    std::env::var_os("AERO_WINDOW_IPC_SOCK")
}

/// Eventos publicados antes de o bundle instalar `window.__aeroEvent` eram
/// descartados pelo `evaluate_javascript` — o painel abria com indicadores
/// `default`/sem foco até o próximo evento. O gate guarda a ordem até a página
/// avisar `ready` e então libera tudo (sem polling, sem novo comando IPC).
#[derive(Clone, Default)]
struct EventGate {
    ready: Arc<AtomicBool>,
    queued: Arc<Mutex<Vec<String>>>,
}

impl EventGate {
    fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    fn queue(&self, event: String) {
        if let Ok(mut queued) = self.queued.lock() {
            queued.push(event);
        }
    }

    fn drain(&self) -> Vec<String> {
        self.queued
            .lock()
            .map(|mut queued| std::mem::take(&mut *queued))
            .unwrap_or_default()
    }
}

fn event_gate() -> &'static EventGate {
    static GATE: OnceLock<EventGate> = OnceLock::new();
    GATE.get_or_init(EventGate::default)
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
    let gate = event_gate().clone();
    let deliver = move |view: &WebView, event: &str| {
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
    };
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(1), move || {
        if gate.is_ready() {
            for event in gate.drain() {
                deliver(&view, &event);
            }
        }
        while let Ok(event) = rx.try_recv() {
            if gate.is_ready() {
                deliver(&view, &event);
            } else {
                gate.queue(event);
            }
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

/// Única capability IPC de uma WebView genérica: input/blur da surface dona.
/// O ID vem do host, nunca de PID/env; qualquer outro comando/campo é recusado.
fn guest_update_command(surface_id: &str, command: &str) -> Result<String, String> {
    let raw = command
        .strip_prefix("surface-update ")
        .ok_or_else(|| "guest só pode atualizar input/blur da própria superfície".to_owned())?;
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|err| err.to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "surface-update guest espera objeto".to_owned())?;
    if object.get("id").and_then(serde_json::Value::as_str) != Some(surface_id) {
        return Err("guest não pode atualizar outra superfície".to_owned());
    }
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "id" | "input_region" | "blur_region"))
    {
        return Err("guest só pode atualizar input_region/blur_region".to_owned());
    }
    Ok(command.to_owned())
}

/// Launch é uma capability explícita da superfície; o ID vai para o comando
/// privilegiado `launch` do compositor, que valida a lista de `.desktop`.
fn guest_launch_command(allow_launch: bool, app_id: Option<&str>) -> Result<String, String> {
    if !allow_launch {
        return Err("superfície sem allowlist launch-app".to_owned());
    }
    let id = app_id.ok_or_else(|| "launch-app sem id".to_owned())?;
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid {
        return Err("app_id inválido".to_owned());
    }
    Ok(format!("launch {id}"))
}

/// Bridge padrão das superfícies genéricas. Eventos chegam pelo único stream do
/// host; comandos passam pela allowlist da superfície vinculada.
pub fn install_surface(
    window: &gtk4::ApplicationWindow,
    webview: &WebView,
    surface_id: &str,
    allow_launch: bool,
) -> UserContentManager {
    let weak_window = window.downgrade();
    let weak_webview = webview.downgrade();
    let manager = webview
        .user_content_manager()
        .expect("WebView sem UserContentManager");
    let id_json = serde_json::to_string(surface_id).unwrap_or_else(|_| "\"\"".to_owned());
    let bootstrap = format!(
        r#"window.aeroSurfaceId={id_json};
window.aero=Object.freeze({{
  postMessage(message) {{
    window.webkit.messageHandlers.aero.postMessage(message);
  }}
}});
window.__aeroEvent=function(event) {{
  window.dispatchEvent(new CustomEvent('aeroEvent',{{detail:event}}));
}};"#
    );
    manager.add_script(&webkit6::UserScript::new(
        &bootstrap,
        webkit6::UserContentInjectedFrames::TopFrame,
        webkit6::UserScriptInjectionTime::Start,
        &[],
        &[],
    ));
    manager.register_script_message_handler("aero", None);
    let owned_id = surface_id.to_owned();
    manager.connect_script_message_received(Some("aero"), move |_, value| {
        let json = value.to_json(0).unwrap_or_default().to_string();
        let Ok(message) = serde_json::from_str::<serde_json::Value>(&json) else {
            return;
        };
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
                let lines = match guest_update_command(&owned_id, command) {
                    Ok(allowed) => forward(&allowed),
                    Err(error) => vec![format!("ERR permissão negada: {error}")],
                };
                tracing::info!(command, ?lines, "PONTE IPC superfície restrita");
                reply(&webview, seq, &lines);
            }
            Some("launch-app") => {
                let (Some(webview), Some(app_id), Some(seq)) = (
                    weak_webview.upgrade(),
                    message.get("id").and_then(|v| v.as_str()),
                    message.get("seq").and_then(|v| v.as_u64()),
                ) else {
                    return;
                };
                let lines = match guest_launch_command(allow_launch, Some(app_id)) {
                    Ok(command) => forward(&command),
                    Err(error) => vec![format!("ERR permissão negada: {error}")],
                };
                tracing::info!(?app_id, allow_launch, ?lines, "PONTE launch-app superfície");
                reply(&webview, seq, &lines);
            }
            _ => {
                let Some(window) = weak_window.upgrade() else {
                    return;
                };
                if let Some(rects) = crate::input_region::parse(&json) {
                    crate::input_region::apply(&window, &rects);
                } else {
                    tracing::info!(%json, "mensagem do guest de superfície");
                }
            }
        }
    });
    manager
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
            Some("ready") => event_gate().mark_ready(),
            // Toasts vivem acima da barra de 48 px; a WebView só cresce enquanto
            // houver notificações. A exclusive zone do painel não é alterada.
            Some("surface-height") => {
                if let (Some(window), Some(height)) = (
                    weak_window.upgrade(),
                    message.get("height").and_then(|v| v.as_u64()),
                ) {
                    window.set_height_request(height.clamp(48, 480) as i32);
                    window.queue_resize();
                }
            }
            Some("ipc") => {
                let (Some(webview), Some(command), Some(seq)) = (
                    weak_webview.upgrade(),
                    message.get("command").and_then(|v| v.as_str()),
                    message.get("seq").and_then(|v| v.as_u64()),
                ) else {
                    return;
                };
                let lines = forward(command);
                if command.starts_with("workspace-") || command.starts_with("window-workspace") {
                    tracing::info!(command, ?lines, "PONTE IPC workspace");
                }
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

#[cfg(test)]
mod tests {
    use super::{guest_launch_command, guest_update_command};

    #[test]
    fn guest_so_atualiza_input_e_blur_da_propria_superficie() {
        let own = guest_update_command(
            "panel-a",
            r#"surface-update {"id":"panel-a","input_region":[],"blur_region":null}"#,
        );
        assert!(own.is_ok());
        for command in [
            "focus 1",
            "minimize 1",
            "fullscreen 1",
            "move 1 640 80",
            "resize 1 800 600",
            "begin-move 1",
            "begin-resize 1 bottom-right",
            "thumbnail 1",
            "surface-create {}",
            "surface-destroy panel-a",
            r#"input-bind {"id":"guest","keys":"Super+1"}"#,
            "input-unbind guest",
            r#"surface-update {"id":"panel-b","input_region":[]}"#,
            r#"surface-update {"id":"panel-a","html":"fora da capability"}"#,
            r#"surface-update {"id":"panel-a","keyboard_mode":"exclusive"}"#,
        ] {
            assert!(
                guest_update_command("panel-a", command).is_err(),
                "aceitou: {command}"
            );
        }
    }

    #[test]
    fn launch_app_exige_allowlist_e_id_seguro() {
        assert_eq!(
            guest_launch_command(true, Some("org.gnome.gedit")),
            Ok("launch org.gnome.gedit".to_owned())
        );
        assert!(guest_launch_command(false, Some("org.gnome.gedit")).is_err());
        for invalid in [
            None,
            Some(""),
            Some("../etc/passwd"),
            Some("foot;rm -rf /"),
            Some("a b"),
            Some(&"x".repeat(129)),
        ] {
            assert!(
                guest_launch_command(true, invalid).is_err(),
                "aceitou: {invalid:?}"
            );
        }
    }
}
