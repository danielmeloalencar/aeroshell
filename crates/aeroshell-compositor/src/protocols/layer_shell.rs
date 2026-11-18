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
        tracing::info!(%namespace, "nova layer surface (panel/desktop/overlay)");
        let layer = LayerSurface::new(surface, namespace);
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
