//! Criação da superfície de painel via gtk4-layer-shell.

use gtk4::prelude::*;
use gtk4::ApplicationWindow;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

/// Painel ancorado na borda inferior, com exclusive zone de 48 px.
pub fn panel_window(app: &gtk4::Application) -> ApplicationWindow {
    let window = ApplicationWindow::builder().application(app).build();

    // IMPORTANTE: init_layer_shell() antes de present()/realize.
    window.init_layer_shell();
    window.set_namespace(Some("aeroshell-panel"));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, true);
    window.set_anchor(Edge::Right, true);
    window.set_exclusive_zone(48);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    // GTK4 no Wayland já usa superfície ARGB; a transparência é definida via CSS
    // (install_transparent_css) e pela cor de fundo do WebView.
    window.set_height_request(48);
    window.set_title(Some("AeroShell panel"));

    if !window.is_layer_window() {
        tracing::warn!("gtk4-layer-shell não inicializou: verifique a ordem de link / LD_PRELOAD");
    }

    window
}

/// CSS mínimo: nenhum widget deve pintar fundo opaco.
pub fn install_transparent_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(
        "window, webview, .aero-surface { background-color: transparent; border: none; }",
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
