//! 02o: readback de PIXELS REAIS do framebuffer sob `cfg(feature = "bench")`.
//!
//! API real do Smithay 0.7.0 fixado:
//!   `ExportMem::copy_framebuffer` (backend/renderer/mod.rs:733-738) e
//!   `ExportMem::map_texture` (mod.rs:774-775) devolvem `&[u8]` com os pixels.
//! Nada disto existe no binário sem `bench`: a variável `AERO_PIXEL_DUMP_DIR`
//! só é lida aqui e desliga a captura quando ausente.
//!
//! O dump é uma grade 64x40 de médias RGB dos pixels ARGB8888 reais lidos do
//! framebuffer. Ela cobre a janela inteira (inclusive a barra de título e o
//! canto arrastado), sem depender da transformação do output para escolher
//! coordenadas. Nenhum filtro, limiar ou amostra é aplicado neste módulo.

use std::sync::atomic::{AtomicUsize, Ordering};

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{ExportMem, Texture},
    },
    utils::{Buffer as BufferCoord, Point, Rectangle, Size},
};

const GRID_W: usize = 64;
const GRID_H: usize = 40;

static NEXT_FRAME: AtomicUsize = AtomicUsize::new(0);
// F1f: dump FULL-RES (ligado por `AERO_PIXEL_FULL`) para medir com exatidão a
// continuidade borda do frame ↔ cliente. Sobrescreve o arquivo a cada frame:
// o primeiro frame é todo clear e não serve de medição.
/// Chamado DEPOIS de `tracker.render_output`, com o renderer e o framebuffer
/// ainda vivos. Não altera a cena: `copy_framebuffer` é não destrutivo.
pub fn capture<R: ExportMem>(
    renderer: &mut R,
    framebuffer: &R::Framebuffer<'_>,
    slice_generation: u64,
    release_pending: bool,
) {
    let Some(dir) = std::env::var_os("AERO_PIXEL_DUMP_DIR") else {
        return;
    };

    let size = framebuffer.size();
    if size.w <= 0 || size.h <= 0 {
        eprintln!("PIXEL_FRAME_ERRO framebuffer vazio {}x{}", size.w, size.h);
        return;
    }
    let region = Rectangle::<i32, BufferCoord>::new(Point::new(0, 0), size);

    let mapping = match renderer.copy_framebuffer(framebuffer, region, Fourcc::Argb8888) {
        Ok(mapping) => mapping,
        Err(err) => {
            eprintln!("PIXEL_FRAME_ERRO copy_framebuffer falhou: {err}");
            return;
        }
    };
    let pixels = match renderer.map_texture(&mapping) {
        Ok(pixels) => pixels,
        Err(err) => {
            eprintln!("PIXEL_FRAME_ERRO map_texture falhou: {err}");
            return;
        }
    };

    let w = size.w as usize;
    let h = size.h as usize;
    let mut grid = vec![0u8; GRID_W * GRID_H * 3];
    for cy in 0..GRID_H {
        let y0 = cy * h / GRID_H;
        let y1 = ((cy + 1) * h / GRID_H).max(y0 + 1).min(h);
        for cx in 0..GRID_W {
            let x0 = cx * w / GRID_W;
            let x1 = ((cx + 1) * w / GRID_W).max(x0 + 1).min(w);
            let mut r = 0u64;
            let mut g = 0u64;
            let mut b = 0u64;
            let mut n = 0u64;
            // Argb8888 em memória little-endian: B, G, R, A (gles/format.rs:21).
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    if i + 3 < pixels.len() {
                        b += pixels[i] as u64;
                        g += pixels[i + 1] as u64;
                        r += pixels[i + 2] as u64;
                        n += 1;
                    }
                }
            }
            let denom = n.max(1);
            let o = (cy * GRID_W + cx) * 3;
            grid[o] = (r / denom) as u8;
            grid[o + 1] = (g / denom) as u8;
            grid[o + 2] = (b / denom) as u8;
        }
    }

    // F1f: um único dump full-res (w*h*4 bytes BGRA), sem filtro nem amostra.
    if std::env::var_os("AERO_PIXEL_FULL").is_some() {
        let full = std::path::Path::new(&dir).join("full_000000.bgra");
        if let Err(err) = std::fs::write(&full, pixels) {
            eprintln!("PIXEL_FRAME_ERRO dump full {full:?} falhou: {err}");
        } else {
            eprintln!("PIXEL_FRAME_FULL size={w}x{h} path={full:?}");
        }
    }

    let frame = NEXT_FRAME.fetch_add(1, Ordering::Relaxed);
    let path = std::path::Path::new(&dir).join(format!("frame_{frame:06}.bin"));
    if let Err(err) = std::fs::write(&path, &grid) {
        eprintln!("PIXEL_FRAME_ERRO escrita {path:?} falhou: {err}");
        return;
    }

    let mut hash = 0xcbf29ce484222325u64;
    for byte in &grid {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // `map_texture` chama `EglContext::make_current()` (gles/mod.rs:1395), o que
    // deixa um drawable diferente corrente e faz o `submit` seguinte falhar com
    // BAD_SURFACE nesta VM. `copy_framebuffer` chama `target.make_current`
    // (gles/mod.rs:1279): uma chamada 1x1 restaura o drawable do winit antes do
    // swap. Só existe sob `bench`.
    let _restore = renderer.copy_framebuffer(
        framebuffer,
        Rectangle::new(Point::new(0, 0), Size::new(1, 1)),
        Fourcc::Argb8888,
    );

    let t = crate::frame_diagnostics::now_ns();
    eprintln!(
        "PIXEL_FRAME t={t} frame={frame} size={}x{} gen={slice_generation} \
         release={} sha={hash:016x}",
        size.w, size.h, release_pending,
    );
}
