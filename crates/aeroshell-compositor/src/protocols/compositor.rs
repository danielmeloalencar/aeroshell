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
            let expected = self.frame_configured_content.get(surface).map(|(w, h)| {
                let geo = self
                    .frame_geometries
                    .get(surface)
                    .copied()
                    .unwrap_or_default();
                (w + geo.extra_w(), h + geo.extra_h())
            });
            let ok = committed.is_some() && committed == expected;
            eprintln!(
                "FRAME_BUFFER_COMMIT buffer={:?} expected={:?} ok={ok}",
                committed, expected
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
        #[cfg(feature = "bench")]
        {
            // 02c: medir o instante em que o conteúdo e a decoração realmente
            // entram na tela (commit do buffer), não o instante do configure.
            // Lido ANTES de `on_commit_buffer_handler`, que faz `attrs.buffer.take()`.
            let t_nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let committed = with_states(surface, |states| {
                match &states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .buffer
                {
                    Some(BufferAssignment::NewBuffer(buffer)) => {
                        smithay::wayland::shm::with_buffer_contents(buffer, |_, _, data| {
                            (data.width, data.height)
                        })
                        .ok()
                    }
                    _ => None,
                }
            });
            if let Some((w, h)) = committed {
                if self
                    .space
                    .elements()
                    .any(|window| window.toplevel().map(|t| t.wl_surface()) == Some(surface))
                {
                    eprintln!("RESIZE_SYNC_CLIENT t={t_nanos} size={w}x{h}");
                }
                if let Some((frame_surface, window_id)) = self
                    .associated_frames
                    .iter()
                    .find(|(frame_surface, _, _, _)| frame_surface == surface)
                    .map(|(frame_surface, _, id, _)| (frame_surface.clone(), *id))
                {
                    let geo = self
                        .frame_geometries
                        .get(&frame_surface)
                        .copied()
                        .unwrap_or_default();
                    eprintln!(
                        "RESIZE_SYNC_FRAME t={t_nanos} window_id={window_id} buffer={w}x{h} content={}x{}",
                        w - geo.extra_w(),
                        h - geo.extra_h()
                    );
                }
            }
        }
        on_commit_buffer_handler::<Self>(surface);
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            let committed_window = self
                .space
                .elements()
                .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(&root))
                .cloned();
            if let Some(window) = committed_window {
                window.on_commit();
                // O frame SEGUE o conteúdo: se a janela cliente mudou de tamanho,
                // reenvia aero_frame.configure + zwlr configure com o novo total.
                let assoc = self
                    .associated_frames
                    .iter()
                    .find(|(_, _, _, cs)| cs.as_ref() == Some(&root))
                    .map(|(s, _, id, _)| (s.clone(), *id));
                if let Some((frame_surface, window_id)) = assoc {
                    if let Some(content) = self.client_content_size(window_id) {
                        if self.frame_configured_content.get(&frame_surface) != Some(&content) {
                            tracing::info!(
                                bbox = ?window.bbox(),
                                geometry = ?window.geometry(),
                                elem_geo = ?self.space.element_geometry(&window),
                                "geometria real do cliente (bbox vs window geometry)"
                            );
                            self.send_frame_configure(window_id, content);
                        }
                    }
                }
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
