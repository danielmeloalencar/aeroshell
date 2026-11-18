//! Benchmarks de arquitetura de frames (F0/S5), SEM aero_frame_v1.
//! Experimento A: N WebViews (com/sem related-view), mesmo WebContext.
//! Experimento B: 1 WebView com 20 componentes de frame em atlas (grade sem sobreposição).

use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{LoadEvent, UserContentManager, WebContext, WebView};

const FRAME_HTML: &str = r#"
<!doctype html><html><body style="margin:0;background:#33dd66;color:#001100;
font:14px sans-serif;padding:8px">WebView benchmark · mesmo WebContext</body></html>
"#;

const ATLAS_HTML: &str = r#"
<!doctype html><html><head><meta charset="utf-8"><style>
  html,body { margin:0; background:#202028; font:11px sans-serif; color:#fff }
  #atlas { display:grid; grid-template-columns:repeat(4, 1fr); gap:0; width:100%; height:100vh }
  .frame { display:grid; grid-template-rows:14px 1fr 2px; width:100%; height:100%; }
  .title { background:#3a7bd5; height:14px; line-height:14px; padding-left:3px }
  .edge { background:#33dd66 }
  .client { background:transparent }
</style></head><body>
<div id="atlas"></div>
<script>
function makeFrame(i) {
  const f = document.createElement('div'); f.className='frame'; f.id='f'+i;
  f.innerHTML='<div class="title">frame '+i+'</div><div class="client"></div><div class="edge"></div>';
  return f;
}
function measure(n) {
  const atlas = document.getElementById('atlas');
  const t0 = performance.now();
  for (let k=0;k<10;k++) { atlas.innerHTML=''; for (let i=0;i<n;i++) atlas.appendChild(makeFrame(i)); }
  const t1 = performance.now();
  const victim = document.getElementById('f0');
  const t2 = performance.now();
  for (let k=0;k<100;k++) { victim.remove(); atlas.appendChild(victim); }
  const t3 = performance.now();
  window.webkit.messageHandlers.aero.postMessage({
    type:'atlas', frames:n,
    insert_all_ms:(t1-t0)/10, remove_one_ms:(t3-t2)/100,
    surface_bytes:window.innerWidth*window.innerHeight*4,
    surface_size:window.innerWidth+'x'+window.innerHeight
  });
}
window.addEventListener('load', () => measure(20));
</script></body></html>
"#;

fn manager() -> UserContentManager {
    let manager = UserContentManager::new();
    manager.register_script_message_handler("aero", None);
    manager.connect_script_message_received(Some("aero"), |_, value| {
        tracing::info!(json = ?value.to_json(0), "BENCH: medição enviada pela WebView");
    });
    manager
}

pub fn run_webviews(app: &gtk4::Application, n: usize, related: bool) {
    let context = WebContext::new();
    let manager = manager();
    let mut windows = Vec::with_capacity(n);
    let mut views = Vec::with_capacity(n);
    let mut creation_ms = Vec::with_capacity(n);
    let started = Instant::now();

    for i in 0..n {
        let t = Instant::now();
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title(format!("aero-bench-{i}"))
            .default_width(320)
            .default_height(200)
            .build();
        let mut builder = WebView::builder()
            .user_content_manager(&manager)
            .web_context(&context);
        if related {
            if let Some(first) = views.first() {
                builder = builder.related_view(first);
            }
        }
        let view = builder.build();
        view.load_html(FRAME_HTML, Some("aero://bench/"));
        window.set_child(Some(&view));
        window.present();
        creation_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        windows.push(window);
        views.push(view);
    }

    tracing::info!(
        n,
        related,
        per_webview_ms = ?creation_ms,
        "BENCH A: WebViews criadas (mesmo WebContext)"
    );

    let remaining = std::rc::Rc::new(std::cell::Cell::new(n));
    for view in &views {
        let remaining = remaining.clone();
        view.connect_load_changed(move |_, event| {
            if event == LoadEvent::Finished {
                remaining.set(remaining.get().saturating_sub(1));
                if remaining.get() == 0 {
                    tracing::info!(
                        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                        "BENCH A: load-changed=Finished de TODAS as views"
                    );
                }
            }
        });
    }

    let app_owned = app.clone();
    glib::timeout_add_seconds_local_once(9, move || {
        let t = Instant::now();
        for window in &windows {
            window.close();
        }
        tracing::info!(
            n,
            destroy_ms = t.elapsed().as_secs_f64() * 1000.0,
            "BENCH A: WebViews destruídas"
        );
        let app_quit = app_owned.clone();
        glib::timeout_add_seconds_local_once(2, move || app_quit.quit());
    });
}

/// Teste do ADR 0001: âncora oculta + N views related; destrói a âncora e observa o processo.
pub fn run_anchor(app: &gtk4::Application, n: usize) {
    let context = WebContext::new();
    let manager = manager();

    let anchor_window = gtk4::ApplicationWindow::builder()
        .application(app)
        .default_width(1)
        .default_height(1)
        .build();
    let anchor = WebView::builder()
        .user_content_manager(&manager)
        .web_context(&context)
        .build();
    anchor.load_html(FRAME_HTML, Some("aero://bench/anchor"));
    anchor_window.set_child(Some(&anchor));
    anchor_window.present();
    tracing::info!("BENCH ANCHOR: âncora oculta criada");

    let mut views = Vec::with_capacity(n);
    for _ in 0..n {
        let view = WebView::builder()
            .user_content_manager(&manager)
            .web_context(&context)
            .related_view(&anchor)
            .build();
        view.load_html(FRAME_HTML, Some("aero://bench/related"));
        views.push(view);
    }
    // Manter as views vivas: um window por view.
    let mut windows = Vec::with_capacity(n);
    for view in views {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .default_width(320)
            .default_height(200)
            .build();
        window.set_child(Some(&view));
        window.present();
        windows.push(window);
    }
    tracing::info!(n, "BENCH ANCHOR: views related criadas");

    let app_owned = app.clone();
    glib::timeout_add_seconds_local_once(5, move || {
        tracing::info!("BENCH ANCHOR: destruindo a âncora");
        anchor_window.close();
        glib::timeout_add_seconds_local_once(5, move || {
            tracing::info!("BENCH ANCHOR: pós-destruição (5 s)");
            for window in &windows {
                window.close();
            }
            let app_quit = app_owned.clone();
            glib::timeout_add_seconds_local_once(2, move || app_quit.quit());
        });
    });
}

pub fn run_atlas(app: &gtk4::Application) {
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("aero-bench-atlas")
        .default_width(1000)
        .default_height(650)
        .build();
    let view = WebView::builder()
        .user_content_manager(&manager())
        .web_context(&WebContext::new())
        .build();
    view.load_html(ATLAS_HTML, Some("aero://bench/atlas"));
    window.set_child(Some(&view));
    window.present();
    tracing::info!("BENCH B: atlas de 20 componentes em UMA WebView");

    let app_owned = app.clone();
    glib::timeout_add_seconds_local_once(9, move || {
        window.close();
        let app_quit = app_owned.clone();
        glib::timeout_add_seconds_local_once(2, move || app_quit.quit());
    });
}
