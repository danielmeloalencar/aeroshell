//! Cena de UM output, frente->trás, compartilhada por headless, winit (N>1) e udev.
//!
//! F1d3: o mesmo `push_frame_elements`, a mesma ordem de layers/clientes/frames e o
//! mesmo formato de log em todos os backends. Esta árvore não conhece winit/udev;
//! só Smithay e `AeroState`. O caminho N=1 do winit mantém sua cópia especializada
//! (age=0, cor de clear configurável) para não regredir a validação visual F0/F1c.

use smithay::{
    backend::renderer::{
        element::{surface::WaylandSurfaceRenderElement, AsRenderElements, Kind},
        gles::GlesRenderer,
    },
    output::Output,
    reexports::wayland_server::Resource,
    utils::{IsAlive, Scale as UtilsScale},
    wayland::{compositor::with_states, shell::wlr_layer::Layer},
};

use crate::{
    render::elements::{AeroRenderElements, FrameRenderInput, FrameRenderKind},
    state::AeroState,
};

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
            // Space usa coordenadas globais; render_output recorta a partir de
            // (0,0), então cada output precisa da sua posição local.
            let local_loc = geo.loc - output_origin;
            let render_loc = local_loc - window.geometry().loc;
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
                        local_loc.x + dx,
                        local_loc.y + dy,
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
                        client_origin: local_loc,
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

    #[cfg(feature = "bench")]
    let probe = titlebar_probe;
    #[cfg(not(feature = "bench"))]
    let probe = ();
    (elements, probe)
}
