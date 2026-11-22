//! Cena de UM output, frente->trás, compartilhada por headless, winit (N>1) e udev.
//!
//! F1d3: o mesmo `push_frame_elements`, a mesma ordem de layers/clientes/frames e o
//! mesmo formato de log em todos os backends. Esta árvore não conhece winit/udev;
//! só Smithay e `AeroState`. O caminho N=1 do winit mantém sua cópia especializada
//! (age=0, cor de clear configurável) para não regredir a validação visual F0/F1c.

#[cfg(feature = "bench")]
use smithay::utils::Physical;
#[cfg(feature = "bench")]
use std::sync::atomic::{AtomicU64, Ordering};

use smithay::{
    backend::renderer::{
        buffer_dimensions,
        element::{surface::WaylandSurfaceRenderElement, AsRenderElements, Kind},
        gles::GlesRenderer,
    },
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{IsAlive, Logical, Scale as UtilsScale, Size},
    wayland::{
        compositor::{with_states, BufferAssignment, SurfaceAttributes},
        fractional_scale::with_fractional_scale,
        shell::wlr_layer::Layer,
        viewporter::ViewportCachedState,
    },
};

use crate::{
    render::elements::{AeroRenderElements, FrameRenderInput, FrameRenderKind},
    state::AeroState,
};

/// F1f: amostragem baixa (1/10 frames) do diagnóstico de escala, compartilhada
/// entre nested/headless e udev para comparação direta dos logs.
#[cfg(feature = "bench")]
static FSCALE_LOG: AtomicU64 = AtomicU64::new(0);

/// Escala EFETIVA da superfície: a preferida pelo `wp_fractional_scale_v1`, com
/// fallback para a escala do output. É o que o `AsRenderElements` deve usar —
/// não somente `output.current_scale()` (F1f).
fn surface_scale(surface: &WlSurface, output_scale: f64) -> f64 {
    with_states(surface, |states| {
        with_fractional_scale(states, |fs| fs.preferred_scale().unwrap_or(output_scale))
    })
}

/// Tamanho LÓGICO do cliente, no modelo do domicile (`logical_size`):
/// pixels do buffer ÷ `buffer_scale`, substituído por `wp_viewport.dst` quando
/// o cliente fractional usa o viewport. É ESTE tamanho — não `geo.size` nem o
/// buffer cru — que ancora o 9-slice do frame, para borda e conteúdo colarem.
fn client_logical_size(surface: &WlSurface, fallback: Size<i32, Logical>) -> Size<i32, Logical> {
    with_states(surface, |states| {
        let mut attrs = states.cached_state.get::<SurfaceAttributes>();
        let attrs = attrs.current();
        let buffer_scale = attrs.buffer_scale.max(1);
        let from_buffer = match &attrs.buffer {
            Some(BufferAssignment::NewBuffer(buffer)) => buffer_dimensions(buffer).map(|size| {
                Size::<i32, Logical>::new(size.w / buffer_scale, size.h / buffer_scale)
            }),
            _ => None,
        };
        let viewport = states
            .cached_state
            .get::<ViewportCachedState>()
            .current()
            .dst;
        viewport.or(from_buffer)
    })
    .unwrap_or(fallback)
}

#[cfg(feature = "bench")]
pub type OutputProbe = Option<crate::frame_titlebar_probe::TitlebarFrame>;
#[cfg(not(feature = "bench"))]
pub type OutputProbe = ();

/// Monta a cena frente->trás de UM output em coordenadas locais desse output.
/// `output_origin` vem de `space.output_geometry(output)`; é `(0,0)` nos outputs
/// single/headless e a posição real no nested multi-output e no DRM.
pub fn collect_output_elements(
    state: &mut AeroState,
    output: &Output,
    renderer: &mut GlesRenderer,
    // O headless emitia `RENDER_SHARED …` por frame sob `bench`; o nested N>1 não.
    // O parâmetro preserva essa diferença de log sem duplicar a cena.
    _log_render_shared: bool,
) -> (Vec<AeroRenderElements<GlesRenderer>>, OutputProbe) {
    let output_scale = output.current_scale().fractional_scale();
    let output_origin = state
        .space
        .output_geometry(output)
        .map(|geometry| geometry.loc)
        .unwrap_or_default();
    let mut elements: Vec<AeroRenderElements<GlesRenderer>> = Vec::new();
    #[cfg(feature = "bench")]
    let mut titlebar_probe = None;

    {
        let layer_map = smithay::desktop::layer_map_for_output(output);
        let (lower, upper): (Vec<_>, Vec<_>) = layer_map
            .layers()
            .rev()
            .partition(|s| matches!(s.layer(), Layer::Background | Layer::Bottom));

        // LayerMap já usa coordenadas locais do output (layer.rs:248-267).
        for layer in upper {
            if let Some(geo) = layer_map.layer_geometry(layer) {
                let layer_scale = surface_scale(layer.wl_surface(), output_scale);
                elements.extend(AsRenderElements::render_elements::<
                    AeroRenderElements<GlesRenderer>,
                >(
                    layer,
                    renderer,
                    geo.loc.to_physical_precise_round(output_scale),
                    UtilsScale::from(layer_scale),
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
            // Space usa coordenadas globais; render_output recorta a partir de
            // (0,0), então cada output precisa da sua posição local.
            let local_loc = geo.loc - output_origin;
            let render_loc = local_loc - window.geometry().loc;
            // F1d3: a linha `ordem de render … client@…` abaixo é empurrada
            // incondicionalmente; só esta sonda prova que o elemento do cliente
            // foi realmente criado. `from_surface` é a mesma chamada usada por
            // `render_elements_from_surface_tree` (smithay surface.rs:194-222),
            // executada aqui SOB `bench` e sem alterar a cena.
            let client_surface = window.toplevel().map(|t| t.wl_surface());
            let _client_scale = client_surface
                .as_ref()
                .map(|s| surface_scale(s, output_scale))
                .unwrap_or(output_scale);
            // Modelo domicile: a geometria do cliente vem do buffer/viewport,
            // não da window geometry configurada. O frame ancora nela.
            let client_logical = client_surface
                .as_ref()
                .map(|s| client_logical_size(s, geo.size))
                .unwrap_or(geo.size);
            #[cfg(feature = "bench")]
            let probe_loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                render_loc.to_physical_precise_round(output_scale);
            #[cfg(feature = "bench")]
            let client_probe = client_surface.map(|surface| {
                with_states(surface, |states| {
                    WaylandSurfaceRenderElement::from_surface(
                        renderer,
                        surface,
                        states,
                        probe_loc.to_f64(),
                        1.0,
                        Kind::Unspecified,
                    )
                })
            });
            #[cfg(feature = "bench")]
            let before = elements.len();
            elements.extend(AsRenderElements::render_elements::<
                AeroRenderElements<GlesRenderer>,
            >(
                window,
                renderer,
                render_loc.to_physical_precise_round(output_scale),
                UtilsScale::from(output_scale),
                1.0,
            ));
            #[cfg(feature = "bench")]
            {
                let after = elements.len();
                let (state, err) = match &client_probe {
                    Some(Ok(Some(_))) => ("Some", "none".to_owned()),
                    Some(Ok(None)) => ("None", "none".to_owned()),
                    Some(Err(err)) => ("Err", err.to_string().replace([' ', '\n', '\t'], "_")),
                    None => ("no_toplevel", "none".to_owned()),
                };
                let surface = match client_surface {
                    Some(surface) => format!("{:?}", surface.id()),
                    None => "none".to_owned(),
                };
                let t = crate::frame_diagnostics::now_ns();
                eprintln!(
                    "CLIENT_ELEMENT_PROBE t={t} surface={surface} from_surface={state} \
                     err={err} before={before} after={after} delta={}",
                    after - before
                );
            }

            // Frame deste cliente: depois do cliente = abaixo dele.
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
                // F1f: o 9-slice precisa da MESMA escala do cliente (preferred_scale),
                // não a do frame/output — senão `geo.loc + fgeo.offset()` e o
                // WaylandSurfaceRenderElement divergem e a borda "descola" 1 px.
                let (dx, dy) = fgeo.offset();
                let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                    smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                        local_loc.x + dx,
                        local_loc.y + dy,
                    )
                    .to_physical_precise_round(output_scale);

                #[cfg(feature = "bench")]
                if FSCALE_LOG.fetch_add(1, Ordering::Relaxed).is_multiple_of(5) {
                    let foot_bs = client_surface
                        .as_ref()
                        .map(|surface| {
                            with_states(surface, |states| {
                                states
                                    .cached_state
                                    .get::<SurfaceAttributes>()
                                    .current()
                                    .buffer_scale
                            })
                        })
                        .unwrap_or(-1);
                    let frame_pref = surface_scale(frame_surface, output_scale);
                    let frame_bs = with_states(frame_surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .buffer_scale
                    });
                    let viewport_dst = client_surface.as_ref().and_then(|surface| {
                        with_states(surface, |states| {
                            states
                                .cached_state
                                .get::<ViewportCachedState>()
                                .current()
                                .dst
                        })
                    });
                    let foot_dst: Size<i32, Physical> =
                        client_logical.to_physical_precise_round(output_scale);
                    let foot_loc: smithay::utils::Point<i32, Physical> =
                        render_loc.to_physical_precise_round(output_scale);
                    let frame_dst: Size<i32, Physical> =
                        crate::render::elements::frame_target(client_logical, &fgeo)
                            .to_physical_precise_round(output_scale);
                    let t = crate::frame_diagnostics::now_ns();
                    eprintln!(
                        "FSCALE_FRAME t={t} backend=scene output={} window_id={} output_scale={} client_scale={} frame_pref={} foot_buffer_scale={} frame_buffer_scale={} foot_logical={:?} viewport_dst={:?} foot_loc={:?} foot_dst={:?} frame_dst={:?} frame_loc={:?}",
                        output.name(),
                        *_window_id,
                        output_scale,
                        _client_scale,
                        frame_pref,
                        foot_bs,
                        frame_bs,
                        client_logical,
                        viewport_dst,
                        foot_loc,
                        foot_dst,
                        frame_dst,
                        loc
                    );
                }

                let result = crate::render::elements::push_frame_elements(
                    renderer,
                    &mut elements,
                    FrameRenderInput {
                        surface: frame_surface,
                        fgeo,
                        loc,
                        client_origin: local_loc,
                        committed: client_logical,
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
                if let Some(result) = result {
                    placed_frames.push(frame_surface.clone());
                    let kind = match result.kind {
                        FrameRenderKind::Frame => "frame",
                        FrameRenderKind::Placeholder => "placeholder",
                    };
                    render_order.push(format!("{kind}@{:?}", frame_surface.id()));
                    #[cfg(feature = "bench")]
                    {
                        if _log_render_shared {
                            eprintln!("RENDER_SHARED window_id={_window_id} kind={kind}");
                        }
                        if let Some(probe) = result.titlebar_probe {
                            titlebar_probe = Some(probe);
                        }
                    }
                } else {
                    // 02q: decoração do compositor para janelas que mapeiam antes
                    // do WebView associar. Mesma geometria/insets do caminho winit.
                    let mut window_id = 0;
                    for (id, surface) in state.window_surfaces.iter() {
                        if client_surface == Some(surface) {
                            window_id = *id;
                            break;
                        }
                    }
                    let content = if geo.size.w > 0 && geo.size.h > 0 {
                        client_logical
                    } else {
                        state
                            .suggested_window_size
                            .get(&window_id)
                            .copied()
                            .unwrap_or(client_logical)
                    };
                    let fgeo = crate::state::FrameGeometry {
                        insets: state.default_frame_insets,
                        ..Default::default()
                    };
                    let (dx, dy) = fgeo.offset();
                    let loc: smithay::utils::Point<i32, smithay::utils::Physical> =
                        smithay::utils::Point::<i32, smithay::utils::Logical>::new(
                            local_loc.x + dx,
                            local_loc.y + dy,
                        )
                        .to_physical_precise_round(output_scale);
                    crate::render::elements::push_window_placeholder(
                        &mut elements,
                        &fgeo,
                        loc,
                        content,
                        output_scale,
                        local_loc,
                        window_id,
                    );
                    render_order.push(format!("placeholder-sem-frame@{window_id}"));
                }
            }
        }

        // O estado de log é único; não alternar com a cena dos outros outputs.
        let is_first_output = state
            .outputs
            .first()
            .map(|first| first == output)
            .unwrap_or(false);
        if is_first_output && render_order != state.last_render_order {
            tracing::info!(order = ?render_order, "ordem de render (frente->tras)");
            state.last_render_order = render_order;
        }

        // Frames sem cliente mapeado: abaixo de todas as janelas. Um frame cujo
        // cliente JÁ mapeou é desenhado pelo 9-slice no output do cliente; se
        // cair aqui, a layer surface (canvas >= output + insets) aparece inteira
        // na origem de TODOS os outputs — em N>1 isso ocupa metade da tela.
        for layer in &state.frame_layer_surfaces {
            if !layer.alive()
                || placed_frames.contains(layer.wl_surface())
                || state.frame_has_mapped_client(layer.wl_surface())
            {
                continue;
            }
            let f = smithay::utils::Point::<f64, smithay::utils::Physical>::from((0.0, 0.0));
            let fallback_scale = surface_scale(layer.wl_surface(), output_scale);
            if let Ok(Some(el)) = with_states(layer.wl_surface(), |states| {
                WaylandSurfaceRenderElement::from_surface(
                    renderer,
                    layer.wl_surface(),
                    states,
                    f,
                    fallback_scale as f32,
                    Kind::Unspecified,
                )
            }) {
                elements.push(AeroRenderElements::Surface(el));
            }
        }

        for layer in lower {
            if let Some(geo) = layer_map.layer_geometry(layer) {
                let layer_scale = surface_scale(layer.wl_surface(), output_scale);
                elements.extend(AsRenderElements::render_elements::<
                    AeroRenderElements<GlesRenderer>,
                >(
                    layer,
                    renderer,
                    geo.loc.to_physical_precise_round(output_scale),
                    UtilsScale::from(layer_scale),
                    1.0,
                ));
            }
        }
    }

    #[cfg(feature = "bench")]
    let probe = titlebar_probe;
    #[cfg(not(feature = "bench"))]
    let probe = ();
    (elements, probe)
}
