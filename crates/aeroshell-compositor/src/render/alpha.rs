//! Hit-test por alfa do último buffer importado, com readback lazy de 1×1.
//! A região/geometry continua sendo avaliada antes deste módulo. Falha de import,
//! coordenada fora do buffer ou readback indisponível preserva o hit antigo.

use std::collections::HashMap;

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            buffer_y_inverted,
            element::{
                surface::{WaylandSurfaceRenderElement, WaylandSurfaceTexture},
                Kind,
            },
            gles::GlesRenderer,
            utils::{CommitCounter, RendererSurfaceStateUserData, SurfaceView},
            ExportMem,
        },
    },
    reexports::wayland_server::{backend::ObjectId, protocol::wl_surface::WlSurface, Resource},
    utils::{Buffer as BufferCoord, Logical, Point, Rectangle, Size},
    wayland::compositor::with_states,
};

const MAX_CACHE_ENTRIES: usize = 4096;

#[derive(Clone)]
pub struct AlphaSample {
    commit: CommitCounter,
    buffer: ObjectId,
    view: SurfaceView,
    src: Rectangle<f64, BufferCoord>,
    pixel: Point<i32, BufferCoord>,
    opaque: bool,
}

pub enum AlphaRead {
    Cached(bool),
    Read(AlphaSample),
    Unavailable,
}

type ReadFn = dyn FnMut(&WlSurface, Point<f64, Logical>, Option<&AlphaSample>) -> AlphaRead;

pub struct AlphaSampler {
    read: Box<ReadFn>,
    cache: HashMap<(ObjectId, i32, i32), AlphaSample>,
}

impl AlphaSampler {
    pub fn new(
        read: impl FnMut(&WlSurface, Point<f64, Logical>, Option<&AlphaSample>) -> AlphaRead + 'static,
    ) -> Self {
        Self {
            read: Box::new(read),
            cache: HashMap::new(),
        }
    }

    /// `true` preserva o hit; `false` deixa a busca continuar para o cliente abaixo.
    pub fn is_opaque(&mut self, surface: &WlSurface, local: Point<f64, Logical>) -> bool {
        let key = (surface.id(), local.x.floor() as i32, local.y.floor() as i32);
        let cached = self.cache.get(&key).cloned();
        match (self.read)(surface, local, cached.as_ref()) {
            AlphaRead::Cached(opaque) => opaque,
            AlphaRead::Read(sample) => {
                let opaque = sample.opaque;
                if self.cache.len() >= MAX_CACHE_ENTRIES {
                    self.cache.clear();
                }
                self.cache.insert(key, sample);
                opaque
            }
            AlphaRead::Unavailable => true,
        }
    }
}

fn same_frame(sample: &AlphaSample, other: &AlphaSample) -> bool {
    sample.commit == other.commit
        && sample.buffer == other.buffer
        && sample.view == other.view
        && sample.src == other.src
        && sample.pixel == other.pixel
}

/// Valida o cache pelo RendererSurfaceState sem importar textura. Num miss,
/// reutiliza/importa o commit atual e lê somente o pixel consultado.
/// Fontes Smithay 0.7.0: `renderer/utils/wayland.rs:263-345`,
/// `element/surface.rs:191-282` e `gles/mod.rs:1333-1422`.
pub fn sample_gles(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    local: Point<f64, Logical>,
    cached: Option<&AlphaSample>,
) -> AlphaRead {
    with_states(surface, |states| {
        let Some(data) = states.data_map.get::<RendererSurfaceStateUserData>() else {
            return AlphaRead::Unavailable;
        };
        let Ok(data) = data.lock() else {
            tracing::warn!("hit-test alfa: RendererSurfaceState envenenado");
            return AlphaRead::Unavailable;
        };
        let Some(view) = data.view() else {
            return AlphaRead::Unavailable;
        };
        let Some(buffer) = data.buffer().cloned() else {
            return AlphaRead::Unavailable;
        };
        let Some(logical_buffer) = data.buffer_size() else {
            return AlphaRead::Unavailable;
        };
        let commit = data.current_commit();
        let buffer_scale = data.buffer_scale();
        let transform = data.buffer_transform();
        drop(data);

        if view.dst.w <= 0 || view.dst.h <= 0 || buffer_scale <= 0 {
            return AlphaRead::Unavailable;
        }
        let u = local.x / f64::from(view.dst.w);
        let v = local.y / f64::from(view.dst.h);
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return AlphaRead::Unavailable;
        }
        let logical_point = Point::new(
            view.src.loc.x + u * view.src.size.w,
            view.src.loc.y + v * view.src.size.h,
        );
        let scale = buffer_scale as f64;
        let src = view
            .src
            .to_buffer(scale, transform, &logical_buffer.to_f64());
        let pixel_f64 = logical_point.to_buffer(scale, transform, &logical_buffer.to_f64());
        let buffer_dimensions = logical_buffer
            .to_f64()
            .to_buffer(scale, transform)
            .to_i32_round();
        let mut pixel =
            Point::<i32, BufferCoord>::new(pixel_f64.x.floor() as i32, pixel_f64.y.floor() as i32);
        if buffer_y_inverted(&buffer).unwrap_or(false) {
            pixel.y = buffer_dimensions.h - 1 - pixel.y;
        }
        if pixel.x < 0
            || pixel.y < 0
            || pixel.x >= buffer_dimensions.w
            || pixel.y >= buffer_dimensions.h
        {
            return AlphaRead::Unavailable;
        }
        let mut sample = AlphaSample {
            commit,
            buffer: buffer.id(),
            view,
            src,
            pixel,
            opaque: true,
        };
        if let Some(old) = cached {
            if same_frame(old, &sample) {
                return AlphaRead::Cached(old.opaque);
            }
        }

        let element = match WaylandSurfaceRenderElement::from_surface(
            renderer,
            surface,
            states,
            Point::default(),
            1.0,
            Kind::Unspecified,
        ) {
            Ok(Some(element)) => element,
            Ok(None) => return AlphaRead::Unavailable,
            Err(err) => {
                tracing::warn!(?err, "hit-test alfa: import do buffer falhou");
                return AlphaRead::Unavailable;
            }
        };
        sample.opaque = match element.texture() {
            WaylandSurfaceTexture::SolidColor(color) => color.a() > 0.0,
            WaylandSurfaceTexture::Texture(texture) => {
                let mapping = match renderer.copy_texture(
                    texture,
                    Rectangle::new(pixel, Size::new(1, 1)),
                    Fourcc::Argb8888,
                ) {
                    Ok(mapping) => mapping,
                    Err(err) => {
                        tracing::warn!(?err, "hit-test alfa: readback 1x1 falhou");
                        return AlphaRead::Unavailable;
                    }
                };
                let pixels = match renderer.map_texture(&mapping) {
                    Ok(pixels) => pixels,
                    Err(err) => {
                        tracing::warn!(?err, "hit-test alfa: map do readback falhou");
                        return AlphaRead::Unavailable;
                    }
                };
                let alpha = pixels.get(3).copied().unwrap_or(255);
                #[cfg(feature = "bench")]
                eprintln!(
                    "ALPHA_READBACK size=1x1 full_frame=0 pixel=({}, {}) alpha={alpha}",
                    pixel.x, pixel.y
                );
                alpha != 0
            }
        };
        AlphaRead::Read(sample)
    })
}
