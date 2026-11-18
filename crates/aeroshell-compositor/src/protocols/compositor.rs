//! wl_compositor, wl_shm e buffer.

use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::{
        protocol::{wl_buffer, wl_surface::WlSurface},
        Client,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler,
            CompositorState,
        },
        shm::{ShmHandler, ShmState},
    },
};

#[cfg(feature = "bench")]
use smithay::wayland::compositor::{with_states, BufferAssignment, SurfaceAttributes};

use crate::{protocols::xdg_shell, state::AeroState};

impl CompositorHandler for AeroState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client
            .get_data::<crate::state::ClientState>()
            .expect("cliente Wayland sem ClientState")
            .compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        // Medir o buffer REAL commitado pelo frame vs. o tamanho configurado.
        #[cfg(debug_assertions)]
        if self
            .associated_frames
            .iter()
            .any(|(s, _, _, _)| s == surface)
        {
            let committed = smithay::wayland::compositor::with_states(surface, |states| {
                match &states
                    .cached_state
                    .get::<smithay::wayland::compositor::SurfaceAttributes>()
                    .current()
                    .buffer
                {
                    Some(smithay::wayland::compositor::BufferAssignment::NewBuffer(buffer)) => {
                        smithay::wayland::shm::with_buffer_contents(buffer, |_, _, data| {
                            (data.width, data.height)
                        })
                        .ok()
                    }
                    _ => None,
                }
            });
            let configured = self
                .frame_geometries
                .get(surface)
                .map(|g| (g.insets.0 + g.insets.2, g.insets.1 + g.insets.3));
            eprintln!(
                "FRAME_BUFFER_COMMIT buffer={:?} insets_extra={:?}",
                committed, configured
            );
        }
        #[cfg(feature = "bench")]
        {
            // Métricas do protótipo do frame (i) e (ii): ler o buffer ANTES do
            // `on_commit_buffer_handler`, que faz `attrs.buffer.take()`.
            let probe_index = self
                .frame_probes
                .iter()
                .position(|p| &p.surface == surface && !p.committed);
            if let Some(index) = probe_index {
                let buffer_size = with_states(surface, |states| {
                    match &states
                        .cached_state
                        .get::<SurfaceAttributes>()
                        .current()
                        .buffer
                    {
                        Some(BufferAssignment::NewBuffer(buffer)) => {
                            smithay::wayland::shm::with_buffer_contents(buffer, |_, _, data| {
                                (data.width as u32, data.height as u32)
                            })
                            .ok()
                        }
                        _ => None,
                    }
                });
                let probe = &mut self.frame_probes[index];
                probe.committed = true;
                let mismatch = buffer_size
                    .map(|size| crate::frame_metrics::is_descompasso(size, probe.size))
                    .unwrap_or(false);
                self.frame_commit_samples.push((
                    probe.serial,
                    probe.configured_at.elapsed().as_secs_f64() * 1000.0,
                ));
                eprintln!(
                    "FRAME_COMMIT serial={} window_id={} configure_to_commit_ms={:.3} buffer={:?} configured={:?} descompasso={mismatch}",
                    probe.serial,
                    probe.window_id,
                    probe.configured_at.elapsed().as_secs_f64() * 1000.0,
                    buffer_size,
                    probe.size
                );
            }
        }
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(&root))
                .cloned()
            {
                window.on_commit();
            }
        }
        xdg_shell::handle_commit(&mut self.popups, &self.space, surface);

        if let Some(layer) = self
            .layer_surfaces
            .iter()
            .find(|layer| layer.wl_surface() == surface)
            .cloned()
        {
            let input_region = smithay::wayland::compositor::with_states(surface, |states| {
                states
                    .cached_state
                    .get::<smithay::wayland::compositor::SurfaceAttributes>()
                    .current()
                    .input_region
                    .clone()
            });
            tracing::debug!(?input_region, "input_region recebida do cliente layer");
            let cached = layer.cached_state();
            static LOGGED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                tracing::info!(
                    layer = ?cached.layer,
                    anchor = ?cached.anchor,
                    exclusive_zone = ?cached.exclusive_zone,
                    margins = ?cached.margin,
                    "primeiro commit da layer surface (zwlr_layer_surface_v1)"
                );
            }
            let accepted = cached.size;
            static LAST_SIZE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let signature = (accepted.w as u32) << 16 | (accepted.h as u32).min(0xffff);
            if LAST_SIZE.swap(signature, std::sync::atomic::Ordering::Relaxed) != signature {
                tracing::info!(size = ?accepted, "tamanho aceito pela layer surface");
            }
            self.configure_layer_surface(&layer);
            let _ = layer.layer_surface().send_pending_configure();
            self.arrange_layers();
            if let Some(output) = self.outputs.first() {
                let map = smithay::desktop::layer_map_for_output(output);
                let zone = map.non_exclusive_zone();
                let geometry = map.layer_geometry(&layer);
                let signature = zone.size.w as u32 * 65_536
                    + zone.size.h as u32
                    + geometry.map(|g| g.size.w as u32 * 131_072).unwrap_or(0);
                static LAST_GEO: std::sync::atomic::AtomicU32 =
                    std::sync::atomic::AtomicU32::new(0);
                if LAST_GEO.swap(signature, std::sync::atomic::Ordering::Relaxed) != signature {
                    tracing::info!(
                        zone = ?zone,
                        geometry = ?geometry,
                        "geometria real da layer surface (LayerMap)"
                    );
                }
            }
        }
    }
}

impl BufferHandler for AeroState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for AeroState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(AeroState);
delegate_shm!(AeroState);
