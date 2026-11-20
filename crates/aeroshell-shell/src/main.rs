//! aeroshell-shell — host das WebViews (cliente Wayland do AeroShell).

mod benchmark;
mod bridge;
mod host;
mod input_region;
#[cfg(feature = "probe-registry")]
mod registry_probe;
mod scheme;

use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{WebContext, WebView};

fn main() {
    // stderr é unbuffered: importante quando o processo é encerrado por timeout em testes.
    if let Ok(filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .init();
    }

    // Prova do caminho de produção: usar WAYLAND_SOCKET ANTES do GDK e remover a
    // variável em seguida, para o painel seguir pelo socket público.
    #[cfg(feature = "probe-registry")]
    registry_probe::log_privileged_globals();

    // GApplication rejeita opções desconhecidas antes do nosso código; use env vars.
    let benchmark_n = std::env::var("AERO_BENCH")
        .ok()
        .and_then(|v| v.parse().ok());
    let benchmark_related = std::env::var_os("AERO_BENCH_RELATED").is_some();
    let benchmark_atlas = std::env::var_os("AERO_BENCH_ATLAS").is_some();
    let benchmark_anchor = std::env::var("AERO_BENCH_ANCHOR")
        .ok()
        .and_then(|v| v.parse().ok());

    let app_id = if std::env::var_os("AERO_TEST_BACKDROP").is_some() {
        "dev.aeroshell.backdrop"
    } else {
        "dev.aeroshell.shell"
    };
    let app = gtk4::Application::builder().application_id(app_id).build();

    app.connect_activate(move |app| {
        if std::env::var_os("AERO_TEST_BACKDROP").is_some() {
            let window = gtk4::ApplicationWindow::builder()
                .application(app)
                .title("BACKDROP atrás do painel")
                .default_width(1280)
                .default_height(800)
                .build();
            let label = gtk4::Label::new(Some("BACKDROP — clique na área transparente do painel"));
            // Contador visível: permite confirmar que o clique na parte transparente
            // do painel passa através da layer surface e chega a esta janela.
            let counter = std::rc::Rc::new(std::cell::Cell::new(0u32));
            let gesture = gtk4::GestureClick::new();
            let weak_label = label.downgrade();
            let weak_counter = counter.clone();
            gesture.connect_pressed(move |_, _, _, _| {
                if let Some(label) = weak_label.upgrade() {
                    let n = weak_counter.get() + 1;
                    weak_counter.set(n);
                    label.set_label(&format!("BACKDROP recebeu {n} clique(s)"));
                }
            });
            window.add_controller(gesture);
            window.set_child(Some(&label));
            window.present();
            tracing::info!("BACKDROP apresentado");
            return;
        }
        if let Some(n) = benchmark_anchor {
            benchmark::run_anchor(app, n);
            return;
        }
        if let Some(n) = benchmark_n {
            tracing::info!(
                n,
                related = benchmark_related,
                "modo benchmark A (sem aero_frame_v1)"
            );
            benchmark::run_webviews(app, n, benchmark_related);
            return;
        }
        if benchmark_atlas {
            benchmark::run_atlas(app);
            return;
        }
        host::install_transparent_css();
        let window = host::panel_window(app);

        let context = WebContext::new();
        scheme::install(&context);
        let webview = WebView::builder().web_context(&context).build();
        bridge::install(&window, &webview);
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
                let js =
                    "JSON.stringify({overflow:getComputedStyle(document.documentElement).overflow,\
                    scrollHeight:document.documentElement.scrollHeight,\
                    clientHeight:document.documentElement.clientHeight,\
                    scrollWidth:document.documentElement.scrollWidth,\
                    clientWidth:document.documentElement.clientWidth})";
                view.evaluate_javascript(
                    js,
                    None,
                    None,
                    None::<&webkit6::gio::Cancellable>,
                    |result| match result {
                        Ok(value) => tracing::info!(json = ?value.to_json(0), "METRICS do painel"),
                        Err(err) => tracing::warn!(%err, "evaluate_javascript falhou"),
                    },
                );
            }
        });
        webview.load_uri(&uri);
        window.set_child(Some(&webview));
        window.present();

        tracing::info!(uri = scheme::PANEL_URI, "painel layer-shell apresentado");

        if let Ok(seconds) = std::env::var("AERO_SHELL_EXIT_AFTER")
            .and_then(|v| v.parse::<u32>().map_err(|_| std::env::VarError::NotPresent))
        {
            let app_quit = app.clone();
            gtk4::glib::timeout_add_seconds_local_once(seconds, move || app_quit.quit());
        }
    });

    app.run();
}
