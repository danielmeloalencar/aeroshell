//! aeroshell-shell — host das WebViews (cliente Wayland do AeroShell).

mod benchmark;
mod bridge;
mod host;
mod scheme;

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::WebView;

const PANEL_HTML: &str = r#"
<!doctype html>
<html lang="pt-br">
  <head>
    <meta charset="utf-8" />
    <style>
      html, body { margin: 0; height: 100%; background: transparent; color: white;
        font: 13px sans-serif; }
      /* Cor de validação do S2: precisa ser impossível confundir com o fundo. */
      #panel { height: 100%; display: flex; align-items: center; padding: 0 12px;
        background: #33dd66; color: #001100; font-weight: bold; font-size: 16px;
        border-radius: 10px 10px 0 0; margin: 4px 0 0 0; }
    </style>
  </head>
  <body>
    <div id="panel" data-aero-interactive>PANEL AEROSHELL S2 — verde = layer-shell + WebView</div>
    <script>
      window.webkit.messageHandlers.aero.postMessage({ type: "ready", uri: location.href });
    </script>
  </body>
</html>
"#;

fn main() {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    // GApplication rejeita opções desconhecidas antes do nosso código; use env var.
    let benchmark_n = std::env::var("AERO_BENCH").ok().and_then(|v| v.parse().ok());

    let app = gtk4::Application::builder()
        .application_id("dev.aeroshell.shell")
        .build();

    app.connect_activate(move |app| {
        if let Some(n) = benchmark_n {
            tracing::info!(n, "modo benchmark do S5 (sem aero_frame_v1)");
            benchmark::run(app, n);
            return;
        }
        host::install_transparent_css();
        let window = host::panel_window(app);

        let manager = bridge::install();
        let webview = WebView::builder().user_content_manager(&manager).build();
        webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        tracing::info!(
            alpha = webview.background_color().alpha(),
            "WebView com fundo transparente"
        );
        webview.load_html(PANEL_HTML, Some(scheme::PANEL_URI));
        window.set_child(Some(&webview));
        window.present();

        tracing::info!(uri = scheme::PANEL_URI, "painel layer-shell apresentado");
    });

    app.run();
}
