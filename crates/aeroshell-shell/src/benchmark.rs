//! Micro-benchmark do S5 sem protocolo: N WebViews, MESMO processo shell e MESMO WebContext.
//! Não há aero_frame_v1 aqui — apenas a pergunta arquitetural: quantos WebProcesses e
//! quanta RAM incremental uma WebView custa.

use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{WebContext, WebView};

const HTML: &str = r#"
<!doctype html><html><body style="margin:0;background:#33dd66;color:#001100;
font:14px sans-serif;padding:8px">WebView benchmark · mesmo WebContext</body></html>
"#;

pub fn run(app: &gtk4::Application, n: usize) {
    let context = WebContext::new();
    let mut windows = Vec::with_capacity(n);
    let mut creation_ms = Vec::with_capacity(n);

    for i in 0..n {
        let start = Instant::now();
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title(format!("aero-bench-{i}"))
            .default_width(320)
            .default_height(200)
            .build();
        let webview = WebView::builder().web_context(&context).build();
        webview.load_html(HTML, Some("aero://bench/"));
        window.set_child(Some(&webview));
        window.present();
        creation_ms.push(start.elapsed().as_secs_f64() * 1000.0);
        windows.push(window);
    }

    tracing::info!(
        n,
        per_webview_ms = ?creation_ms,
        "BENCH: WebViews criadas (mesmo WebContext)"
    );

    let app_owned = app.clone();
    glib::timeout_add_seconds_local_once(5, move || {
        let start = Instant::now();
        for window in &windows {
            window.close();
        }
        let destroy_ms = start.elapsed().as_secs_f64() * 1000.0;
        tracing::info!(n, destroy_ms, "BENCH: WebViews destruídas");
        let app_quit = app_owned.clone();
        glib::timeout_add_seconds_local_once(2, move || {
            app_quit.quit();
        });
    });
}
