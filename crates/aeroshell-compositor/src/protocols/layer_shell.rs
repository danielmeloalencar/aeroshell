//! zwlr_layer_shell_v1: desktop, panels e overlay.
//!
//! As superfícies entram no `LayerMap` do output: é ele que calcula geometry,
//! exclusive zone, empilhamento e transform. Renderização manual aqui seria um erro.

use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface},
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::shell::wlr_layer::{Layer, WlrLayerShellHandler, WlrLayerShellState},
};

use crate::state::AeroState;

impl AeroState {
    /// Sugere ao cliente a largura do output e a altura do painel do spike.
    pub fn configure_layer_surface(&self, layer: &LayerSurface) {
        let Some(output) = self.outputs.first() else {
            return;
        };
        let Some(mode) = output.current_mode() else {
            return;
        };
        let desired = (mode.size.w, 48).into();
        let protocol = layer.layer_surface();
        if protocol.current_state().size != Some(desired) {
            protocol.with_pending_state(|state| state.size = Some(desired));
            let _ = protocol.send_pending_configure();
        }
    }

    /// Reorganiza todas as camadas depois de um commit.
    pub fn arrange_layers(&self) {
        for output in &self.outputs {
            let mut map = layer_map_for_output(output);
            let _ = map.arrange();
        }
    }
}

impl WlrLayerShellHandler for AeroState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let layer = LayerSurface::new(surface, namespace.clone());

        // ADR 0004: superfície associada a um frame é EXCEÇÃO ao LayerMap.
        // Não entra no mapa nem na lista de janelas; o compositor define a
        // geometria e envia o duplo configure na mesma transação.
        let frame_assoc = self
            .associated_frames
            .iter()
            .find(|(s, _, _)| s == layer.wl_surface())
            .map(|(_, frame, window_id)| (frame.clone(), *window_id));
        if let Some((frame, window_id)) = frame_assoc {
            // Geometria 100% do compositor: usar a janela real (zenity) quando
            // existir, em vez do tamanho do output.
            let content = self
                .space
                .elements()
                .next()
                .and_then(|w| self.space.element_geometry(w))
                .map(|g| (g.size.w, g.size.h))
                .or_else(|| {
                    self.outputs
                        .first()
                        .and_then(|o| o.current_mode())
                        .map(|m| (m.size.w, m.size.h))
                })
                .unwrap_or((1280, 800));
            // Insets/shadow ainda não implementados; margem diagnóstica
            // (debug/bench) deixa o contorno visível nos testes visuais.
            let margin = self.frame_test_margin;
            let total = (content.0 + 2 * margin, content.1 + 2 * margin);
            layer.layer_surface().with_pending_state(|state| {
                state.size = Some(total.into());
            });
            let _ = layer.layer_surface().send_pending_configure();
            let serial = self.pending_configures.len() as u32 + 1;
            let frame_state = crate::aero_frame_protocol::aero_frame_v1::State::empty();
            frame.configure(serial, content.0, content.1, frame_state);
            self.pending_configures
                .push((serial, std::time::Instant::now()));
            self.frame_layer_surfaces.push(layer);
            tracing::info!(
                window_id,
                total_w = total.0,
                total_h = total.1,
                "frame layer_surface: duplo configure, FORA do LayerMap"
            );
            return;
        }

        tracing::info!(%namespace, "nova layer surface (panel/desktop/overlay)");
        self.configure_layer_surface(&layer);
        layer.layer_surface().send_configure();

        for output in &self.outputs {
            let mut map = layer_map_for_output(output);
            if let Err(err) = map.map_layer(&layer) {
                tracing::warn!(?err, "falha ao mapear layer surface");
            }
            let _ = map.arrange();
            tracing::info!(
                output = output.name(),
                non_exclusive_zone = ?map.non_exclusive_zone(),
                "LayerMap após mapear o painel"
            );
        }
        self.layer_surfaces.push(layer);
    }
}

delegate_layer_shell!(AeroState);
