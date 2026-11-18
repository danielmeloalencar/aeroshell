//! Backend nested: janela no compositor host via winit.

use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{calloop::EventLoop, winit::dpi::PhysicalSize},
    utils::{IsAlive, Transform},
};

use crate::{cli::Options, CalloopData};

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
        data.state
            .space
            .map_output(&output, (i as i32 * size.w, 0));
        outputs.push(output);
    }
    data.state.outputs = outputs.clone();

    let mut damage_trackers: Vec<_> = outputs.iter().map(OutputDamageTracker::from_output).collect();
    let forced_size = opts.size;

    event_loop.handle().insert_source(winit_source, move |event, _, data| {
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
                        // `render_output` consulta o LayerMap do output: não desenhamos
                        // layer surfaces manualmente (isso causava o painel invisível).
                        match smithay::desktop::space::render_output::<
                            _,
                            WaylandSurfaceRenderElement<GlesRenderer>,
                            _,
                            _,
                        >(
                            output,
                            renderer,
                            &mut framebuffer,
                            1.0,
                            0,
                            [&state.space],
                            &[],
                            tracker,
                            if std::env::var_os("AERO_DEBUG_CLEAR_LIGHT").is_some() {
                                [0.95, 0.95, 0.95, 1.0]
                            } else {
                                [0.08, 0.08, 0.10, 1.0]
                            },
                        ) {
                            Ok(rendered) => Some(rendered),
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
                        window.send_frame(
                            output,
                            now,
                            Some(Duration::ZERO),
                            |_, _| Some(output.clone()),
                        )
                    });
                    for layer in &state.layer_surfaces {
                        if layer.alive() {
                            layer.send_frame(
                                output,
                                now,
                                Some(Duration::ZERO),
                                |_, _| Some(output.clone()),
                            );
                        }
                    }
                    state.space.refresh();
                    state.popups.cleanup();
                    let _ = data.display_handle.flush_clients();
                }

                backend.window().request_redraw();
            }
            WinitEvent::CloseRequested => state.loop_signal.stop(),
            _ => {}
        }
    })?;

    Ok(())
}
