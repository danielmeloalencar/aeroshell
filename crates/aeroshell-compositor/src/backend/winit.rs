//! Backend nested: janela no compositor host via winit.

use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            element::{
                surface::WaylandSurfaceRenderElement,
                utils::{CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement},
                AsRenderElements, Element, Kind,
            },
            gles::GlesRenderer,
            ImportAll, ImportMem,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_server::Resource,
        winit::{dpi::PhysicalSize, window::CursorIcon},
    },
    utils::{IsAlive, Scale as UtilsScale, Transform},
    wayland::{compositor::with_states, shell::wlr_layer::Layer},
};

use crate::{cli::Options, CalloopData};

// Padrão anvil v0.7.0 (anvil/src/render.rs): um enum cobre o elemento normal e
// o elemento reescalado. Fonte: docs.rs/smithay/0.7.0/.../macro.render_elements.html
//
// 02k: `Slice` é uma fatia do 9-slice. A ordem dos wrappers é
// Relocate<Rescale<Crop<E>>>: o Crop INTERNO ajusta `src()` para a fatia do
// buffer (elements.rs:157-172); o Rescale muda só a geometria (dst); o Relocate
// reposiciona. Se o Crop fosse externo, ele recalcularia `src()` usando a
// geometria já escalada e amostraria o buffer inteiro dentro da fatia.
type FrameSlice<R> =
    RelocateRenderElement<RescaleRenderElement<CropRenderElement<WaylandSurfaceRenderElement<R>>>>;
type SliceRect = smithay::utils::Rectangle<i32, smithay::utils::Physical>;
type SlicePoint = smithay::utils::Point<i32, smithay::utils::Physical>;

smithay::backend::renderer::element::render_elements! {
    pub AeroRenderElements<R> where R: ImportAll + ImportMem;
    Surface = WaylandSurfaceRenderElement<R>,
    Rescaled = RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
    Slice = FrameSlice<R>,
}

pub struct WinitBackend;

impl super::Backend for WinitBackend {
    fn name(&self) -> &'static str {
        "winit"
    }
    fn seat_name(&self) -> String {
        "aeroshell-winit".to_owned()
    }
}

pub fn init_winit(
    opts: &Options,
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit_source) = winit::init::<GlesRenderer>()?;

    // winit 0.30 não expõe Window::set_size. Fixamos min=max para forçar o tamanho
    // pedido; assim `window_size()` e os eventos Resized convergem para `--size`.
    if let Some((width, height)) = opts.size {
        let requested = PhysicalSize::new(width, height);
        backend.window().set_min_inner_size(Some(requested));
        backend.window().set_max_inner_size(Some(requested));
    }

    let size = opts
        .size
        .map(|(w, h)| (w as i32, h as i32).into())
        .unwrap_or_else(|| backend.window_size());
    let mode = Mode {
        size,
        refresh: 60_000,
    };

    tracing::info!(
        requested = ?opts.size,
        window = ?backend.window_size(),
        mode = ?size,
        "tamanho do output winit"
    );

    let mut outputs = Vec::new();
    for i in 0..opts.outputs {
        let output = Output::new(
            format!("aero-{i}"),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "AeroShell".into(),
                model: "nested".into(),
            },
        );
        let _global = output.create_global::<crate::state::AeroState>(&data.display_handle);
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            Some(Scale::Fractional(opts.scale)),
            Some((i as i32 * size.w, 0).into()),
        );
        output.set_preferred(mode);
        data.state.space.map_output(&output, (i as i32 * size.w, 0));
        outputs.push(output);
    }
    data.state.outputs = outputs.clone();

    let mut damage_trackers: Vec<_> = outputs
        .iter()
        .map(OutputDamageTracker::from_output)
        .collect();
    let forced_size = opts.size;

    event_loop
        .handle()
        .insert_source(winit_source, move |event, _, data| {
            let state = &mut data.state;
            match event {
                WinitEvent::Resized { size, .. } => {
                    // Sem `--size`, a janela host é a fonte da verdade. Com `--size`,
                    // mantemos o modo pedido: winit 0.30 não permite redimensionar a janela.
                    if forced_size.is_none() {
                        if let Some(output) = state.outputs.first() {
                            let new_mode = Mode {
                                size,
                                refresh: 60_000,
                            };
                            output.change_current_state(Some(new_mode), None, None, None);
                            output.set_preferred(new_mode);
                        }
                    }
                    backend.window().request_redraw();
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => {
                    // F0/S1: renderizamos o primeiro output; multi-output real entra em F1.
                    let Some(output) = state.outputs.first() else {
                        return;
                    };
                    let Some(tracker) = damage_trackers.first_mut() else {
                        return;
                    };
                    let rendered = match backend.bind() {
                        Ok((renderer, mut framebuffer)) => {
                            // ADR 0004: a lista é FRENTE-PARA-TRÁS (primeiro = topo).
                            // Smithay 0.7: space_render_elements põe Top/Overlay primeiro e
                            // Background/Bottom por último. O frame associado vai LOGO APÓS
                            // o render_element do seu cliente (ou seja, ABAIXO dele), mantendo
                            // o OutputDamageTracker — nada de draw direto no framebuffer.
                            let output_scale = output.current_scale().fractional_scale();
                            let mut elements: Vec<AeroRenderElements<GlesRenderer>> =
                                Vec::new();
                            #[cfg(feature = "bench")]
                            let mut titlebar_probe = None;
                            {
                                let layer_map = smithay::desktop::layer_map_for_output(output);
                                let (lower, upper): (Vec<_>, Vec<_>) = layer_map
                                    .layers()
                                    .rev()
                                    .partition(|s| matches!(s.layer(), Layer::Background | Layer::Bottom));

                                for layer in upper {
                                    if let Some(geo) = layer_map.layer_geometry(layer) {
                                        elements.extend(AsRenderElements::render_elements::<
                                            AeroRenderElements<GlesRenderer>,
                                        >(
                                            layer,
                                            renderer,
                                            geo.loc.to_physical_precise_round(output_scale),
                                            UtilsScale::from(output_scale),
                                            1.0,
                                        ));
                                    }
                                }

                                let mut placed_frames = Vec::new();
                                let mut render_order: Vec<String> = Vec::new();
                                for window in state.space.elements_for_output(output).rev() {
                                    let Some(geo) = state.space.element_geometry(window) else {
                                        continue;
                                    };
                                    // Render canônico do Smithay: a superfície é desenhada em
                                    // render_location = location - geometry.loc, para que o
                                    // conteúdo VISÍVEL (descontada a sombra CSD) caia em `location`.
                                    // Passar geo.loc direto desloca o conteúdo em geometry.loc.
                                    // Fonte: smithay-0.7.0/src/desktop/space/mod.rs:403,464.
                                    let render_loc = geo.loc - window.geometry().loc;
                                    elements.extend(AsRenderElements::render_elements::<
                                        AeroRenderElements<GlesRenderer>,
                                    >(
                                        window,
                                        renderer,
                                        render_loc.to_physical_precise_round(output_scale),
                                        UtilsScale::from(output_scale),
                                        1.0,
                                    ));

                                    // Frame deste cliente: depois do cliente = abaixo dele.
                                    let client_surface = window.toplevel().map(|t| t.wl_surface());
                                    render_order.push(format!("client@{:?}", client_surface.map(|s| s.id())));
                                    if let Some((frame_surface, _, _window_id, _)) = state
                                        .associated_frames
                                        .iter()
                                        .find(|(_, _, _, cs)| cs.as_ref() == client_surface)
                                    {
                                        let fgeo = state
                                            .frame_geometries
                                            .get(frame_surface)
                                            .copied()
                                            .unwrap_or_default();
                                        let (dx, dy) = fgeo.offset();
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> = smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                                            geo.loc.x + dx,
                                            geo.loc.y + dy,
                                        )
                                        .to_physical_precise_round(output_scale);
                                        let f = smithay::utils::Point::<f64, smithay::utils::Physical>::from((
                                            loc.x as f64,
                                            loc.y as f64,
                                        ));
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
                                        match element {
                                            Ok(Some(el)) => {
                                                // Alvo = conteúdo JÁ COMMITADO + insets/shadow.
                                                // O buffer da decoração é assíncrono (WebKit);
                                                // enquanto ele não alcança o alvo, escalamos no
                                                // render para manter a borda colada (ADR 0006).
                                                let target = smithay::utils::Size::<i32, smithay::utils::Logical>::new(
                                                    geo.size.w + fgeo.extra_w(),
                                                    geo.size.h + fgeo.extra_h(),
                                                );
                                                let buffer = el.buffer_size();
                                                // 02p: canvas fixo => TODO commit válido usa
                                                // 9-slice, inclusive quando buffer == target.
                                                // Não há caminho nativo/swap no release.
                                                let rescaled = buffer.w > 0 && buffer.h > 0;
                                                // 02k: linhas de corte = insets do frame (state.rs:36-56).
                                                // `shadow` é 0 nesta fase; se passar a existir,
                                                // entra nas fatias externas.
                                                let es = UtilsScale::from(output_scale);
                                                let elem_geo = el.geometry(es);
                                                let origin = elem_geo.loc;
                                                let buf_phys = elem_geo.size;
                                                let dst_phys: smithay::utils::Size<i32, smithay::utils::Physical> =
                                                    target.to_physical_precise_round(output_scale);
                                                let l = (fgeo.insets.0 as f64 * output_scale).round() as i32;
                                                let t = (fgeo.insets.1 as f64 * output_scale).round() as i32;
                                                let r = (fgeo.insets.2 as f64 * output_scale).round() as i32;
                                                let b = (fgeo.insets.3 as f64 * output_scale).round() as i32;
                                                let src_mid_w = (buf_phys.w - l - r).max(1);
                                                let src_mid_h = (buf_phys.h - t - b).max(1);
                                                let dst_mid_w = (dst_phys.w - l - r).max(1);
                                                let dst_mid_h = (dst_phys.h - t - b).max(1);
                                                // Só o interior estica: cantos fixos, laterais
                                                // esticam em Y, topo/base esticam em X.
                                                let slice_sx = dst_mid_w as f64 / src_mid_w as f64;
                                                let slice_sy = dst_mid_h as f64 / src_mid_h as f64;
                                                let delta = SlicePoint::new(loc.x - origin.x, loc.y - origin.y);
                                                // 02l: zonas protegidas declaradas pelo shell.
                                                // Somente o intervalo entre elas estica em X;
                                                // texto e botões mantêm a largura do buffer.
                                                let title_slices = (
                                                    fgeo.titlebar_slices.0.max(0),
                                                    fgeo.titlebar_slices.1.max(0),
                                                );
                                                let tl = (title_slices.0 as f64 * output_scale).round() as i32;
                                                let tr = (title_slices.1 as f64 * output_scale).round() as i32;
                                                // Nunca deixar o intervalo sem pixels: o Crop
                                                // de uma fatia vazia é descartado.
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
                                                // Fatias presas à borda direita/inferior do
                                                // alvo precisam do deslocamento da diferença
                                                // buffer->alvo; as presas à esquerda/topo usam
                                                // o deslocamento comum.
                                                let shift_x = dst_mid_w - src_mid_w;
                                                let shift_y = dst_mid_h - src_mid_h;
                                                let delta_rx = SlicePoint::new(delta.x + shift_x, delta.y);
                                                let delta_by = SlicePoint::new(delta.x, delta.y + shift_y);
                                                let delta_rb = SlicePoint::new(delta.x + shift_x, delta.y + shift_y);
                                                #[cfg(feature = "bench")]
                                                if *_window_id == 0 {
                                                    let region = smithay::utils::Rectangle::<
                                                        i32,
                                                        smithay::utils::Physical,
                                                    >::new(loc, (dst_phys.w, t).into());
                                                    let region = output
                                                        .current_mode()
                                                        .map(|mode| {
                                                            let visible = smithay::utils::Rectangle::from_size(
                                                                mode.size,
                                                            );
                                                            let region = region
                                                                .intersection(visible)
                                                                .unwrap_or(region);
                                                            output
                                                                .current_transform()
                                                                .invert()
                                                                .transform_rect_in(region, &mode.size)
                                                        })
                                                        .unwrap_or(region);
                                                    titlebar_probe = Some(
                                                        crate::frame_titlebar_probe::TitlebarFrame {
                                                            window_id: *_window_id,
                                                            region: smithay::utils::Rectangle::new(
                                                                smithay::utils::Point::new(
                                                                    region.loc.x,
                                                                    region.loc.y,
                                                                ),
                                                                smithay::utils::Size::new(
                                                                    region.size.w,
                                                                    region.size.h,
                                                                ),
                                                            ),
                                                            buffer: (buffer.w, buffer.h),
                                                            target: (target.w, target.h),
                                                            rescaled,
                                                            release_pending: false,
                                                        },
                                                    );
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
                                                        geo.size.w, geo.size.h, target.w, target.h, buffer.w, buffer.h,
                                                    );
                                                    // 02j: duração do episódio de escala,
                                                    // sem alterar o RescaleRenderElement.
                                                    let max_scale = if buffer.w > 0 && buffer.h > 0 {
                                                        (target.w as f64 / buffer.w as f64)
                                                            .max(target.h as f64 / buffer.h as f64)
                                                    } else {
                                                        0.0
                                                    };
                                                    state.frame_diag.note_stretch(
                                                        *_window_id,
                                                        rescaled
                                                            && state.resize_grab_active
                                                                == Some(*_window_id),
                                                        max_scale,
                                                        std::time::Instant::now(),
                                                        crate::frame_diagnostics::now_ns(),
                                                    );
                                                    // 02k: com 9-slice as fatias fixas mantêm os insets
                                                    // exatos (escala 1 no eixo fixo); as demais
                                                    // esticam só no eixo livre.
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
                                                        buffer.w, buffer.h, target.w, target.h,
                                                        fgeo.insets.0, fgeo.insets.1,
                                                        fgeo.insets.2, fgeo.insets.3,
                                                        fgeo.insets.0, fgeo.insets.2, fgeo.insets.1,
                                                        title_slices.0, title_slices.1,
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
                                                    // (crop na origem, âncora do rescale,
                                                    //  escala, deslocamento final)
                                                    let specs: [(
                                                        SliceRect,
                                                        SlicePoint,
                                                        (f64, f64),
                                                        SlicePoint,
                                                    ); 9] = [
                                                        // Topo: texto e botões NÃO esticam;
                                                        // só o intervalo entre eles muda.
                                                        (rect(x0, y0, l + tl, t), SlicePoint::new(x0, y0), (1.0, 1.0), delta),
                                                        (rect(x1 + tl, y0, src_gap_w, t), SlicePoint::new(x1 + tl, y0), (gap_sx, 1.0), delta),
                                                        (rect(x2 - tr, y0, r + tr, t), SlicePoint::new(x2 - tr, y0), (1.0, 1.0), delta_rx),
                                                        // 2 cantos inferiores: sem escala.
                                                        (rect(x0, y2, l, b), SlicePoint::new(x0, y2), (1.0, 1.0), delta_by),
                                                        (rect(x2, y2, r, b), SlicePoint::new(x2, y2), (1.0, 1.0), delta_rb),
                                                        // left/right: largura fixa, altura estica.
                                                        (rect(x0, y1, l, src_mid_h), SlicePoint::new(x0, y1), (1.0, slice_sy), delta),
                                                        (rect(x2, y1, r, src_mid_h), SlicePoint::new(x2, y1), (1.0, slice_sy), delta_rx),
                                                        // base: altura fixa, largura estica.
                                                        (rect(x1, y2, src_mid_w, b), SlicePoint::new(x1, y2), (slice_sx, 1.0), delta_by),
                                                        // centro (transparente sob o cliente).
                                                        (rect(x1, y1, src_mid_w, src_mid_h), SlicePoint::new(x1, y1), (slice_sx, slice_sy), delta),
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
                                                                renderer, frame_surface, states, f,
                                                                1.0, Kind::Unspecified,
                                                            )
                                                        });
                                                        let Ok(Some(slice)) = slice else {
                                                            continue;
                                                        };
                                                        let Some(cropped) =
                                                            CropRenderElement::from_element(slice, es, crop)
                                                        else {
                                                            continue;
                                                        };
                                                        let scaled = RescaleRenderElement::from_element(
                                                            cropped, anchor, UtilsScale::from((sxa, sya)),
                                                        );
                                                        let moved = RelocateRenderElement::from_element(
                                                            scaled, move_delta, Relocate::Relative,
                                                        );
                                                        // 02l: geometria REAL do elemento
                                                        // composto — não é o valor calculado
                                                        // para a instrumentação de cima.
                                                        #[cfg(feature = "bench")]
                                                        {
                                                            let g = moved.geometry(es);
                                                            eprintln!(
                                                                "SLICE_GEOM t={frame_t_nanos} window_id={_window_id} \
                                                                 i={} x={} y={} w={} h={}",
                                                                slice_idx - 1,
                                                                g.loc.x, g.loc.y, g.size.w, g.size.h,
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
                                                placed_frames.push(frame_surface.clone());
                                                render_order.push(format!("frame@{:?}", frame_surface.id()));
                                            }
                                            Ok(None) => {}
                                            Err(err) => {
                                                tracing::warn!(?err, "import do frame falhou")
                                            }
                                        }
                                    }
                                }

                                // Loga a ordem frente->trás só quando muda (map/raise/associate).
                                if render_order != state.last_render_order {
                                    tracing::info!(order = ?render_order, "ordem de render (frente->tras)");
                                    state.last_render_order = render_order;
                                }

                                // Frames sem cliente mapeado: abaixo de todas as janelas.
                                for layer in &state.frame_layer_surfaces {
                                    if !layer.alive() || placed_frames.contains(layer.wl_surface()) {
                                        continue;
                                    }
                                    let f = smithay::utils::Point::<f64, smithay::utils::Physical>::from((
                                        0.0,
                                        0.0,
                                    ));
                                    if let Ok(Some(el)) = with_states(layer.wl_surface(), |states| {
                                        WaylandSurfaceRenderElement::from_surface(
                                            renderer,
                                            layer.wl_surface(),
                                            states,
                                            f,
                                            1.0,
                                            Kind::Unspecified,
                                        )
                                    }) {
                                        elements.push(AeroRenderElements::Surface(el));
                                    }
                                }

                                for layer in lower {
                                    if let Some(geo) = layer_map.layer_geometry(layer) {
                                        elements.extend(AsRenderElements::render_elements::<
                                            AeroRenderElements<GlesRenderer>,
                                        >(
                                            layer,
                                            renderer,
                                            geo.loc.to_physical_precise_round(output_scale),
                                            UtilsScale::from(output_scale),
                                            1.0,
                                        ));
                                    }
                                }
                            }

                            match tracker.render_output(
                                renderer,
                                &mut framebuffer,
                                0,
                                &elements,
                                if std::env::var_os("AERO_DEBUG_CLEAR_LIGHT").is_some() {
                                    [0.95, 0.95, 0.95, 1.0]
                                } else {
                                    [0.08, 0.08, 0.10, 1.0]
                                },
                            ) {
                                Ok(rendered) => {
                                    // 02o: ler os pixels REAIS já compostos, antes do submit.
                                    // Só sob `bench` e com AERO_PIXEL_DUMP_DIR definida.
                                    #[cfg(feature = "bench")]
                                    crate::frame_pixel_probe::capture(
                                        renderer,
                                        &framebuffer,
                                        0,
                                        false,
                                    );
                                    #[cfg(feature = "bench")]
                                    if let Some(frame) = titlebar_probe {
                                        crate::frame_titlebar_probe::capture(
                                            renderer,
                                            &framebuffer,
                                            frame,
                                        );
                                    }
                                    Some(rendered)
                                }
                                Err(err) => {
                                    tracing::warn!("render_output falhou: {err}");
                                    None
                                }
                            }
                        }
                        Err(err) => {
                            tracing::warn!("bind falhou: {err}");
                            None
                        }
                    };

                    if let Some(rendered) = rendered {
                        if let Some(damage) = rendered.damage {
                            if let Err(err) = backend.submit(Some(damage)) {
                                tracing::warn!("submit falhou: {err}");
                            }
                        }
                        let now = state.start_time.elapsed();
                        state.space.elements().for_each(|window| {
                            window.send_frame(output, now, Some(Duration::ZERO), |_, _| {
                                Some(output.clone())
                            })
                        });
                        for layer in &state.layer_surfaces {
                            if layer.alive() {
                                layer.send_frame(output, now, Some(Duration::ZERO), |_, _| {
                                    Some(output.clone())
                                });
                            }
                        }
                        for layer in &state.frame_layer_surfaces {
                            if layer.alive() {
                                layer.send_frame(output, now, Some(Duration::ZERO), |_, _| {
                                    Some(output.clone())
                                });
                            }
                        }
                        if let Some((action, started)) = state.last_geometry_action.take() {
                            tracing::info!(
                                action = %action,
                                latency_ms = started.elapsed().as_secs_f64() * 1000.0,
                                "S5: resize/move → frame atualizado"
                            );
                        }
                        state.space.refresh();
                        state.popups.cleanup();
                        let _ = data.display_handle.flush_clients();
                    }

                    backend
                        .window()
                        .set_cursor(if state.cursor_over_interactive {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        });
                    backend.window().request_redraw();
                }
                WinitEvent::CloseRequested => state.loop_signal.stop(),
                _ => {}
            }
        })?;

    Ok(())
}
