//! Elementos de render do frame: canvas fixo + 9-slice + placeholder (ADR 0007).
//!
//! Agnóstico de backend: genérico sobre `R`, sem winit/udev/headless e sem
//! acesso direto ao `AeroState`. O backend fornece só os dados que o frame
//! precisa (geometria declarada, origem, conteúdo commitado e output).
//!
//! A ordem dos wrappers é `Relocate<Rescale<Crop<E>>>`: o `Crop` INTERNO ajusta
//! `src()` para a fatia do buffer; o `Rescale` muda só a geometria (`dst`); o
//! `Relocate` reposiciona. Se o `Crop` fosse externo, ele recalcularia `src()`
//! com a geometria já escalada e amostraria o buffer inteiro dentro da fatia.

use smithay::{
    backend::renderer::{
        element::{
            solid::{SolidColorBuffer, SolidColorRenderElement},
            surface::WaylandSurfaceRenderElement,
            utils::{CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement},
            Element, Kind,
        },
        ImportAll, ImportMem,
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Physical, Point, Rectangle, Scale, Size},
    wayland::compositor::with_states,
};

use crate::state::FrameGeometry;

#[cfg(feature = "bench")]
use crate::{frame_diagnostics::FrameDiagnostics, frame_titlebar_probe::TitlebarFrame};
#[cfg(feature = "bench")]
use smithay::output::Output;

pub type FrameSlice<R> =
    RelocateRenderElement<RescaleRenderElement<CropRenderElement<WaylandSurfaceRenderElement<R>>>>;
pub type SliceRect = Rectangle<i32, Physical>;
pub type SlicePoint = Point<i32, Physical>;

// Padrão anvil v0.7.0 (anvil/src/render.rs): um enum cobre o elemento normal e
// o elemento reescalado. Fonte: docs.rs/smithay/0.7.0/.../macro.render_elements.html
smithay::backend::renderer::element::render_elements! {
    pub AeroRenderElements<R> where R: ImportAll + ImportMem;
    Surface = WaylandSurfaceRenderElement<R>,
    Rescaled = RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
    Slice = FrameSlice<R>,
    Solid = SolidColorRenderElement,
}

/// Alvo = conteúdo JÁ COMMITADO + insets/shadow (canvas fixo, ADR 0007).
pub fn frame_target(committed: Size<i32, Logical>, fgeo: &FrameGeometry) -> Size<i32, Logical> {
    Size::new(committed.w + fgeo.extra_w(), committed.h + fgeo.extra_h())
}

/// O que o chamador precisa para atualizar seu próprio estado após montar o frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRenderKind {
    /// Buffer do WebKit disponível: 9-slice do canvas.
    Frame,
    /// Primeiro buffer ainda ausente: placeholder sólido.
    Placeholder,
}

/// Dados do frame que o compositor já resolveu antes de chamar o render.
pub struct FrameRenderInput<'a> {
    pub surface: &'a WlSurface,
    pub fgeo: FrameGeometry,
    /// Origem do frame em coordenadas FÍSICAS (conteúdo + offset do frame).
    pub loc: SlicePoint,
    /// Origem do cliente em coordenadas LÓGICAS (usada pelo log do placeholder).
    #[cfg(feature = "bench")]
    pub client_origin: Point<i32, Logical>,
    /// Tamanho de conteúdo commitado pelo cliente.
    pub committed: Size<i32, Logical>,
    pub output_scale: f64,
    /// Output, usado só pela sonda de pixels da titlebar sob `bench`.
    #[cfg(feature = "bench")]
    pub output: &'a Output,
    pub window_id: u32,
    /// resize grab ativo nesta janela (diagnóstico 02j, sob `bench`).
    #[cfg(feature = "bench")]
    pub resize_active: bool,
}

pub struct FrameRenderResult {
    pub kind: FrameRenderKind,
    #[cfg(feature = "bench")]
    pub titlebar_probe: Option<TitlebarFrame>,
}

/// Monta os elementos do frame (9-slice com canvas fixo ou placeholder) em
/// `elements`, na mesma ordem em que o backend vinha fazendo. Nenhuma mudança
/// de geometria, escala ou instrumentação em relação ao baseline F0.
pub fn push_frame_elements<R: ImportAll + ImportMem>(
    renderer: &mut R,
    elements: &mut Vec<AeroRenderElements<R>>,
    input: FrameRenderInput<'_>,
    #[cfg(feature = "bench")] diag: &mut FrameDiagnostics,
) -> Option<FrameRenderResult>
where
    R::TextureId: Clone + 'static,
{
    let frame_surface = input.surface;
    let fgeo = input.fgeo;
    let loc = input.loc;
    let committed = input.committed;
    let output_scale = input.output_scale;
    #[cfg(feature = "bench")]
    let output = input.output;
    let _window_id = input.window_id;

    let f = Point::<f64, Physical>::from((loc.x as f64, loc.y as f64));
    let element = with_states(frame_surface, |states| {
        WaylandSurfaceRenderElement::from_surface(
            renderer,
            frame_surface,
            states,
            f,
            1.0,
            Kind::Unspecified,
        )
    });

    // Alvo = conteúdo JÁ COMMITADO + insets/shadow.
    // Também define a geometria do placeholder enquanto o primeiro buffer
    // WebKit ainda não existe.
    let target = frame_target(committed, &fgeo);

    #[cfg(feature = "bench")]
    let mut titlebar_probe = None;

    match element {
        Ok(Some(el)) => {
            // O buffer da decoração é assíncrono (WebKit); enquanto ele não
            // alcança o alvo, escalamos no render para manter a borda colada
            // (ADR 0006).
            let buffer = el.buffer_size();
            // 02p: canvas fixo => todo commit válido usa 9-slice, inclusive
            // quando buffer == target. Não há caminho nativo/swap no release.
            let rescaled = buffer.w > 0 && buffer.h > 0;
            // 02k: linhas de corte = insets do frame (state.rs:36-56).
            // `shadow` é 0 nesta fase; se passar a existir, entra nas fatias externas.
            let es = Scale::from(output_scale);
            let elem_geo = el.geometry(es);
            let origin = elem_geo.loc;
            let buf_phys = elem_geo.size;
            let dst_phys: Size<i32, Physical> = target.to_physical_precise_round(output_scale);
            let l = (fgeo.insets.0 as f64 * output_scale).round() as i32;
            let t = (fgeo.insets.1 as f64 * output_scale).round() as i32;
            let r = (fgeo.insets.2 as f64 * output_scale).round() as i32;
            let b = (fgeo.insets.3 as f64 * output_scale).round() as i32;
            let src_mid_w = (buf_phys.w - l - r).max(1);
            let src_mid_h = (buf_phys.h - t - b).max(1);
            let dst_mid_w = (dst_phys.w - l - r).max(1);
            let dst_mid_h = (dst_phys.h - t - b).max(1);
            // Só o interior estica: cantos fixos, laterais esticam em Y,
            // topo/base esticam em X.
            let slice_sx = dst_mid_w as f64 / src_mid_w as f64;
            let slice_sy = dst_mid_h as f64 / src_mid_h as f64;
            let delta = SlicePoint::new(loc.x - origin.x, loc.y - origin.y);
            // 02l: zonas protegidas declaradas pelo shell. Somente o intervalo
            // entre elas estica em X; texto e botões mantêm a largura do buffer.
            let title_slices = (fgeo.titlebar_slices.0.max(0), fgeo.titlebar_slices.1.max(0));
            let tl = (title_slices.0 as f64 * output_scale).round() as i32;
            let tr = (title_slices.1 as f64 * output_scale).round() as i32;
            // Nunca deixar o intervalo sem pixels: o Crop de uma fatia vazia é descartado.
            let tl = tl.min((src_mid_w - 1).max(0));
            let tr = tr.min((src_mid_w - tl - 1).max(0));
            let src_gap_w = (src_mid_w - tl - tr).max(1);
            let dst_gap_w = (dst_mid_w - tl - tr).max(1);
            let gap_sx = dst_gap_w as f64 / src_gap_w as f64;
            #[cfg(feature = "bench")]
            let title_n = 3;
            #[cfg(feature = "bench")]
            let title_left_w = tl;
            #[cfg(feature = "bench")]
            let title_gap_w = dst_gap_w;
            #[cfg(feature = "bench")]
            let title_right_w = tr;
            // Fatias presas à borda direita/inferior do alvo precisam do
            // deslocamento da diferença buffer->alvo; as presas à esquerda/topo
            // usam o deslocamento comum.
            let shift_x = dst_mid_w - src_mid_w;
            let shift_y = dst_mid_h - src_mid_h;
            let delta_rx = SlicePoint::new(delta.x + shift_x, delta.y);
            let delta_by = SlicePoint::new(delta.x, delta.y + shift_y);
            let delta_rb = SlicePoint::new(delta.x + shift_x, delta.y + shift_y);
            #[cfg(feature = "bench")]
            if _window_id == 0 {
                let region = Rectangle::<i32, Physical>::new(loc, (dst_phys.w, t).into());
                let region = output
                    .current_mode()
                    .map(|mode| {
                        let visible = Rectangle::from_size(mode.size);
                        let region = region.intersection(visible).unwrap_or(region);
                        output
                            .current_transform()
                            .invert()
                            .transform_rect_in(region, &mode.size)
                    })
                    .unwrap_or(region);
                titlebar_probe = Some(TitlebarFrame {
                    window_id: _window_id,
                    region: Rectangle::new(
                        Point::new(region.loc.x, region.loc.y),
                        Size::new(region.size.w, region.size.h),
                    ),
                    buffer: (buffer.w, buffer.h),
                    target: (target.w, target.h),
                    rescaled,
                    release_pending: false,
                });
            }
            #[cfg(feature = "bench")]
            let frame_t_nanos;
            #[cfg(feature = "bench")]
            {
                frame_t_nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                eprintln!(
                    "RESIZE_GLUE t={frame_t_nanos} window_id={_window_id} content={}x{} target={}x{} buffer={}x{} rescaled={rescaled}",
                    committed.w, committed.h, target.w, target.h, buffer.w, buffer.h,
                );
                // 02j: duração do episódio de escala, sem alterar o RescaleRenderElement.
                let max_scale = if buffer.w > 0 && buffer.h > 0 {
                    (target.w as f64 / buffer.w as f64).max(target.h as f64 / buffer.h as f64)
                } else {
                    0.0
                };
                diag.note_stretch(
                    _window_id,
                    rescaled && input.resize_active,
                    max_scale,
                    std::time::Instant::now(),
                    crate::frame_diagnostics::now_ns(),
                );
                // 02k: com 9-slice as fatias fixas mantêm os insets exatos
                // (escala 1 no eixo fixo); as demais esticam só no eixo livre.
                let (sx, sy) = if rescaled {
                    (slice_sx, slice_sy)
                } else {
                    (1.0, 1.0)
                };
                eprintln!(
                    "RESIZE_SLICE t={frame_t_nanos} window_id={_window_id} \
                     buf={}x{} target={}x{} rescaled={rescaled} \
                     insets={},{},{},{} edge_l={} \
                     edge_r={} title_h={} \
                     slice_scale_x={sx:.4} slice_scale_y={sy:.4} \
                     title_n={title_n} title_slices={},{} \
                     title_left_w={title_left_w} \
                     title_gap_w={title_gap_w} \
                     title_right_w={title_right_w}",
                    buffer.w,
                    buffer.h,
                    target.w,
                    target.h,
                    fgeo.insets.0,
                    fgeo.insets.1,
                    fgeo.insets.2,
                    fgeo.insets.3,
                    fgeo.insets.0,
                    fgeo.insets.2,
                    fgeo.insets.1,
                    title_slices.0,
                    title_slices.1,
                );
            }
            if rescaled {
                let x0 = origin.x;
                let y0 = origin.y;
                let x1 = x0 + l;
                let y1 = y0 + t;
                let x2 = x0 + buf_phys.w - r;
                let y2 = y0 + buf_phys.h - b;
                let rect = |x: i32, y: i32, w: i32, h: i32| {
                    SliceRect::new(SlicePoint::new(x, y), (w, h).into())
                };
                // (crop na origem, âncora do rescale, escala, deslocamento final)
                let specs: [(SliceRect, SlicePoint, (f64, f64), SlicePoint); 9] = [
                    // Topo: texto e botões NÃO esticam; só o intervalo entre eles muda.
                    (
                        rect(x0, y0, l + tl, t),
                        SlicePoint::new(x0, y0),
                        (1.0, 1.0),
                        delta,
                    ),
                    (
                        rect(x1 + tl, y0, src_gap_w, t),
                        SlicePoint::new(x1 + tl, y0),
                        (gap_sx, 1.0),
                        delta,
                    ),
                    (
                        rect(x2 - tr, y0, r + tr, t),
                        SlicePoint::new(x2 - tr, y0),
                        (1.0, 1.0),
                        delta_rx,
                    ),
                    // 2 cantos inferiores: sem escala.
                    (
                        rect(x0, y2, l, b),
                        SlicePoint::new(x0, y2),
                        (1.0, 1.0),
                        delta_by,
                    ),
                    (
                        rect(x2, y2, r, b),
                        SlicePoint::new(x2, y2),
                        (1.0, 1.0),
                        delta_rb,
                    ),
                    // left/right: largura fixa, altura estica.
                    (
                        rect(x0, y1, l, src_mid_h),
                        SlicePoint::new(x0, y1),
                        (1.0, slice_sy),
                        delta,
                    ),
                    (
                        rect(x2, y1, r, src_mid_h),
                        SlicePoint::new(x2, y1),
                        (1.0, slice_sy),
                        delta_rx,
                    ),
                    // base: altura fixa, largura estica.
                    (
                        rect(x1, y2, src_mid_w, b),
                        SlicePoint::new(x1, y2),
                        (slice_sx, 1.0),
                        delta_by,
                    ),
                    // centro (transparente sob o cliente).
                    (
                        rect(x1, y1, src_mid_w, src_mid_h),
                        SlicePoint::new(x1, y1),
                        (slice_sx, slice_sy),
                        delta,
                    ),
                ];
                #[cfg(feature = "bench")]
                let before = elements.len();
                #[cfg(feature = "bench")]
                let mut slice_idx = 0usize;
                for (crop, anchor, (sxa, sya), move_delta) in specs {
                    #[cfg(feature = "bench")]
                    {
                        slice_idx += 1;
                    }
                    let slice = with_states(frame_surface, |states| {
                        WaylandSurfaceRenderElement::from_surface(
                            renderer,
                            frame_surface,
                            states,
                            f,
                            1.0,
                            Kind::Unspecified,
                        )
                    });
                    let Ok(Some(slice)) = slice else {
                        continue;
                    };
                    let Some(cropped) = CropRenderElement::from_element(slice, es, crop) else {
                        continue;
                    };
                    let scaled = RescaleRenderElement::from_element(
                        cropped,
                        anchor,
                        Scale::from((sxa, sya)),
                    );
                    let moved =
                        RelocateRenderElement::from_element(scaled, move_delta, Relocate::Relative);
                    // 02l: geometria REAL do elemento composto — não é o valor
                    // calculado para a instrumentação de cima.
                    #[cfg(feature = "bench")]
                    {
                        let g = moved.geometry(es);
                        eprintln!(
                            "SLICE_GEOM t={frame_t_nanos} window_id={_window_id} \
                             i={} x={} y={} w={} h={}",
                            slice_idx - 1,
                            g.loc.x,
                            g.loc.y,
                            g.size.w,
                            g.size.h,
                        );
                    }
                    elements.push(AeroRenderElements::Slice(moved));
                }
                #[cfg(feature = "bench")]
                {
                    let composed = elements.len() - before;
                    let t_nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    eprintln!(
                        "SLICE_COMPOSE t={t_nanos} window_id={_window_id} \
                         n={composed} buf={}x{} target={}x{}",
                        buffer.w, buffer.h, target.w, target.h,
                    );
                }
            } else {
                elements.push(AeroRenderElements::Surface(el));
            }
        }
        Ok(None) => {
            // 02q: sem buffer ainda, a decoração não fica ausente. Desenha só
            // titlebar/bordas (o centro continua livre para o cliente), com a
            // mesma cor do HTML e na geometria-alvo.
            let dst: Size<i32, Physical> = target.to_physical_precise_round(output_scale);
            let l = (fgeo.insets.0 as f64 * output_scale).round() as i32;
            let t = (fgeo.insets.1 as f64 * output_scale).round() as i32;
            let r = (fgeo.insets.2 as f64 * output_scale).round() as i32;
            let b = (fgeo.insets.3 as f64 * output_scale).round() as i32;
            let middle_h = (dst.h - t - b).max(0);
            let rects = [
                (loc, dst.w, t),
                (SlicePoint::new(loc.x, loc.y + t), l, middle_h),
                (SlicePoint::new(loc.x + dst.w - r, loc.y + t), r, middle_h),
                (SlicePoint::new(loc.x, loc.y + dst.h - b), dst.w, b),
            ];
            for (origin, w, h) in rects {
                if w <= 0 || h <= 0 {
                    continue;
                }
                let solid = SolidColorBuffer::new(
                    (w, h),
                    [43.0_f32 / 255.0, 43.0_f32 / 255.0, 43.0_f32 / 255.0, 1.0],
                );
                elements.push(AeroRenderElements::Solid(
                    SolidColorRenderElement::from_buffer(
                        &solid,
                        origin,
                        1.0,
                        1.0,
                        Kind::Unspecified,
                    ),
                ));
            }
            #[cfg(feature = "bench")]
            {
                let t_nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                eprintln!(
                    "FRAME_PLACEHOLDER t={t_nanos} window_id={_window_id} \
                     x={} y={} w={} h={}",
                    input.client_origin.x + fgeo.offset().0,
                    input.client_origin.y + fgeo.offset().1,
                    target.w,
                    target.h,
                );
            }
            return Some(FrameRenderResult {
                kind: FrameRenderKind::Placeholder,
                #[cfg(feature = "bench")]
                titlebar_probe,
            });
        }
        Err(err) => {
            tracing::warn!(?err, "import do frame falhou");
            return None;
        }
    }

    Some(FrameRenderResult {
        kind: FrameRenderKind::Frame,
        #[cfg(feature = "bench")]
        titlebar_probe,
    })
}
