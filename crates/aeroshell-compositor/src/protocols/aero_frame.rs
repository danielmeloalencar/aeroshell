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
use smithay::wayland::compositor::give_role;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource};

const AERO_FRAME_ROLE: &str = "aero_frame";

/// Resultado puro da validação de um `get_frame` (testável sem objects Wayland).
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

impl GlobalDispatch<AeroFrameManagerV1, ()> for AeroState {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<AeroFrameManagerV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        tracing::info!("cliente privilegiado vinculou aero_frame_manager_v1");
        let _ = data_init.init(resource, ());
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
            ManagerRequest::GetFrame {
                id,
                surface,
                window_id,
            } => {
                if give_role(&surface, AERO_FRAME_ROLE).is_err() {
                    manager.post_error(ManagerError::Role, "a wl_surface já possui outro role");
                    return;
                }
                let existing: Vec<u32> = state.frames.iter().map(|(_, id)| *id).collect();
                let window_count = state.space.elements().count() + state.test_window_count;
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
                        state.frames.push((frame, window_id));
                        tracing::info!(window_id, "get_frame após fechamento: closed enviado");
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
                let serial = state.pending_configures.len() as u32 + 1;
                frame.configure(serial, size.w, size.h, frame_state);
                state.pending_configures.push((serial, Instant::now()));
                state.frames.push((frame, window_id));
                tracing::info!(
                    window_id,
                    serial,
                    "frame criado; frame_requested+configure enviados"
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
                tracing::info!(
                    window_id = *window_id,
                    left,
                    top,
                    right,
                    bottom,
                    "insets pendentes"
                )
            }
            FrameRequest::SetShadowExtents {
                left,
                top,
                right,
                bottom,
            } => {
                tracing::info!(
                    window_id = *window_id,
                    left,
                    top,
                    right,
                    bottom,
                    "sombra pendente"
                )
            }
            FrameRequest::Move { seat, serial } => {
                if !serial_valid(state.last_button_serial, serial) {
                    tracing::debug!(serial, "move ignorado: serial inválido");
                    return;
                }
                let _ = seat;
                state.last_geometry_action = Some(("move".to_owned(), now));
                tracing::info!(window_id = *window_id, serial, "move do frame");
            }
            FrameRequest::Resize {
                seat,
                serial,
                edges,
            } => {
                if !serial_valid(state.last_button_serial, serial) {
                    tracing::debug!(serial, "resize ignorado: serial inválido");
                    return;
                }
                let _ = seat;
                state.last_geometry_action = Some(("resize".to_owned(), now));
                tracing::info!(window_id = *window_id, serial, ?edges, "resize do frame");
            }
            FrameRequest::AckConfigure { serial } => {
                if let Some(pos) = state
                    .pending_configures
                    .iter()
                    .position(|(s, _)| *s == serial)
                {
                    let (_, started) = state.pending_configures.remove(pos);
                    tracing::info!(
                        serial,
                        ack_ms = started.elapsed().as_secs_f64() * 1000.0,
                        "ack_configure"
                    );
                }
            }
            FrameRequest::Destroy => {
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
}
