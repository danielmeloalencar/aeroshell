//! WebContext e âncora construct-only compartilhados (ADR 0001).
//!
//! Todos os kinds usam a mesma âncora no processo hospedeiro; cada WebView nasce
//! com `related_view` para não criar um WebProcess novo por superfície.

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{WebContext, WebView};

pub struct SharedWeb {
    pub context: WebContext,
    pub anchor_window: gtk4::ApplicationWindow,
    pub anchor_webview: WebView,
}

/// Cria o contexto único e a âncora oculta (realize, nunca present).
pub fn new(app: &gtk4::Application) -> SharedWeb {
    let context = WebContext::new();
    crate::scheme::install(&context);
    let anchor_window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("aero surface anchor (hidden)")
        .default_width(1)
        .default_height(1)
        .build();
    let anchor_webview = WebView::builder().web_context(&context).build();
    anchor_webview.load_html(
        "<!doctype html><html></html>",
        Some("aero://surface-anchor/"),
    );
    anchor_window.set_child(Some(&anchor_webview));
    gtk4::prelude::WidgetExt::realize(&anchor_window);
    SharedWeb {
        context,
        anchor_window,
        anchor_webview,
    }
}
