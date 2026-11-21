//! Backend headless para CI/testes: render EGL surfaceless + damage por output.
//!
//! Aceita comandos de ponteiro sintético por Unix socket (`AERO_TEST_INPUT_SOCK`):
//!   motion <x> <y>\n   button press|release\n
//!
//! Diferente do backend winit (que desenha numa janela do host), aqui o alvo é
//! um `GlesRenderbuffer` offscreen. A montagem da cena usa o MESMO módulo
//! `render::elements` do nested — inclusive o 9-slice/canvas/placeholder do
//! frame (F1a). Nada disto exige udev/DRM/GBM/apt ou novas features.

use std::{error::Error, time::Duration};

use calloop::{
    timer::{TimeoutAction, Timer},
    EventLoop,
};
use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{native::EGLSurfacelessDisplay, EGLContext, EGLDisplay},
        renderer::{
            damage::OutputDamageTracker,
            element::{surface::WaylandSurfaceRenderElement, AsRenderElements, Kind},
            gles::{GlesRenderbuffer, GlesRenderer},
            Bind, Offscreen,
        },
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    utils::{IsAlive, Logical, Point, Scale as UtilsScale, Size, Transform},
    wayland::{compositor::with_states, shell::wlr_layer::Layer},
};

use crate::{
    cli::Options,
    render::elements::{AeroRenderElements, FrameRenderInput, FrameRenderKind},
    CalloopData,
};

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const CLEAR_COLOR: [f32; 4] = [0.08, 0.08, 0.10, 1.0];

pub struct HeadlessBackend;

impl super::Backend for HeadlessBackend {
    fn name(&self) -> &'static str {
        "headless"
    }
    fn seat_name(&self) -> String {
        "aeroshell-headless".to_owned()
    }
}

pub fn init_headless(
    opts: &Options,
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    let size = opts
        .size
        .map(|(w, h)| (w as i32, h as i32).into())
        .unwrap_or((1280, 800).into());
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        "aero-headless".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "AeroShell".into(),
            model: "headless".into(),
        },
    );
    let _global = output.create_global::<crate::state::AeroState>(&data.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    data.state.space.map_output(&output, (0, 0));
    data.state.outputs = vec![output.clone()];

    // EGL surfaceless: `EGLSurfacelessDisplay` usa EGL_MESA_platform_surfaceless
    // (smithay-0.7.0/src/backend/egl/native.rs:262-275). Já disponível porque a
    // feature `backend_winit` habilita `backend_egl` e `renderer_gl`.
    // SAFETY: o native display `EGLSurfacelessDisplay` é um tipo ZST válido da
    // própria Smithay; nenhum fd/ponteiro externo é emprestado e o `EGLDisplay`
    // resultante é dono do recurso pelo tempo desta função e do event source.
    let egl_display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let egl_context = EGLContext::new(&egl_display)?;
    // SAFETY: o contexto foi recém-criado aqui, é usado apenas nesta thread e
    // não está ativo em outra; `GlesRenderer::new` exige exatamente isso
    // (smithay-0.7.0/src/backend/renderer/gles/mod.rs:460-470).
    let mut renderer = unsafe { GlesRenderer::new(egl_context)? };

    let target_size = Size::<i32, smithay::utils::Buffer>::new(size.w, size.h);
    let mut renderbuffer: GlesRenderbuffer =
        renderer.create_buffer(Fourcc::Argb8888, target_size)?;
    let mut tracker = OutputDamageTracker::from_output(&output);
    #[cfg(feature = "bench")]
    let mut frame_no = 0u64;

    event_loop
        .handle()
        .insert_source(Timer::from_duration(FRAME_INTERVAL), move |_, _, data| {
            #[cfg(feature = "bench")]
            {
                frame_no += 1;
            }
            let output_scale = output.current_scale().fractional_scale();
            let mut elements: Vec<AeroRenderElements<GlesRenderer>> = Vec::new();
            let mut placed_frames = Vec::new();

            {
                let layer_map = smithay::desktop::layer_map_for_output(&output);
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
                            &mut renderer,
                            geo.loc.to_physical_precise_round(output_scale),
                            UtilsScale::from(output_scale),
                            1.0,
                        ));
                    }
                }

                for window in data.state.space.elements_for_output(&output).rev() {
                    let Some(geo) = data.state.space.element_geometry(window) else {
                        continue;
                    };
                    let render_loc = geo.loc - window.geometry().loc;
                    elements.extend(AsRenderElements::render_elements::<
                        AeroRenderElements<GlesRenderer>,
                    >(
                        window,
                        &mut renderer,
                        render_loc.to_physical_precise_round(output_scale),
                        UtilsScale::from(output_scale),
                        1.0,
                    ));

                    let client_surface = window.toplevel().map(|t| t.wl_surface());
                    if let Some((frame_surface, _, window_id, _)) = data
                        .state
                        .associated_frames
                        .iter()
                        .find(|(_, _, _, cs)| cs.as_ref() == client_surface)
                    {
                        let fgeo = data
                            .state
                            .frame_geometries
                            .get(frame_surface)
                            .copied()
                            .unwrap_or_default();
                        let (dx, dy) = fgeo.offset();
                        let loc = Point::<i32, Logical>::new(geo.loc.x + dx, geo.loc.y + dy)
                            .to_physical_precise_round(output_scale);

                        // F1b: o MESMO caminho de render do nested (F1a).
                        let result = crate::render::elements::push_frame_elements(
                            &mut renderer,
                            &mut elements,
                            FrameRenderInput {
                                surface: frame_surface,
                                fgeo,
                                loc,
                                #[cfg(feature = "bench")]
                                client_origin: geo.loc,
                                committed: geo.size,
                                output_scale,
                                #[cfg(feature = "bench")]
                                output: &output,
                                window_id: *window_id,
                                #[cfg(feature = "bench")]
                                resize_active: data.state.resize_grab_active == Some(*window_id),
                            },
                            #[cfg(feature = "bench")]
                            &mut data.state.frame_diag,
                        );
                        if let Some(result) = result {
                            placed_frames.push(frame_surface.clone());
                            let kind = match result.kind {
                                FrameRenderKind::Frame => "frame",
                                FrameRenderKind::Placeholder => "placeholder",
                            };
                            #[cfg(feature = "bench")]
                            {
                                let t = crate::frame_diagnostics::now_ns();
                                eprintln!(
                                    "RENDER_SHARED t={t} frame={frame_no} window_id={window_id} kind={kind}"
                                );
                            }
                            #[cfg(not(feature = "bench"))]
                            let _ = kind;
                        }
                    }
                }

                for layer in &data.state.frame_layer_surfaces {
                    if !layer.alive() || placed_frames.contains(layer.wl_surface()) {
                        continue;
                    }
                    let f = Point::<f64, smithay::utils::Physical>::from((0.0, 0.0));
                    if let Ok(Some(el)) = with_states(layer.wl_surface(), |states| {
                        WaylandSurfaceRenderElement::from_surface(
                            &mut renderer,
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
                            &mut renderer,
                            geo.loc.to_physical_precise_round(output_scale),
                            UtilsScale::from(output_scale),
                            1.0,
                        ));
                    }
                }
            }

            let rendered = {
                let mut framebuffer = match renderer.bind(&mut renderbuffer) {
                    Ok(fb) => fb,
                    Err(err) => {
                        tracing::warn!(?err, "headless: bind falhou");
                        return TimeoutAction::ToDuration(FRAME_INTERVAL);
                    }
                };
                let rendered = match tracker.render_output(
                    &mut renderer,
                    &mut framebuffer,
                    0,
                    &elements,
                    CLEAR_COLOR,
                ) {
                    Ok(rendered) => Some(rendered),
                    Err(err) => {
                        tracing::warn!(?err, "headless: render_output falhou");
                        None
                    }
                };
                #[cfg(feature = "bench")]
                if rendered.is_some() && frame_no <= 5 {
                    crate::frame_pixel_probe::capture(&mut renderer, &framebuffer, 0, false);
                }
                rendered
            };

            #[cfg(feature = "bench")]
            if let Some(rendered) = &rendered {
                let damage = match &rendered.damage {
                    Some(rects) if !rects.is_empty() => rects
                        .iter()
                        .map(|r| {
                            format!("{}x{}@{},{}", r.size.w, r.size.h, r.loc.x, r.loc.y)
                        })
                        .collect::<Vec<_>>()
                        .join(";"),
                    _ => "none".to_owned(),
                };
                let t = crate::frame_diagnostics::now_ns();
                eprintln!(
                    "HEADLESS_FRAME t={t} output={} frame={frame_no} damage={damage} elements={}",
                    output.name(),
                    elements.len()
                );
            }
            #[cfg(not(feature = "bench"))]
            let _ = rendered;

            let now = data.state.start_time.elapsed();
            data.state.space.elements().for_each(|window| {
                window.send_frame(&output, now, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                })
            });
            for layer in &data.state.layer_surfaces {
                if layer.alive() {
                    layer.send_frame(&output, now, Some(Duration::ZERO), |_, _| {
                        Some(output.clone())
                    });
                }
            }
            for layer in &data.state.frame_layer_surfaces {
                if layer.alive() {
                    layer.send_frame(&output, now, Some(Duration::ZERO), |_, _| {
                        Some(output.clone())
                    });
                }
            }
            data.state.space.refresh();
            data.state.popups.cleanup();
            let _ = data.display_handle.flush_clients();

            TimeoutAction::ToDuration(FRAME_INTERVAL)
        })?;

    tracing::info!("backend headless com render EGL surfaceless ativo");
    Ok(())
}
