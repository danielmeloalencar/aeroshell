//! Spike B (sem WebKit, sem Bun): ordenação init → realize → associate → present.
//! Só prova Quando o role aparece e se `zwlr_layer_surface_v1` expõe o `wl_surface`.
//! Rode: cargo run -p aeroshell-shell --example frame_spike

use gtk4::prelude::*;
use gtk4_layer_shell::LayerShell;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("dev.aeroshell.frame-spike")
        .build();
    app.connect_activate(|app| {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .default_width(320)
            .default_height(200)
            .title("frame spike")
            .build();

        // 1) init ANTES de realize (gtk4-layer-shell lib.rs:170-172).
        window.init_layer_shell();
        window.set_namespace(Some("aeroshell-frame-spike"));

        window.connect_realize(|w| {
            let proxy = gtk4_layer_shell::zwlr_layer_surface_v1(w);
            eprintln!("SPIKE realize: zwlr_layer_surface_v1={}", proxy.is_some());
        });
        window.connect_map(|w| {
            let proxy = gtk4_layer_shell::zwlr_layer_surface_v1(w);
            eprintln!("SPIKE map: zwlr_layer_surface_v1={}", proxy.is_some());
        });

        // 3) present (map) — é aqui que o role é atribuído, não antes.
        window.present();
        eprintln!("SPIKE present chamado");

        let quit = app.clone();
        gtk4::glib::timeout_add_seconds_local_once(3, move || quit.quit());
    });
    app.run();
}
