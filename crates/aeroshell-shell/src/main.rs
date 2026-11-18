//! aeroshell-shell — host das WebViews (cliente Wayland do AeroShell).

mod benchmark;
mod bridge;
mod host;
mod scheme;

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{WebContext, WebView};

fn main() {
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().init();
    }

    // GApplication rejeita opções desconhecidas antes do nosso código; use env vars.
    let benchmark_n = std::env::var("AERO_BENCH").ok().and_then(|v| v.parse().ok());
    let benchmark_related = std::env::var_os("AERO_BENCH_RELATED").is_some();
    let benchmark_atlas = std::env::var_os("AERO_BENCH_ATLAS").is_some();
    let benchmark_anchor = std::env::var("AERO_BENCH_ANCHOR")
        .ok()
        .and_then(|v| v.parse().ok());

    let app = gtk4::Application::builder()
        .application_id("dev.aeroshell.shell")
        .build();

    app.connect_activate(move |app| {
        if let Some(n) = benchmark_anchor {
            benchmark::run_anchor(app, n);
            return;
        }
        if let Some(n) = benchmark_n {
            tracing::info!(n, related = benchmark_related, "modo benchmark A (sem aero_frame_v1)");
            benchmark::run_webviews(app, n, benchmark_related);
            return;
        }
        if benchmark_atlas {
            benchmark::run_atlas(app);
            return;
        }
        host::install_transparent_css();
        let window = host::panel_window(app);

        let manager = bridge::install();
        let context = WebContext::new();
        scheme::install(&context);
        let webview = WebView::builder()
            .user_content_manager(&manager)
            .web_context(&context)
            .build();
        webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        tracing::info!(
            alpha = webview.background_color().alpha(),
            "WebView com fundo transparente"
        );
        let params = match (
            std::env::var_os("AERO_PANEL_LEGACY").is_some(),
            std::env::var_os("AERO_PANEL_FORCE_OVERFLOW").is_some(),
        ) {
            (true, true) => "?legacy&overflow",
            (true, false) => "?legacy",
            (false, true) => "?overflow",
            (false, false) => "",
        };
        let uri = format!("{}{params}", scheme::PANEL_URI);
        webview.connect_load_failed(|_, event, uri, error| {
            tracing::warn!(?event, %uri, %error, "carga falhou");
            true
        });
        webview.connect_load_changed(|view, event| {
            if event == webkit6::LoadEvent::Finished {
                let js = "JSON.stringify({overflow:getComputedStyle(document.documentElement).overflow,\
                    scrollHeight:document.documentElement.scrollHeight,\
                    clientHeight:document.documentElement.clientHeight,\
                    scrollWidth:document.documentElement.scrollWidth,\
                    clientWidth:document.documentElement.clientWidth})";
                view.evaluate_javascript(js, None, None, None::<&webkit6::gio::Cancellable>, |result| {
                    match result {
                        Ok(value) => tracing::info!(json = ?value.to_json(0), "METRICS do painel"),
                        Err(err) => tracing::warn!(%err, "evaluate_javascript falhou"),
                    }
                });
            }
        });
        webview.load_uri(&uri);
        window.set_child(Some(&webview));
        window.present();

        tracing::info!(uri = scheme::PANEL_URI, "painel layer-shell apresentado");
    });

    app.run();
}
