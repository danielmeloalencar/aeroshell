//! Ponte `window.webkit.messageHandlers.aero.postMessage` ⇄ host.

use webkit6::UserContentManager;

/// Registra o handler `aero` e devolve o UserContentManager para a WebView.
pub fn install() -> UserContentManager {
    let manager = UserContentManager::new();
    manager.register_script_message_handler("aero", None);
    manager.connect_script_message_received(Some("aero"), |_, value| {
        tracing::info!(json = ?value.to_json(0), "BENCH/METRICS: mensagem da WebView");
    });
    manager
}
