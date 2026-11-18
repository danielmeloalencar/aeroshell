//! Ponte `window.webkit.messageHandlers.aero.postMessage` ⇄ host.

use gtk4::prelude::*;
use webkit6::UserContentManager;

/// Registra o handler `aero` e conecta mensagens de input region à janela.
pub fn install(window: &gtk4::ApplicationWindow) -> UserContentManager {
    let weak = window.downgrade();
    let manager = UserContentManager::new();
    manager.register_script_message_handler("aero", None);
    manager.connect_script_message_received(Some("aero"), move |_, value| {
        let json = value.to_json(0).unwrap_or_default().to_string();
        let Some(window) = weak.upgrade() else { return };
        if let Some(rects) = crate::input_region::parse(&json) {
            crate::input_region::apply(&window, &rects);
        } else {
            tracing::info!(%json, "mensagem da WebView");
        }
    });
    manager
}
