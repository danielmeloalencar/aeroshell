//! Backend nested: janela no compositor host via winit.

use std::time::Duration;

#[cfg(feature = "bench")]
use smithay::{
    utils::{Physical, Size},
    wayland::{compositor::SurfaceAttributes, fractional_scale::with_fractional_scale},
};
#[cfg(feature = "bench")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "bench")]
static WINIT_FSCALE_LOG: AtomicU64 = AtomicU64::new(0);

use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            damage::OutputDamageTracker,
            element::{
                surface::WaylandSurfaceRenderElement, texture::TextureRenderElement,
                AsRenderElements, Id, Kind,
            },
            gles::{GlesRenderer, GlesTexture},
            Bind, Offscreen, Renderer, Texture,
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
    render::{
        elements::{AeroRenderElements, FrameRenderInput, FrameRenderKind},
        scene::collect_output_elements,
    },
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

// A composição do host é especializada em GlesTexture; a cena de cada output
// continua usando o enum genérico compartilhado com o headless.
smithay::backend::renderer::element::render_elements! {
    ComposedElements<=GlesRenderer>;
    Texture = TextureRenderElement<GlesTexture>,
}

pub fn init_winit(
    opts: &Options,
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit_source) = winit::init::<GlesRenderer>()?;

    let multi = opts.outputs > 1;
    let initial_host_size = backend.window_size();
    let size = opts
        .size
        .map(|(w, h)| (w as i32, h as i32).into())
        .unwrap_or_else(|| {
            if multi {
                // Sem --size, mantemos a janela host no tamanho inicial e
                // dividimos sua largura entre os outputs.
                (
                    initial_host_size.w / opts.outputs as i32,
                    initial_host_size.h,
                )
                    .into()
            } else {
                initial_host_size
            }
        });
    let host_size = if multi {
        (size.w * opts.outputs as i32, size.h).into()
    } else {
        size
    };

    // winit 0.30 não expõe Window::set_size. Fixamos min=max para forçar o
    // tamanho pedido; N>1 usa um tile horizontal por output na mesma janela.
    if opts.size.is_some() || multi {
        let requested = PhysicalSize::new(host_size.w as u32, host_size.h as u32);
        backend.window().set_min_inner_size(Some(requested));
        backend.window().set_max_inner_size(Some(requested));
        if multi {
            // min/max NÃO muda o tamanho de uma janela já criada nesta VM.
            // A resposta pode ser None: nesse caso o tamanho real vem em Resized.
            let reported = backend.window().request_inner_size(requested);
            tracing::info!(?reported, "resize explícito da janela multi-output");
        }
    }

    let mode = Mode {
        size,
        refresh: 60_000,
    };

    tracing::info!(
        requested = ?opts.size,
        window = ?backend.window_size(),
        mode = ?size,
        host = ?host_size,
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
    data.state.resend_scales();

    let mut damage_trackers: Vec<_> = outputs
        .iter()
        .map(OutputDamageTracker::from_output)
        .collect();

    // F1c: somente N>1 sai do caminho direto. Cada textura é um backbuffer
    // persistente (age=1); o host é recomposto por inteiro a cada redraw/vsync.
    let mut output_textures: Vec<GlesTexture> = Vec::new();
    if multi {
        for _ in &outputs {
            let texture = <GlesRenderer as Offscreen<GlesTexture>>::create_buffer(
                backend.renderer(),
                Fourcc::Abgr8888,
                smithay::utils::Size::<i32, smithay::utils::Buffer>::new(size.w, size.h),
            )?;
            output_textures.push(texture);
        }
    }
    let mut output_ages = vec![0usize; outputs.len()];
    // As texturas dos outputs já foram renderizadas com Flipped180. Aplicar a
    // mesma transformação no host inverte a apresentação pela segunda vez.
    let mut host_tracker =
        OutputDamageTracker::new(host_size, UtilsScale::from(1.0), Transform::Normal);
    #[cfg(feature = "bench")]
    let mut redraw_seq = 0u64;
    #[cfg(feature = "bench")]
    let mut n1_present_seq = 0u64;
    #[cfg(feature = "bench")]
    let mut host_probe_done = false;
    let fixed_host_size = opts.size.is_some() || multi;

    event_loop
        .handle()
        .insert_source(winit_source, move |event, _, data| {
            let state = &mut data.state;
            match event {
                WinitEvent::Resized { size, .. } => {
                    // Sem `--size`, a janela host é a fonte da verdade. Com `--size`,
                    // mantemos o modo pedido: winit 0.30 não permite redimensionar a janela.
                    if !fixed_host_size {
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
                    if state.outputs.len() > 1 {
                        let outputs = state.outputs.clone();
                        #[cfg(feature = "bench")]
                        let seq = {
                            redraw_seq += 1;
                            redraw_seq
                        };
                        let clear = if std::env::var_os("AERO_DEBUG_CLEAR_LIGHT").is_some() {
                            [0.95, 0.95, 0.95, 1.0]
                        } else {
                            [0.08, 0.08, 0.10, 1.0]
                        };

                        // Todas as operações offscreen acontecem ANTES de
                        // backend.bind(): map/bind trocam a surface EGL atual.
                        let mut scenes = Vec::new();
                        for output in &outputs {
                            scenes.push(collect_output_elements(
                                state,
                                output,
                                backend.renderer(),
                                false,
                            ));
                        }

                        let context = backend.renderer().context_id();
                        let mut composed: Vec<ComposedElements> = Vec::new();
                        for (index, _output) in outputs.iter().enumerate() {
                            let age = output_ages[index];
                            let (elements, _) = &scenes[index];
                            let (damage_text, output_ok) = {
                                let renderer = backend.renderer();
                                match renderer.bind(&mut output_textures[index]) {
                                    Ok(mut target) => match damage_trackers[index].render_output(
                                        renderer, &mut target, age, elements, clear,
                                    ) {
                                        Ok(rendered) => {
                                            let text = match rendered.damage {
                                                Some(rects) if !rects.is_empty() => rects
                                                    .iter()
                                                    .map(|rect| {
                                                        format!(
                                                            "{}x{}@{},{}",
                                                            rect.size.w,
                                                            rect.size.h,
                                                            rect.loc.x,
                                                            rect.loc.y
                                                        )
                                                    })
                                                    .collect::<Vec<_>>()
                                                    .join(";"),
                                                _ => "none".to_owned(),
                                            };
                                            if rendered.damage.is_some() {
                                                output_ages[index] = 1;
                                            }
                                            (text, true)
                                        }
                                        Err(err) => {
                                            tracing::warn!(?err, "render_output do output falhou");
                                            ("error".to_owned(), false)
                                        }
                                    },
                                    Err(err) => {
                                        tracing::warn!(?err, "bind do output offscreen falhou");
                                        ("error".to_owned(), false)
                                    }
                                }
                            };
                            if !output_ok {
                                // O próximo frame deve refazer integralmente este alvo.
                                output_ages[index] = 0;
                            }
                            #[cfg(feature = "bench")]
                            eprintln!(
                                "OUTPUT_FRAME t={} output={} seq={seq} age={age} damage={damage_text}",
                                crate::frame_diagnostics::now_ns(), _output.name()
                            );
                            #[cfg(not(feature = "bench"))]
                            let _ = damage_text;

                            let texture_size = output_textures[index].size();
                            composed.push(ComposedElements::Texture(
                                TextureRenderElement::from_static_texture(
                                    Id::new(),
                                    context.clone(),
                                    smithay::utils::Point::<
                                        f64,
                                        smithay::utils::Physical,
                                    >::new(
                                        index as f64 * size.w as f64,
                                        0.0,
                                    ),
                                    output_textures[index].clone(),
                                    1,
                                    Transform::Normal,
                                    Some(1.0),
                                    Some(smithay::utils::Rectangle::<
                                        f64,
                                        smithay::utils::Logical,
                                    >::from_size(
                                        smithay::utils::Size::new(
                                            texture_size.w as f64,
                                            texture_size.h as f64,
                                        ),
                                    )),
                                    Some(smithay::utils::Size::<
                                        i32,
                                        smithay::utils::Logical,
                                    >::new(size.w, size.h)),
                                    None,
                                    Kind::Unspecified,
                                ),
                            ));
                        }

                        let mut window_damage = None;
                        {
                            // Mesmo com todos os outputs estáveis, o host faz
                            // full compose/submit: o swap fornece o vsync e evita
                            // busy-loop; o skip incremental fica nas texturas.
                            #[cfg(feature = "bench")]
                            let host_size_matches = backend.window_size() == host_size;
                            match backend.bind() {
                                Ok((renderer, mut framebuffer)) => {
                                    // Texturas estáticas não carregam o damage
                                    // offscreen: o host é full por decisão explícita.
                                    match host_tracker.render_output(
                                        renderer,
                                        &mut framebuffer,
                                        0,
                                        &composed,
                                        clear,
                                    ) {
                                        Ok(rendered) => {
                                            #[cfg(feature = "bench")]
                                            if !host_probe_done
                                                && host_size_matches
                                                && scenes[0].1.is_some()
                                            {
                                                // UMA captura por execução com frame real:
                                                // mede a orientação final do host, não a textura isolada.
                                                crate::frame_pixel_probe::capture(
                                                    renderer,
                                                    &framebuffer,
                                                    0,
                                                    false,
                                                );
                                                host_probe_done = true;
                                            }
                                            window_damage = rendered.damage.cloned();
                                        }
                                        Err(err) => {
                                            tracing::warn!(?err, "composição do host falhou");
                                        }
                                    }
                                }
                                Err(err) => {
                                    tracing::warn!(?err, "bind do host falhou");
                                }
                            }
                        }

                        if let Some(damage) = window_damage {
                            if let Err(err) = backend.submit(Some(&damage)) {
                                tracing::warn!("submit falhou: {err}");
                            }
                        }

                        let now = state.start_time.elapsed();
                        for output in &outputs {
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
                        backend
                            .window()
                            .set_cursor(if state.cursor_over_interactive {
                                CursorIcon::Pointer
                            } else {
                                CursorIcon::Default
                            });
                        backend.window().request_redraw();
                        return;
                    }

                    // F0/S1: caminho direto de UM output, inalterado pela F1c.
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

                                        // F1f: o nested N=1 tem caminho próprio (nao
                                        // passa por render::scene); medir os mesmos
                                        // campos para comparar com o udev/DRM.
                                        #[cfg(feature = "bench")]
                                        if WINIT_FSCALE_LOG
                                            .fetch_add(1, Ordering::Relaxed)
                                            .is_multiple_of(120)
                                        {
                                            let client_scale = client_surface
                                                .as_ref()
                                                .map(|s| {
                                                    with_states(s, |st| {
                                                        with_fractional_scale(st, |fs| {
                                                            fs.preferred_scale().unwrap_or(output_scale)
                                                        })
                                                    })
                                                })
                                                .unwrap_or(output_scale);
                                            let foot_bs = client_surface
                                                .as_ref()
                                                .map(|s| {
                                                    with_states(s, |st| {
                                                        st.cached_state
                                                            .get::<SurfaceAttributes>()
                                                            .current()
                                                            .buffer_scale
                                                    })
                                                })
                                                .unwrap_or(-1);
                                            let foot_dst: Size<i32, Physical> =
                                                geo.size.to_physical_precise_round(client_scale);
                                            let frame_dst: Size<i32, Physical> =
                                                crate::render::elements::frame_target(geo.size, &fgeo)
                                                    .to_physical_precise_round(output_scale);
                                            let t = crate::frame_diagnostics::now_ns();
                                            eprintln!(
                                                "FSCALE_FRAME t={t} backend=winit-n1 output={} window_id={} output_scale={} client_scale={} foot_buffer_scale={} foot_dst={:?} frame_dst={:?} frame_loc={:?}",
                                                output.name(), *_window_id, output_scale,
                                                client_scale, foot_bs, foot_dst, frame_dst, loc
                                            );
                                        }

                                        // F1a: o 9-slice/canvas/placeholder vive em
                                        // render/elements.rs, agnóstico de backend.
                                        let result = crate::render::elements::push_frame_elements(
                                            renderer,
                                            &mut elements,
                                            FrameRenderInput {
                                                surface: frame_surface,
                                                fgeo,
                                                loc,
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
                                    } else {
                                        // 02q: nenhum WebView associado ainda. A janela
                                        // mapeada recebe a decoração sólida do compositor
                                        // com as insets anunciadas pelo shell no startup.
                                        let mut window_id = 0;
                                        for (id, surface) in state.window_surfaces.iter() {
                                            if client_surface == Some(surface) {
                                                window_id = *id;
                                                break;
                                            }
                                        }
                                        // Enquanto o cliente não comita, a geometria do
                                        // Space é 0×0. Usa o tamanho SUGERIDO do configure
                                        // inicial como geometria provisória do placeholder.
                                        let content = if geo.size.w > 0 && geo.size.h > 0 {
                                            geo.size
                                        } else {
                                            state
                                                .suggested_window_size
                                                .get(&window_id)
                                                .copied()
                                                .unwrap_or(geo.size)
                                        };
                                        let fgeo = crate::state::FrameGeometry {
                                            insets: state.default_frame_insets,
                                            ..Default::default()
                                        };
                                        let (dx, dy) = fgeo.offset();
                                        let loc =
                                            smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                                                geo.loc.x + dx,
                                                geo.loc.y + dy,
                                            )
                                            .to_physical_precise_round(output_scale);
                                        crate::render::elements::push_window_placeholder(
                                            &mut elements,
                                            &fgeo,
                                            loc,
                                            content,
                                            output_scale,
                                            geo.loc,
                                            window_id,
                                        );
                                        render_order.push(format!("placeholder-sem-frame@{window_id}"));
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
                        #[cfg(feature = "bench")]
                        {
                            n1_present_seq += 1;
                            eprintln!(
                                "OUTPUT_FRAME t={} output=n1 seq={n1_present_seq} age=0 damage=submitted",
                                crate::frame_diagnostics::now_ns()
                            );
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
