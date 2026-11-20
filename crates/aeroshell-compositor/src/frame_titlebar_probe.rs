//! 02p: readback full-res da titlebar, somente sob `cfg(feature = "bench")`.
//!
//! Diferente da grade 64x40 da 02o, cada pixel RGB da região é preservado. A
//! captura usa `ExportMem::copy_framebuffer`/`map_texture` do Smithay 0.7 e só
//! é ativada por `AERO_TITLEBAR_DUMP_DIR`; nada é lido no binário sem bench.

use std::sync::atomic::{AtomicUsize, Ordering};

use smithay::{
    backend::{allocator::Fourcc, renderer::ExportMem},
    utils::{Buffer as BufferCoord, Point, Rectangle, Size},
};

#[derive(Debug, Clone, Copy)]
pub struct TitlebarFrame {
    pub window_id: u32,
    pub region: Rectangle<i32, BufferCoord>,
    pub buffer: (i32, i32),
    pub target: (i32, i32),
    pub rescaled: bool,
    pub release_pending: bool,
}

static NEXT_FRAME: AtomicUsize = AtomicUsize::new(0);

pub fn capture<R: ExportMem>(
    renderer: &mut R,
    framebuffer: &R::Framebuffer<'_>,
    frame: TitlebarFrame,
) {
    let Some(dir) = std::env::var_os("AERO_TITLEBAR_DUMP_DIR") else {
        return;
    };
    if frame.region.size.w <= 0 || frame.region.size.h <= 0 {
        eprintln!("TITLEBAR_FRAME_ERRO região vazia {:?}", frame.region);
        return;
    }

    let mapping = match renderer.copy_framebuffer(framebuffer, frame.region, Fourcc::Argb8888) {
        Ok(mapping) => mapping,
        Err(err) => {
            eprintln!("TITLEBAR_FRAME_ERRO copy_framebuffer falhou: {err}");
            return;
        }
    };
    let pixels = match renderer.map_texture(&mapping) {
        Ok(pixels) => pixels,
        Err(err) => {
            eprintln!("TITLEBAR_FRAME_ERRO map_texture falhou: {err}");
            return;
        }
    };

    let number = NEXT_FRAME.fetch_add(1, Ordering::Relaxed);
    let path = std::path::Path::new(&dir).join(format!("titlebar_{number:06}.bgra"));
    if let Err(err) = std::fs::write(&path, pixels) {
        eprintln!("TITLEBAR_FRAME_ERRO escrita {path:?} falhou: {err}");
        return;
    }

    let mut hash = 0xcbf29ce484222325u64;
    for byte in pixels {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Restaura o drawable do winit depois de `map_texture`, como na sonda 02o.
    let _restore = renderer.copy_framebuffer(
        framebuffer,
        Rectangle::new(Point::new(0, 0), Size::new(1, 1)),
        Fourcc::Argb8888,
    );

    let t = crate::frame_diagnostics::now_ns();
    let r = frame.region;
    eprintln!(
        "TITLEBAR_FRAME t={t} frame={number} window_id={} x={} y={} w={} h={} \
         buf={}x{} target={}x{} rescaled={} release={} sha={hash:016x}",
        frame.window_id,
        r.loc.x,
        r.loc.y,
        r.size.w,
        r.size.h,
        frame.buffer.0,
        frame.buffer.1,
        frame.target.0,
        frame.target.1,
        frame.rescaled,
        frame.release_pending,
    );
}
