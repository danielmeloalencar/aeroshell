//! zwlr_layer_shell_v1: desktop, panels e overlay.
//!
//! As superfícies entram no `LayerMap` do output: é ele que calcula geometry,
//! exclusive zone, empilhamento e transform. Renderização manual aqui seria um erro.

use smithay::{
    delegate_layer_shell,
    desktop::{layer_map_for_output, LayerSurface},
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{
            Layer, LayerSurfaceCachedState, WlrLayerShellHandler, WlrLayerShellState,
        },
    },
};

use crate::state::AeroState;

impl AeroState {
    /// Painéis ancorados só na borda inferior mantêm 48 px; desktop/overlay
    /// ancorados nas duas bordas verticais recebem a altura cheia do output.
    pub fn configure_layer_surface(&self, layer: &LayerSurface) {
        let Some(output) = self.outputs.first() else {
            return;
        };
        let Some(mode) = output.current_mode() else {
            return;
        };
        let anchored_vertically = with_states(layer.wl_surface(), |states| {
            states
                .cached_state
                .get::<LayerSurfaceCachedState>()
                .current()
                .anchor
                .anchored_vertically()
        });
        let height = if anchored_vertically { mode.size.h } else { 48 };
        let desired = (mode.size.w, height).into();
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
        let frame_window = self
            .associated_frames
            .iter()
            .find(|(s, _, _, _)| s == layer.wl_surface())
            .map(|(_, _, window_id, _)| *window_id);
        if let Some(window_id) = frame_window {
            // Geometria 100% do compositor. O configure do CANVAS é enviado
            // imediatamente (F1-final-1c): sem ele o GTK nunca anexa wl_buffer.
            // O conteúdo REAL do cliente, quando existir, reconfigura via commit.
            // A visibilidade continua protegida por frame_has_mapped_client e
            // pelo alvo 9-slice `conteúdo + insets` (ADR 0007).
            self.frame_layer_surfaces.push(layer);
            if let Some(content) = self.client_content_size(window_id) {
                self.send_frame_configure(window_id, content, "layer");
                tracing::info!(
                    window_id,
                    "frame layer_surface: duplo configure, FORA do LayerMap"
                );
            } else {
                self.configure_frame_layer_canvas(window_id);
                tracing::info!(
                    window_id,
                    "frame layer_surface: configure inicial do canvas (sem conteúdo do cliente)"
                );
            }
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
