//! Backend headless para CI/testes: socket e estado vivos, com render EGL
//! surfaceless (F1b) e damage por output — sem janela nativa, roda na VM sem 3D.
//! Aceita comandos de ponteiro sintético por Unix socket (`AERO_TEST_INPUT_SOCK`):
//!   motion <x> <y>\n   button press|release\n

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
    utils::{Buffer as BufferCoord, IsAlive, Scale as UtilsScale, Size, Transform},
    wayland::{compositor::with_states, shell::wlr_layer::Layer},
};

use crate::{
    cli::Options,
    render::elements::{AeroRenderElements, FrameRenderInput},
    CalloopData,
};

#[cfg(feature = "bench")]
use crate::render::elements::FrameRenderKind;

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

    // F1b: EGL surfaceless + GlesRenderer, sem janela nativa (llvmpipe nesta VM).
    // SAFETY: `EGLSurfacelessDisplay` usa a plataforma EGL_MESA_platform_surfaceless
    // (egl/native.rs:264-275), sem drawable nativo; o display permanece vivo dentro
    // do `EGLContext` e do renderer pelo resto do processo.
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay)? };
    let context = EGLContext::new(&display)?;
    // SAFETY: o contexto foi criado a partir do display surfaceless válido acima e
    // passa a pertencer ao renderer; nenhum outro contexto EGL é usado nesta thread.
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    let buffer_size: Size<i32, BufferCoord> = Size::new(size.w, size.h);
    let mut renderbuffer = renderer.create_buffer(Fourcc::Abgr8888, buffer_size)?;
    let mut tracker = OutputDamageTracker::from_output(&output);
    // O renderbuffer é persistente (não há swapchain): depois do primeiro draw,
    // o conteúdo disponível é sempre o da tentativa anterior, equivalente a age=1.
    let mut age = 0;

    let timer = Timer::from_duration(Duration::from_millis(16));
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            render_headless(
                data,
                &output,
                &mut renderer,
                &mut renderbuffer,
                &mut tracker,
                &mut age,
            );
            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    tracing::info!("backend headless ativo (render EGL surfaceless)");
    Ok(())
}

/// Monta a cena com a MESMA API de frame do winit (`push_frame_elements`) e
/// renderiza num `GlesRenderbuffer` offscreen com `OutputDamageTracker` por output.
fn render_headless(
    data: &mut CalloopData,
    output: &Output,
    renderer: &mut GlesRenderer,
    renderbuffer: &mut GlesRenderbuffer,
    tracker: &mut OutputDamageTracker,
    age: &mut usize,
) {
    let state = &mut data.state;
    let output_scale = output.current_scale().fractional_scale();
    let mut elements: Vec<AeroRenderElements<GlesRenderer>> = Vec::new();

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
        for window in state.space.elements_for_output(output).rev() {
            let Some(geo) = state.space.element_geometry(window) else {
                continue;
            };
            // Mesma regra do winit: render_location = location - geometry.loc.
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

            let client_surface = window.toplevel().map(|t| t.wl_surface());
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
                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                    smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                        geo.loc.x + dx,
                        geo.loc.y + dy,
                    )
                    .to_physical_precise_round(output_scale);
                let result = crate::render::elements::push_frame_elements(
                    renderer,
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
                        output,
                        window_id: *_window_id,
                        #[cfg(feature = "bench")]
                        resize_active: state.resize_grab_active == Some(*_window_id),
                    },
                    #[cfg(feature = "bench")]
                    &mut state.frame_diag,
                );
                if let Some(_result) = result {
                    placed_frames.push(frame_surface.clone());
                    // Prova do render compartilhado (F1b): o `kind` vem do MESMO
                    // `push_frame_elements` usado pelo winit. Log só sob `bench`.
                    #[cfg(feature = "bench")]
                    {
                        let kind = match _result.kind {
                            FrameRenderKind::Frame => "frame",
                            FrameRenderKind::Placeholder => "placeholder",
                        };
                        eprintln!("RENDER_SHARED window_id={_window_id} kind={kind}");
                    }
                }
            }
        }

        // Frames sem cliente mapeado: abaixo de todas as janelas.
        for layer in &state.frame_layer_surfaces {
            if !layer.alive() || placed_frames.contains(layer.wl_surface()) {
                continue;
            }
            let f = smithay::utils::Point::<f64, smithay::utils::Physical>::from((0.0, 0.0));
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

    let mut target = match renderer.bind(renderbuffer) {
        Ok(target) => target,
        Err(err) => {
            tracing::warn!("headless bind falhou: {err}");
            return;
        }
    };
    match tracker.render_output(
        renderer,
        &mut target,
        *age,
        &elements,
        [0.08, 0.08, 0.10, 1.0],
    ) {
        Ok(rendered) => {
            let had_damage = rendered.damage.is_some();
            #[cfg(feature = "bench")]
            {
                let damage = match rendered.damage {
                    None => "none".to_owned(),
                    Some(rects) if rects.is_empty() => "none".to_owned(),
                    Some(rects) => rects
                        .iter()
                        .map(|r| format!("{}x{}@{},{}", r.size.w, r.size.h, r.loc.x, r.loc.y))
                        .collect::<Vec<_>>()
                        .join(";"),
                };
                eprintln!(
                    "HEADLESS_FRAME output={} age={age} damage={damage}",
                    output.name()
                );
                // 02o: pixels REAIS do alvo offscreen, só com AERO_PIXEL_DUMP_DIR.
                crate::frame_pixel_probe::capture(renderer, &target, 0, false);
            }
            if had_damage {
                *age = 1;
            }
        }
        Err(err) => {
            tracing::warn!("headless render_output falhou: {err}");
            return;
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
    state.space.refresh();
    state.popups.cleanup();
    let _ = data.display_handle.flush_clients();
}
