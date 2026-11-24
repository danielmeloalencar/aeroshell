//! Cliente de teste F2-compositor-cursor-shape: pede formas de cursor ao compositor
//! pelo caminho real do GTK (`wp_cursor_shape_v1`, implementado pelo GDK Wayland).
//!
//! `AERO_CURSOR_SHAPE_SEQUENCE` = lista separada por vírgulas (padrão:
//! `ew-resize,ns-resize,all-scroll,pointer`); uma forma a cada 1,5 s.
//! A sonda `CURSOR_CLIENT` marca cada pedido enviado.

use gtk4::gdk::prelude::SurfaceExt;
use gtk4::gdk::Cursor;
use gtk4::glib;
use gtk4::prelude::*;
use std::time::Duration;

fn main() {
    let app = gtk4::Application::builder()
        .application_id("aero.cursor.shape.test")
        .build();
    app.connect_activate(|app| {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("cursor shape")
            .default_width(320)
            .default_height(240)
            .build();
        window.connect_realize(|window| {
            let Some(surface) = window.native().and_then(|native| native.surface()) else {
                eprintln!("CURSOR_CLIENT erro=sem_surface");
                return;
            };
            let sequence = std::env::var("AERO_CURSOR_SHAPE_SEQUENCE")
                .unwrap_or_else(|_| "ew-resize,ns-resize,all-scroll,pointer".to_owned());
            let shapes: Vec<String> = sequence
                .split(',')
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
                .collect();
            let index = std::rc::Rc::new(std::cell::Cell::new(0usize));
            let apply = {
                let index = index.clone();
                let surface = surface.clone();
                move || {
                    if let Some(name) = shapes.get(index.get() % shapes.len()) {
                        let cursor = Cursor::from_name(name, None);
                        surface.set_cursor(cursor.as_ref());
                        eprintln!("CURSOR_CLIENT pronto shape={name}");
                        index.set(index.get() + 1);
                    }
                    glib::ControlFlow::Continue
                }
            };
            apply();
            glib::timeout_add_local(Duration::from_millis(1500), apply);
        });
        window.present();
    });
    app.run();
}
