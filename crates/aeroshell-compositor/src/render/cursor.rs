//! Elemento de cursor para o cursor plane do DRM (F1-cursor-shape).
//!
//! O cursor plane JÁ é habilitado pelo `DrmOutputManager` (`backend/udev.rs` passa
//! `Some(gbm)`; o Smithay passa `cursor_size` + gbm ao `DrmCompositor::new`). O que
//! faltava era um elemento com `Kind::Cursor` na cena — é o que este módulo fornece.
//!
//! Segue a referência do domicile (`drm_cursor_factory.h:24-45`): a imagem vem do
//! **tema XCursor da mesa**, não de um bitmap desenhado à mão. O hotspot lido do
//! arquivo é essencial — sem ele a ponta da seta não fica sob o ponteiro.
//!
//! APIs lidas na versão fixada: Smithay `memory.rs:389-403` (`from_slice`) e
//! `:463-489` (elemento com `Kind::Cursor`), `gles/format.rs:21` (`Argb8888` =
//! GL_BGRA, bytes B,G,R,A), `xcursor-0.3.11/src/lib.rs:39-74` (`CursorTheme`) e
//! `parser.rs:13-40` (`Image { xhot, yhot, pixels_rgba, … }`).

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            element::{
                memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
                surface::WaylandSurfaceRenderElement,
                Kind,
            },
            ImportAll, ImportMem,
        },
    },
    input::pointer::{CursorIcon, CursorImageStatus},
    output::Output,
    utils::{IsAlive, Logical, Physical, Point, Size, Transform},
    wayland::compositor::with_states,
};

use crate::{render::elements::AeroRenderElements, state::AeroState};

// `surface.id()` só é usado no log sob `bench`; importar sempre geraria aviso no build normal.
#[cfg(feature = "bench")]
use smithay::reexports::wayland_server::Resource;

const DEFAULT_THEME: &str = "default";
/// Mesmo padrão do portal de cursor do domicile (`portals/settings.rs:49-50`).
const DEFAULT_SIZE: u32 = 24;

/// Imagem pronta para o renderer, com o hotspot real do tema.
#[derive(Debug, Clone)]
pub struct CursorImage {
    pub buffer: MemoryRenderBuffer,
    pub hotspot: Point<i32, Logical>,
    pub size: Size<i32, BufferCoords>,
}

// Alias curto para não conflitar com `Physical`/`Logical` no restante do módulo.
use smithay::utils::Buffer as BufferCoords;

/// Nomes CSS do `CursorIcon` → nomes de arquivo XCursor. O que não estiver na
/// tabela é tentado com o próprio nome (muitos temas têm aliases CSS).
pub fn xcursor_name(icon: &CursorIcon) -> &'static str {
    match icon.name() {
        "default" | "auto" => "left_ptr",
        "pointer" => "hand2",
        "text" => "xterm",
        "wait" => "watch",
        "progress" => "left_ptr_watch",
        "move" => "all-scroll",
        "not-allowed" => "not-allowed",
        "no-drop" => "no-drop",
        "grab" => "grab",
        "grabbing" => "grabbing",
        "help" => "question_arrow",
        "crosshair" => "crosshair",
        "ew-resize" => "sb_h_double_arrow",
        "ns-resize" => "sb_v_double_arrow",
        "zoom-in" => "zoom-in",
        "zoom-out" => "zoom-out",
        other => other,
    }
}

/// Converte RGBA (xcursor) para BGRA (o que `Fourcc::Argb8888` espera na memória).
fn to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    let mut i = 0;
    while i + 3 < rgba.len() {
        out.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
        i += 4;
    }
    out
}

/// Carrega um formato do tema, escolhendo o tamanho nominal mais próximo do pedido.
fn load_image(icon: &CursorIcon) -> Option<CursorImage> {
    let theme_name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| DEFAULT_THEME.to_string());
    let wanted: u32 = std::env::var("XCURSOR_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SIZE);
    let theme = xcursor::CursorTheme::load(&theme_name);
    let path = theme.load_icon(xcursor_name(icon))?;
    let data = std::fs::read(path).ok()?;
    let images = xcursor::parser::parse_xcursor(&data)?;
    let image = images
        .into_iter()
        .min_by_key(|i| (i.size as i64 - wanted as i64).abs())?;

    let size = Size::<i32, BufferCoords>::new(image.width as i32, image.height as i32);
    let buffer = MemoryRenderBuffer::from_slice(
        &to_bgra(&image.pixels_rgba),
        Fourcc::Argb8888,
        size,
        1,
        Transform::Normal,
        None,
    );
    Some(CursorImage {
        buffer,
        hotspot: Point::<i32, Logical>::new(image.xhot as i32, image.yhot as i32),
        size,
    })
}

/// Tema ausente/quebrado não pode deixar o cursor invisível: cai na seta padrão.
fn lookup(icon: &CursorIcon) -> Option<CursorImage> {
    load_image(icon).or_else(|| {
        if icon.name() != CursorIcon::Default.name() {
            load_image(&CursorIcon::Default)
        } else {
            None
        }
    })
}

/// Monta o elemento `Kind::Cursor` do output, se houver ponteiro e cursor visível.
pub fn cursor_element<R: ImportAll + ImportMem>(
    renderer: &mut R,
    state: &mut AeroState,
    output: &Output,
) -> Option<AeroRenderElements<R>>
where
    R::TextureId: Clone + Send + 'static,
{
    let location = state.seat.get_pointer()?.current_location();
    let output_geo = state.space.output_geometry(output)?;
    let scale = output.current_scale().fractional_scale();
    let local = location - output_geo.loc.to_f64();
    let physical: Point<i32, Physical> = local.to_physical_precise_round(scale);

    match state.cursor_image.clone() {
        CursorImageStatus::Hidden => None,
        CursorImageStatus::Named(_icon) => {
            let key = _icon.name().to_owned();
            let image = state
                .cursor_images
                .entry(key.clone())
                .or_insert_with(|| lookup(&_icon))
                .as_ref()?;
            let hotspot: Point<i32, Physical> = image.hotspot.to_physical_precise_round(scale);
            let loc = physical - hotspot;
            let loc_f = loc.to_f64();
            let element = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc_f,
                &image.buffer,
                None,
                None,
                None,
                Kind::Cursor,
            )
            .ok()?;
            #[cfg(feature = "bench")]
            {
                let t = crate::frame_diagnostics::now_ns();
                eprintln!(
                    "CURSOR_RENDER t={t} source=theme shape={} xcursor={} size={}x{} hotspot=({},{}) loc=({:.1},{:.1}) output={}",
                    _icon.name(),
                    xcursor_name(&_icon),
                    image.size.w,
                    image.size.h,
                    image.hotspot.x,
                    image.hotspot.y,
                    loc_f.x,
                    loc_f.y,
                    output.name()
                );
            }
            Some(AeroRenderElements::Cursor(element))
        }
        CursorImageStatus::Surface(surface) => {
            if !surface.alive() {
                return None;
            }
            let hotspot = with_states(&surface, |states| {
                states
                    .data_map
                    .get::<smithay::input::pointer::CursorImageSurfaceData>()
                    .map(|data| data.lock().expect("lock envenenado").hotspot)
                    .unwrap_or_default()
            })
            .to_physical_precise_round(scale);
            let loc = physical - hotspot;
            let element = with_states(&surface, |states| {
                WaylandSurfaceRenderElement::from_surface(
                    renderer,
                    &surface,
                    states,
                    loc.to_f64(),
                    scale as f32,
                    Kind::Cursor,
                )
                .ok()
            })??;
            #[cfg(feature = "bench")]
            {
                let t = crate::frame_diagnostics::now_ns();
                eprintln!(
                    "CURSOR_RENDER t={t} source=surface surface={:?} loc=({:.1},{:.1}) output={}",
                    surface.id(),
                    loc.x,
                    loc.y,
                    output.name()
                );
            }
            Some(AeroRenderElements::Surface(element))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_css_viram_nomes_xcursor() {
        assert_eq!(xcursor_name(&CursorIcon::Default), "left_ptr");
        assert_eq!(xcursor_name(&CursorIcon::Pointer), "hand2");
        assert_eq!(xcursor_name(&CursorIcon::Text), "xterm");
        assert_eq!(xcursor_name(&CursorIcon::Wait), "watch");
    }

    /// Verifica o caminho real (tema → parser → BGRA → hotspot). Só é conclusiva
    /// em máquinas com tema XCursor instalado; fora disso imprime SKIP e não afirma nada.
    #[test]
    fn tema_xcursor_carrega_imagem_com_hotspot() {
        let Some(image) = lookup(&CursorIcon::Default) else {
            eprintln!("SKIP: nenhum tema XCursor instalado neste ambiente");
            return;
        };
        eprintln!(
            "TEMA_XCURSO size={}x{} hotspot=({},{})",
            image.size.w, image.size.h, image.hotspot.x, image.hotspot.y
        );
        assert!(image.size.w > 0 && image.size.h > 0);
        assert!(image.hotspot.x >= 0 && (image.hotspot.x as u32) < image.size.w as u32);
        assert!(image.hotspot.y >= 0 && (image.hotspot.y as u32) < image.size.h as u32);
    }

    #[test]
    fn rgba_vira_bgra_para_argb8888() {
        assert_eq!(to_bgra(&[10, 20, 30, 40]), vec![30, 20, 10, 40]);
        assert_eq!(
            to_bgra(&[1, 2, 3, 4, 5, 6, 7, 8]),
            vec![3, 2, 1, 4, 7, 6, 5, 8]
        );
    }
}
