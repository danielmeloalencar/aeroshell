//! Elemento de cursor para o cursor plane do DRM (F1-cursor-shape).
//!
//! O cursor plane JÁ é habilitado pelo `DrmOutputManager` (`backend/udev.rs` passa
//! `Some(gbm)`; o Smithay passa `cursor_size` + gbm ao `DrmCompositor::new`). O que
//! faltava era um elemento com `Kind::Cursor` na cena — é o que este módulo fornece.
//!
//! Padrão do Smithay 0.7.0 fixado: `MemoryRenderBuffer` → `MemoryRenderBufferRenderElement`
//! com `kind = Kind::Cursor` (`backend/renderer/element/memory.rs:463-489`); o
//! `DrmCompositor::try_assign_cursor_plane` (`backend/drm/compositor/mod.rs:3005-3033`)
//! só aceita elementos desse tipo que caibam no `cursor_size` do dispositivo.
//! Bitmap embutido: evita dependência nova (ver cartão F1-cursor-shape, § Decisão pendente).

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
    input::pointer::CursorImageStatus,
    output::Output,
    utils::{IsAlive, Physical, Point, Rectangle, Size, Transform},
    wayland::compositor::with_states,
};

use crate::{render::elements::AeroRenderElements, state::AeroState};

// `surface.id()` só é usado no log sob `bench`; importar sempre geraria aviso no build normal.
#[cfg(feature = "bench")]
use smithay::reexports::wayland_server::Resource;

/// Seta 12×18: `X` = opaco, `.` = transparente. Contorno preto, interior branco.
const ARROW: [&str; 18] = [
    "X...........",
    "XX..........",
    "XXX.........",
    "XXXX........",
    "XXXXX.......",
    "XXXXXX......",
    "XXXXXXX.....",
    "XXXXXXXX....",
    "XXXXXXXXX...",
    "XXXXXXXXXX..",
    "XXXXXXXXXXX.",
    "XXXXXXXXXXXX",
    "XXXXXXXXXXXX",
    "XXXXXXXXXXXX",
    "XXX...XXXXXX",
    "XX.....XXXXX",
    "X.......XXXX",
    ".........XXX",
];
const W: i32 = 12;
const H: i32 = 18;

/// Cria o buffer uma vez; o `MemoryRenderBuffer` é `Arc`-interno e reutilizado a cada frame.
pub fn new_cursor_buffer() -> MemoryRenderBuffer {
    let mut buffer = MemoryRenderBuffer::new(Fourcc::Argb8888, (W, H), 1, Transform::Normal, None);
    let mut ctx = buffer.render();
    ctx.draw(|pixels| {
        for (row, pattern) in ARROW.iter().enumerate() {
            for (col, ch) in pattern.bytes().enumerate() {
                let idx = (row * W as usize + col) * 4;
                if idx + 3 >= pixels.len() || ch == b'.' {
                    continue;
                }
                // ARGB8888 em memória little-endian = bytes [B, G, R, A].
                let outline = row == 0
                    || col == 0
                    || ARROW[row - 1].as_bytes().get(col) != Some(&b'X')
                    || pattern.as_bytes().get(col - 1) != Some(&b'X');
                let (b, g, r) = if outline { (0, 0, 0) } else { (255, 255, 255) };
                pixels[idx..idx + 4].copy_from_slice(&[b, g, r, 255]);
            }
        }
        Result::<_, ()>::Ok(vec![Rectangle::from_size(Size::new(W, H))])
    })
    .expect("draw da seta é incondicionalmente Ok (tipo de erro ())");
    ctx.update_opaque_regions(None);
    drop(ctx);
    buffer
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
            let buffer = state.cursor_buffer.get_or_insert_with(new_cursor_buffer);
            // A seta tem o hotspot no canto superior esquerdo.
            let loc = physical.to_f64();
            let element = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                loc,
                buffer,
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
                    "CURSOR_RENDER t={t} source=named shape={} size={W}x{H} loc=({:.1},{:.1}) output={}",
                    _icon.name(),
                    loc.x,
                    loc.y,
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
    fn bitmap_seta_tem_contorno_interior_e_transparencia() {
        let mut buffer = new_cursor_buffer();
        let mut ctx = buffer.render();
        let mut pixels = Vec::new();
        ctx.draw(|slice| {
            pixels.extend_from_slice(slice);
            Result::<_, ()>::Ok(vec![])
        })
        .expect("draw da seta é incondicionalmente Ok");

        assert_eq!(pixels.len(), (W * H * 4) as usize);
        let at = |x: usize, y: usize| -> [u8; 4] {
            let i = (y * W as usize + x) * 4;
            [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
        };
        // Canto superior esquerdo: contorno preto opaco.
        assert_eq!(at(0, 0), [0, 0, 0, 255]);
        // Fora da seta: transparente.
        assert_eq!(at(11, 0), [0, 0, 0, 0]);
        // Interior da seta: branco opaco.
        assert_eq!(at(3, 10), [255, 255, 255, 255]);
    }
}
