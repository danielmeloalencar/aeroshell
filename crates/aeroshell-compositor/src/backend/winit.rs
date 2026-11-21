//! Backend nested: janela no compositor host via winit.

use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            element::{surface::WaylandSurfaceRenderElement, AsRenderElements, Kind},
            gles::GlesRenderer,
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

use crate::{
    cli::Options,
    render::elements::{AeroRenderElements, FrameRenderInput, FrameRenderKind},
    CalloopData,
};

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
                                        let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                                                geo.loc.x + dx,
                                                geo.loc.y + dy,
                                            )
                                            .to_physical_precise_round(output_scale);

                                        // F1a: o 9-slice/canvas/placeholder vive em
                                        // render/elements.rs, agnóstico de backend.
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
                                                resize_active: state.resize_grab_active
                                                    == Some(*_window_id),
                                            },
                                            #[cfg(feature = "bench")]
                                            &mut state.frame_diag,
                                        );
                                        if let Some(result) = result {
                                            placed_frames.push(frame_surface.clone());
                                            let kind = match result.kind {
                                                FrameRenderKind::Frame => "frame",
                                                FrameRenderKind::Placeholder => "placeholder",
                                            };
                                            render_order
                                                .push(format!("{kind}@{:?}", frame_surface.id()));
                                            #[cfg(feature = "bench")]
                                            if let Some(probe) = result.titlebar_probe {
                                                titlebar_probe = Some(probe);
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
