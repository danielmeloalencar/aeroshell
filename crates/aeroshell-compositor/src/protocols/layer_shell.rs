//! zwlr_layer_shell_v1: desktop, panels e overlay.

use smithay::{
    delegate_layer_shell,
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::shell::wlr_layer::{Layer, LayerSurface, WlrLayerShellHandler, WlrLayerShellState},
};

use crate::state::AeroState;

impl WlrLayerShellHandler for AeroState {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        _namespace: String,
    ) {
        surface.send_configure();
        self.layer_surfaces.push(surface);
    }
}

delegate_layer_shell!(AeroState);
