//! Região de input derivada do DOM (`data-aero-interactive`).
//! O host aplica `wl_surface.set_input_region` via GDK.

use cairo::{RectangleInt, Region};
use gtk4::prelude::*;

#[derive(Debug, Default)]
pub struct InteractiveRects {
    pub rects: Vec<(i32, i32, i32, i32)>,
    pub cursor: String,
}

/// Interpreta a mensagem JSON enviada pela página. Pode vir como string JSON (JSCValue).
pub fn parse(message_json: &str) -> Option<InteractiveRects> {
    let mut value: serde_json::Value = serde_json::from_str(message_json).ok()?;
    if let Some(text) = value.as_str() {
        value = serde_json::from_str(text).ok()?;
    }
    if value.get("type").and_then(|v| v.as_str()) != Some("interactive") {
        return None;
    }
    let rects = value
        .get("rects")?
        .as_array()?
        .iter()
        .filter_map(|r| {
            Some((
                r.get("x")?.as_f64()? as i32,
                r.get("y")?.as_f64()? as i32,
                r.get("w")?.as_f64()? as i32,
                r.get("h")?.as_f64()? as i32,
            ))
        })
        .collect();
    let cursor = value
        .get("cursor")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_owned();
    Some(InteractiveRects { rects, cursor })
}

pub fn apply(window: &gtk4::ApplicationWindow, rects: &InteractiveRects) {
    let Some(surface) = window.native().and_then(|n| n.surface()) else {
        tracing::warn!("janela sem GdkSurface para set_input_region");
        return;
    };
    if rects.rects.is_empty() {
        // `None` em wl_surface.set_input_region significa "superfície inteira",
        // não "vazia". Uma Region vazia é o que deixa o clique passar adiante.
        let empty = Region::create();
        surface.set_input_region(Some(&empty));
        return;
    }
    let areas: Vec<RectangleInt> = rects
        .rects
        .iter()
        .map(|(x, y, w, h)| RectangleInt::new(*x, *y, (*w).max(1), (*h).max(1)))
        .collect();
    let region = Region::create_rectangles(&areas);
    surface.set_input_region(Some(&region));
    tracing::info!(
        rects = ?rects.rects,
        cursor = %rects.cursor,
        "input region aplicada (data-aero-interactive)"
    );
}
