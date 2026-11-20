//! Handlers do protocolo privado `aero_frame_v1` (lado compositor).
//! Nenhuma superfície é desenhada nesta fase: apenas protocolo, segurança e testes.

use std::time::Instant;

use crate::{
    aero_frame_protocol::{
        aero_frame_manager_v1::{
            AeroFrameManagerV1, Error as ManagerError, Request as ManagerRequest,
        },
        aero_frame_v1::{AeroFrameV1, Request as FrameRequest},
    },
    state::{AeroState, ClientState},
};
use smithay::wayland::compositor::{add_destruction_hook, get_role};
use smithay::wayland::shell::wlr_layer::LAYER_SURFACE_ROLE;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

/// Resultado puro da validação de um `associate` (testável sem objetos Wayland).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameCheck {
    Ok,
    /// Janela desapareceu numa corrida legítima: criar objeto e enviar `closed`.
    Closed,
    /// Violação de verdade: erro fatal de protocolo.
    AlreadyConstructed,
}

pub fn check_frame(existing: &[u32], window_id: u32, window_count: usize) -> FrameCheck {
    if existing.contains(&window_id) {
        FrameCheck::AlreadyConstructed
    } else if (window_id as usize) >= window_count {
        FrameCheck::Closed
    } else {
        FrameCheck::Ok
    }
}

pub fn serial_valid(expected: Option<u32>, got: u32) -> bool {
    expected == Some(got)
}

/// `associate` apenas MARCA a superfície (ADR 0004). Aceita superfície sem role
/// ou já com role `layer_surface` (o frame é uma layer surface do shell). Rejeita
/// qualquer outro role (xdg_toplevel, xdg_popup, cursor_image, …): a superfície
/// de um cliente comum já tem dono e não pode virar frame.
pub fn role_allowed(role: Option<&'static str>) -> bool {
    match role {
        None => true,
        Some(r) => r == LAYER_SURFACE_ROLE,
    }
}

impl AeroState {
    /// Envia aero_frame.configure com o tamanho de CONTEÚDO e o duplo configure
    /// da layer surface do frame com o total (conteúdo + insets + shadow).
    /// O shell dimensiona a janela GTK a partir do configure (a lib C do
    /// gtk4-layer-shell ignora o tamanho do zwlr configure em eixos não esticados).
    pub fn send_frame_configure(
        &mut self,
        window_id: u32,
        content: (i32, i32),
        _source: &'static str,
    ) {
        let Some((frame_surface, frame, _, _)) = self
            .associated_frames
            .iter()
            .find(|(_, _, id, _)| *id == window_id)
            .cloned()
        else {
            return;
        };
        self.frame_configure_serial += 1;
        let serial = self.frame_configure_serial;
        let frame_state = crate::aero_frame_protocol::aero_frame_v1::State::empty();
        frame.configure(serial, content.0, content.1, frame_state);
        self.pending_configures
            .push((serial, std::time::Instant::now()));
        self.frame_configured_content
            .insert(frame_surface.clone(), content);
        let geo = self
            .frame_geometries
            .get(&frame_surface)
            .copied()
            .unwrap_or_default();
        let total = (content.0 + geo.extra_w(), content.1 + geo.extra_h());
        #[cfg(feature = "bench")]
        self.frame_diag.note_configure(
            window_id,
            serial,
            (content.0.max(0) as u32, content.1.max(0) as u32),
            (total.0.max(0) as u32, total.1.max(0) as u32),
            Instant::now(),
            crate::frame_diagnostics::now_ns(),
        );
        if let Some(layer) = self
            .frame_layer_surfaces
            .iter()
            .find(|l| l.wl_surface() == &frame_surface)
        {
            layer
                .layer_surface()
                .with_pending_state(|state| state.size = Some(total.into()));
            let _ = layer.layer_surface().send_configure();
        }
        tracing::info!(
            window_id,
            serial,
            content_w = content.0,
            content_h = content.1,
            total_w = total.0,
            total_h = total.1,
            "frame configure (conteúdo do cliente)"
        );
        #[cfg(feature = "bench")]
        {
            let t_nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            eprintln!(
                "FRAME_CONFIGURE t={t_nanos} serial={serial} window_id={window_id} \
                 w={} h={} source={_source}",
                content.0, content.1
            );
        }
    }

    /// Tamanho de conteúdo real da janela cliente, se já mapeada e com buffer.
    pub fn client_content_size(&self, window_id: u32) -> Option<(i32, i32)> {
        let window = self.window_for_id(window_id)?;
        let geo = self.space.element_geometry(window)?;
        (geo.size.w > 0 && geo.size.h > 0).then_some((geo.size.w, geo.size.h))
    }

    /// Liga um frame associado ANTES de o cliente mapear (cliente inicial adiado)
    /// à superfície cliente quando ela finalmente mapeia. Sem isto o entry fica
    /// com `client_surface=None` e o frame nunca segue o conteúdo nem recebe
    /// `closed`. O primeiro configure sai no commit do cliente (compositor.rs).
    pub fn bind_pending_frame_client(
        &mut self,
        window_id: u32,
        client: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let Some(entry) = self
            .associated_frames
            .iter_mut()
            .find(|(_, _, id, cs)| *id == window_id && cs.is_none())
        else {
            return;
        };
        entry.3 = Some(client.clone());
        add_destruction_hook::<AeroState, _>(&client, |state, dead| {
            // O id pode ser reatribuído a uma janela nova; não deixe o mapa
            // estável apontando para a superfície que acabou de morrer.
            state.window_surfaces.retain(|_, cs| *cs != *dead);
            if let Some(pos) = state
                .associated_frames
                .iter()
                .position(|(_, _, _, cs)| cs.as_ref() == Some(dead))
            {
                let (frame_surface, frame, window_id, _) = state.associated_frames.remove(pos);
                frame.closed();
                state.frame_geometries.remove(&frame_surface);
                state.frame_configured_content.remove(&frame_surface);
                state
                    .frame_layer_surfaces
                    .retain(|l| l.wl_surface() != &frame_surface);
                tracing::info!(
                    window_id,
                    "janela cliente destruída: closed enviado ao frame"
                );
            }
        });
        tracing::info!(window_id, "frame pendente ligado ao cliente mapeado");
    }
}

impl GlobalDispatch<AeroFrameManagerV1, ()> for AeroState {
    fn bind(
        state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<AeroFrameManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        tracing::info!("cliente privilegiado vinculou aero_frame_manager_v1");
        state.shell_manager = Some(data_init.init(resource, ()));
    }

    fn can_view(client: Client, _global_data: &()) -> bool {
        client
            .get_data::<ClientState>()
            .map(|state| state.is_shell)
            .unwrap_or(false)
    }
}

impl Dispatch<AeroFrameManagerV1, ()> for AeroState {
    fn request(
        state: &mut Self,
        _client: &Client,
        manager: &AeroFrameManagerV1,
        request: ManagerRequest,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            ManagerRequest::Associate {
                id,
                surface,
                window_id,
            } => {
                // ADR 0004: NÃO atribuir role. Apenas marcar a superfície.
                if state
                    .associated_frames
                    .iter()
                    .any(|(s, _, _, _)| s == &surface)
                {
                    manager.post_error(
                        ManagerError::AlreadyConstructed,
                        "esta wl_surface já está associada",
                    );
                    return;
                }
                if !role_allowed(get_role(&surface)) {
                    manager.post_error(
                        ManagerError::Role,
                        "associate aceita superfície sem role ou layer_surface",
                    );
                    return;
                }
                let existing: Vec<u32> = state.frames.iter().map(|(_, id)| *id).collect();
                // Cold-start temporário: o primeiro associate do frame_host libera o
                // cliente inicial adiado. Ele ainda não mapeou, então este associate
                // NÃO é "janela fechada": tratamos como Ok e o configure sai quando o
                // cliente mapear (reconfigure no commit).
                let pending_cmd = state.pending_initial_command.take();
                if let Some(command) = &pending_cmd {
                    match crate::supervisor::spawn_public_client(&state.socket_name, command) {
                        Ok(pid) => tracing::info!(
                            ?command,
                            pid,
                            "cliente inicial iniciado (após associate)"
                        ),
                        Err(err) => {
                            tracing::warn!(?command, %err, "falha ao iniciar cliente adiado")
                        }
                    }
                }
                let window_count = state.space.elements().count() + state.test_window_count;
                let window_count = if pending_cmd.is_some() {
                    window_count.max(window_id as usize + 1)
                } else {
                    window_count
                };
                match check_frame(&existing, window_id, window_count) {
                    FrameCheck::AlreadyConstructed => {
                        manager.post_error(
                            ManagerError::AlreadyConstructed,
                            format!("janela {window_id} já tem frame"),
                        );
                        return;
                    }
                    FrameCheck::Closed => {
                        let frame: AeroFrameV1 = data_init.init(id, window_id);
                        frame.closed();
                        state.frames.push((frame.clone(), window_id));
                        let client_surface = state
                            .window_for_id(window_id)
                            .and_then(|w| w.toplevel().map(|t| t.wl_surface().clone()));
                        state.associated_frames.push((
                            surface.clone(),
                            frame,
                            window_id,
                            client_surface,
                        ));
                        tracing::info!(window_id, "associate após fechamento: closed enviado");
                        return;
                    }
                    FrameCheck::Ok => {}
                }

                let frame: AeroFrameV1 = data_init.init(id, window_id);
                let size = state
                    .outputs
                    .first()
                    .and_then(|o| o.current_mode())
                    .map(|m| m.size)
                    .unwrap_or((1280, 800).into());
                let frame_state = crate::aero_frame_protocol::aero_frame_v1::State::empty();
                manager.frame_requested(window_id, size.w, size.h, frame_state);
                state.frames.push((frame.clone(), window_id));
                let client_surface = state
                    .window_for_id(window_id)
                    .and_then(|w| w.toplevel().map(|t| t.wl_surface().clone()));
                state.associated_frames.push((
                    surface.clone(),
                    frame,
                    window_id,
                    client_surface.clone(),
                ));
                // Quando a janela cliente morre, o frame recebe `closed` (ADR 0004).
                if let Some(client) = client_surface {
                    add_destruction_hook::<crate::state::AeroState, _>(&client, |state, dead| {
                        if let Some(pos) = state
                            .associated_frames
                            .iter()
                            .position(|(_, _, _, cs)| cs.as_ref() == Some(dead))
                        {
                            let (frame_surface, frame, window_id, _) =
                                state.associated_frames.remove(pos);
                            frame.closed();
                            state.frame_geometries.remove(&frame_surface);
                            state.frame_configured_content.remove(&frame_surface);
                            state
                                .frame_layer_surfaces
                                .retain(|l| l.wl_surface() != &frame_surface);
                            tracing::info!(
                                window_id,
                                "janela cliente destruída: closed enviado ao frame"
                            );
                        }
                    });
                }
                // Em produção: configure com o conteúdo REAL do cliente. No caso
                // normal usa fallback no output; só no cliente inicial adiado (sem
                // cliente mapeado) adia o configure para o commit do cliente.
                #[cfg(not(feature = "bench"))]
                {
                    let real = state.client_content_size(window_id);
                    if real.is_some() || pending_cmd.is_none() {
                        state.send_frame_configure(
                            window_id,
                            real.unwrap_or((size.w, size.h)),
                            "probe",
                        );
                    }
                }
                // Só sob `bench`: quantos passos de resize sintéticos enviar de uma
                // vez (N=5 / N=20 nas medições).
                #[cfg(feature = "bench")]
                {
                    let frame = state.frames.last().expect("frame recém-inserido").0.clone();
                    let steps: u32 = std::env::var("AERO_FRAME_STEPS")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1)
                        .max(1);
                    for step in 0..steps {
                        let serial = step + 1;
                        let w = size.w + (step as i32) * 24;
                        let h = size.h + (step as i32) * 16;
                        frame.configure(serial, w, h, frame_state);
                        let configured_at = Instant::now();
                        state.pending_configures.push((serial, configured_at));
                        state.frame_probes.push(crate::state::FrameProbe {
                            surface: surface.clone(),
                            window_id,
                            serial,
                            configured_at,
                            size: (w as u32, h as u32),
                            acked: false,
                            committed: false,
                            timeout_logged: false,
                        });
                        let t_nanos = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0);
                        eprintln!(
                            "FRAME_CONFIGURE t={t_nanos} serial={serial} window_id={window_id} \
                             w={w} h={h} source=probe"
                        );
                    }
                }
                tracing::info!(
                    window_id,
                    frame_surface = ?surface.id(),
                    "frame associado; frame_requested+configure enviados"
                );
            }
            ManagerRequest::Destroy => {}
        }
    }
}

impl Dispatch<AeroFrameV1, u32> for AeroState {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &AeroFrameV1,
        request: FrameRequest,
        window_id: &u32,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let now = Instant::now();
        match request {
            FrameRequest::SetInsets {
                left,
                top,
                right,
                bottom,
            } => {
                if let Some((surface, _, _, _)) = state
                    .associated_frames
                    .iter()
                    .find(|(_, _, id, _)| id == window_id)
                {
                    let geo = state.frame_geometries.entry(surface.clone()).or_default();
                    geo.insets = (left, top, right, bottom);
                }
                tracing::info!(window_id = *window_id, left, top, right, bottom, "insets");
            }
            FrameRequest::SetShadowExtents {
                left,
                top,
                right,
                bottom,
            } => {
                if let Some((surface, _, _, _)) = state
                    .associated_frames
                    .iter()
                    .find(|(_, _, id, _)| id == window_id)
                {
                    let geo = state.frame_geometries.entry(surface.clone()).or_default();
                    geo.shadow = (left, top, right, bottom);
                }
                tracing::info!(window_id = *window_id, left, top, right, bottom, "sombra");
            }
            FrameRequest::Move { seat, serial } => {
                let _ = seat;
                if crate::grab::start_geometry_grab(state, *window_id, serial, None) {
                    state.last_geometry_action = Some(("move".to_owned(), now));
                    tracing::info!(
                        window_id = *window_id,
                        serial,
                        "move do frame: grab iniciado"
                    );
                } else {
                    tracing::debug!(serial, "move ignorado: serial inválido ou janela ausente");
                }
            }
            FrameRequest::Resize {
                seat,
                serial,
                edges,
            } => {
                let _ = seat;
                let edges = edges
                    .into_result()
                    .unwrap_or(crate::aero_frame_protocol::aero_frame_v1::ResizeEdge::None);
                if crate::grab::start_geometry_grab(state, *window_id, serial, Some(edges)) {
                    state.last_geometry_action = Some(("resize".to_owned(), now));
                    tracing::info!(
                        window_id = *window_id,
                        serial,
                        ?edges,
                        "resize do frame: grab iniciado"
                    );
                } else {
                    tracing::debug!(serial, "resize ignorado: serial inválido ou janela ausente");
                }

                // Só sob `bench`: cada resize válido gera um novo configure com
                // tamanho diferente, para exercitar configure → commit por passo.
                #[cfg(feature = "bench")]
                if let Some(probe) = state.frame_probes.iter_mut().find(|p| p.serial == serial) {
                    let next_serial = state.pending_configures.len() as u32 + 100;
                    let w = probe.size.0.saturating_add(40);
                    let h = probe.size.1.saturating_add(25);
                    let configured_at = Instant::now();
                    let frame_state = crate::aero_frame_protocol::aero_frame_v1::State::empty();
                    resource.configure(next_serial, w as i32, h as i32, frame_state);
                    state.pending_configures.push((next_serial, configured_at));
                    probe.serial = next_serial;
                    probe.size = (w, h);
                    probe.configured_at = configured_at;
                    probe.acked = false;
                    probe.timeout_logged = false;
                    let t_nanos = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0);
                    eprintln!(
                        "FRAME_CONFIGURE t={t_nanos} serial={next_serial} window_id={window_id} \
                         w={w} h={h} source=probe"
                    );
                }
            }
            FrameRequest::AckConfigure { serial } => {
                if let Some(pos) = state
                    .pending_configures
                    .iter()
                    .position(|(s, _)| *s == serial)
                {
                    let (_, started) = state.pending_configures.remove(pos);
                    let ack_ms = started.elapsed().as_secs_f64() * 1000.0;
                    #[cfg(feature = "bench")]
                    {
                        // 02j: o ack conta mesmo para configures REAIS, que não
                        // têm entrada em `frame_probes`.
                        state.frame_diag.note_ack(serial);
                        if let Some(probe) =
                            state.frame_probes.iter_mut().find(|p| p.serial == serial)
                        {
                            probe.acked = true;
                            eprintln!(
                                "FRAME_ACK serial={serial} window_id={} ack_ms={ack_ms:.3}",
                                probe.window_id
                            );
                        }
                    }
                    tracing::info!(serial, ack_ms, "ack_configure");
                }
            }
            FrameRequest::Destroy => {
                if let Some(pos) = state
                    .associated_frames
                    .iter()
                    .position(|(_, frame, _, _)| frame == resource)
                {
                    let (surface, _, _, _) = state.associated_frames.remove(pos);
                    state.frame_geometries.remove(&surface);
                    state.frame_configured_content.remove(&surface);
                    state
                        .frame_layer_surfaces
                        .retain(|l| l.wl_surface() != &surface);
                }
                state.frames.retain(|(frame, _)| frame != resource);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_window_is_not_fatal() {
        assert_eq!(check_frame(&[], 99, 1), FrameCheck::Closed);
    }

    #[test]
    fn existing_window_is_fatal() {
        assert_eq!(check_frame(&[3], 3, 5), FrameCheck::AlreadyConstructed);
    }

    #[test]
    fn known_window_is_ok() {
        assert_eq!(check_frame(&[], 2, 5), FrameCheck::Ok);
    }

    #[test]
    fn serial_from_other_surface_is_ignored() {
        assert!(!serial_valid(Some(10), 11));
        assert!(serial_valid(Some(10), 10));
    }

    #[test]
    fn role_none_or_layer_is_allowed() {
        assert!(role_allowed(None));
        assert!(role_allowed(Some(LAYER_SURFACE_ROLE)));
    }

    #[test]
    fn toplevel_popup_cursor_roles_are_rejected() {
        use smithay::wayland::seat::CURSOR_IMAGE_ROLE;
        use smithay::wayland::shell::xdg::{XDG_POPUP_ROLE, XDG_TOPLEVEL_ROLE};
        assert!(!role_allowed(Some(XDG_TOPLEVEL_ROLE)));
        assert!(!role_allowed(Some(XDG_POPUP_ROLE)));
        assert!(!role_allowed(Some(CURSOR_IMAGE_ROLE)));
    }
}
