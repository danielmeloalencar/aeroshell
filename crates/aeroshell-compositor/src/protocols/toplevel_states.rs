//! F1-xdg-toplevel-states: fullscreen, maximize, parent e min/max.
//! Reusa a mecânica da 02r (`work_area` + `frame_geometry_for_window`), sem
//! novo protocolo e sem mudar a decisão de decoração (ServerSide + CSD).

use smithay::{
    desktop::Window,
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Point, Size},
    wayland::{
        compositor::{add_destruction_hook, with_states},
        shell::xdg::{SurfaceCachedState, XdgToplevelSurfaceData},
    },
};

#[cfg(feature = "bench")]
use smithay::reexports::wayland_server::Resource;

use crate::{
    aero_frame_protocol::aero_frame_v1::State as FrameState, state::AeroState,
    window_ipc::emit_event,
};
use aeroshell_ipc::types::AeroEvent;

///Limites pedidos pelo cliente. `0` num eixo = sem limite (XDG spec).
fn limits(surface: &WlSurface, pending: bool) -> (Size<i32, Logical>, Size<i32, Logical>) {
    with_states(surface, |states| {
        let mut cached = states.cached_state.get::<SurfaceCachedState>();
        let value = if pending {
            *cached.pending()
        } else {
            *cached.current()
        };
        (value.min_size, value.max_size)
    })
}

pub fn clamp_size(
    surface: &WlSurface,
    mut size: Size<i32, Logical>,
    pending: bool,
) -> Size<i32, Logical> {
    let (min, max) = limits(surface, pending);
    if max.w > 0 {
        size.w = size.w.min(max.w);
    }
    if max.h > 0 {
        size.h = size.h.min(max.h);
    }
    if min.w > 0 {
        size.w = size.w.max(min.w);
    }
    if min.h > 0 {
        size.h = size.h.max(min.h);
    }
    size
}

fn log_state(state: &AeroState, window_id: u32, fullscreen: bool, maximized: bool) {
    #[cfg(feature = "bench")]
    {
        let size = state
            .window_for_id(window_id)
            .and_then(|w| state.space.element_geometry(w))
            .map(|g| g.size)
            .unwrap_or_default();
        let loc = state
            .window_for_id(window_id)
            .and_then(|w| state.space.element_geometry(w))
            .map(|g| g.loc)
            .unwrap_or_default();
        eprintln!(
            "XDG_STATE t={} window_id={window_id} fullscreen={fullscreen} maximized={maximized} \
             w={} h={} loc=({},{})",
            crate::frame_diagnostics::now_ns(),
            size.w,
            size.h,
            loc.x,
            loc.y
        );
    }
    #[cfg(not(feature = "bench"))]
    let _ = (state, window_id, fullscreen, maximized);
}

/// Reenvia o configure do frame com os bits de estado atuais (o XML já tem
/// `maximized`/`fullscreen`; nenhuma emenda de protocolo é necessária).
pub fn refresh_frame_state(state: &mut AeroState, window_id: u32) {
    let frame_state = frame_state_for(state, window_id);
    #[cfg(feature = "bench")]
    eprintln!(
        "FRAME_STATE t={} window_id={window_id} fullscreen={} maximized={}",
        crate::frame_diagnostics::now_ns(),
        frame_state.contains(FrameState::Fullscreen),
        frame_state.contains(FrameState::Maximized)
    );
    let content = state.client_content_size(window_id).unwrap_or((0, 0));
    state.send_frame_configure(window_id, content, "state");
    #[cfg(not(feature = "bench"))]
    let _ = frame_state;
}

pub fn frame_state_for(state: &AeroState, window_id: u32) -> FrameState {
    let mut bits = FrameState::empty();
    let Some(window) = state.window_for_id(window_id) else {
        return bits;
    };
    if let Some(toplevel) = window.toplevel() {
        // Estado PENDENTE: o cliente ainda não fez commit quando reenviamos o
        // configure do frame após set_fullscreen/set_maximized.
        let flags = toplevel.with_pending_state(|state| state.states.clone());
        if flags.contains(xdg_toplevel::State::Maximized) {
            bits |= FrameState::Maximized;
        }
        if flags.contains(xdg_toplevel::State::Fullscreen) {
            bits |= FrameState::Fullscreen;
        }
    }
    bits
}

pub fn apply_maximize(state: &mut AeroState, window_id: u32, target: bool) -> bool {
    let Some(window) = state.window_for_id(window_id).cloned() else {
        return false;
    };
    let Some(toplevel) = window.toplevel() else {
        return false;
    };
    let surface = toplevel.wl_surface().clone();
    if target {
        let Some(area) = state.work_area(window_id) else {
            return false;
        };
        if let Some(geo) = state.space.element_geometry(&window) {
            state
                .maximized_restore
                .entry(window_id)
                .or_insert((geo.loc, geo.size));
        }
        let fgeo = state.frame_geometry_for_window(window_id);
        let (dx, dy) = fgeo.offset();
        let client_loc = Point::<i32, Logical>::new(area.loc.x - dx, area.loc.y - dy);
        let client_size = clamp_size(
            &surface,
            Size::new(
                (area.size.w - fgeo.extra_w()).max(1),
                (area.size.h - fgeo.extra_h()).max(1),
            ),
            false,
        );
        state.space.map_element(window.clone(), client_loc, false);
        toplevel.with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Maximized);
            s.size = Some(client_size);
        });
    } else {
        if let Some((loc, size)) = state.maximized_restore.remove(&window_id) {
            state.space.map_element(window.clone(), loc, false);
            toplevel.with_pending_state(|s| {
                s.states.unset(xdg_toplevel::State::Maximized);
                s.size = Some(size);
            });
        } else {
            toplevel.with_pending_state(|s| {
                s.states.unset(xdg_toplevel::State::Maximized);
            });
        }
    }
    state.emit_window_geometry(window_id);
    emit_event(AeroEvent::Maximized {
        id: window_id,
        maximized: target,
    });
    let _ = toplevel.send_configure();
    log_state(state, window_id, false, target);
    refresh_frame_state(state, window_id);
    true
}

pub fn apply_fullscreen(state: &mut AeroState, window_id: u32, target: bool) -> bool {
    let Some(window) = state.window_for_id(window_id).cloned() else {
        return false;
    };
    let Some(toplevel) = window.toplevel() else {
        return false;
    };
    if target {
        let Some(output) = state.output_for_window(window_id) else {
            return false;
        };
        let geo = state.space.output_geometry(&output).unwrap_or_default();
        if let Some(current) = state.space.element_geometry(&window) {
            state
                .fullscreen_restore
                .entry(window_id)
                .or_insert((current.loc, current.size));
        }
        state.space.map_element(window.clone(), geo.loc, false);
        // Fullscreen ignora min/max: ocupa o output inteiro, sem decoração.
        let size = geo.size;
        toplevel.with_pending_state(|s| {
            s.states.set(xdg_toplevel::State::Fullscreen);
            s.size = Some(size);
        });
    } else if let Some((loc, size)) = state.fullscreen_restore.remove(&window_id) {
        state.space.map_element(window.clone(), loc, false);
        toplevel.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Fullscreen);
            s.size = Some(size);
        });
    } else {
        toplevel.with_pending_state(|s| {
            s.states.unset(xdg_toplevel::State::Fullscreen);
        });
    }
    state.emit_window_geometry(window_id);
    emit_event(AeroEvent::Fullscreen {
        id: window_id,
        fullscreen: target,
    });
    let _ = toplevel.send_configure();
    log_state(state, window_id, target, false);
    refresh_frame_state(state, window_id);
    true
}

/// Diálogo com `set_parent`: posiciona relativo ao pai, eleva e foca o filho.
/// Um hook de destruição no pai fecha o filho (modal não sobrevive ao pai).
pub fn apply_parent(state: &mut AeroState, child_surface: &WlSurface) {
    let Some(toplevel) = state
        .window_surfaces
        .iter()
        .find(|(_, s)| *s == child_surface)
        .and_then(|(id, _)| state.window_for_id(*id))
        .and_then(|w| w.toplevel().cloned())
    else {
        return;
    };
    let Some(parent_surface) = toplevel.parent() else {
        return;
    };
    let child_id = state
        .window_surfaces
        .iter()
        .find(|(_, s)| *s == child_surface)
        .map(|(id, _)| *id);
    let parent_geo = state
        .window_surfaces
        .iter()
        .find(|(_, s)| *s == &parent_surface)
        .and_then(|(id, _)| state.window_for_id(*id))
        .and_then(|w| state.space.element_geometry(w));
    if let (Some(child_id), Some(parent_geo)) = (child_id, parent_geo) {
        let loc = Point::new(parent_geo.loc.x + 40, parent_geo.loc.y + 40);
        if let Some(child_window) = state.window_for_id(child_id).cloned() {
            state.space.map_element(child_window.clone(), loc, false);
            state.space.raise_element(&child_window, true);
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            if let Some(keyboard) = state.seat.get_keyboard() {
                keyboard.set_focus(state, Some(toplevel.wl_surface().clone()), serial);
            }
        }
        #[cfg(feature = "bench")]
        eprintln!(
            "XDG_PARENT t={} child={child_id} parent={:?} z=above focus=child",
            crate::frame_diagnostics::now_ns(),
            parent_surface.id()
        );
        if !state
            .transient_parents
            .iter()
            .any(|(c, p)| c == child_surface && p == &parent_surface)
        {
            state
                .transient_parents
                .push((child_surface.clone(), parent_surface.clone()));
            let child = child_surface.clone();
            add_destruction_hook::<crate::state::AeroState, _>(
                &parent_surface,
                move |state, _dead| {
                    state.transient_parents.retain(|(c, _)| c != &child);
                    if let Some((id, _)) = state.window_surfaces.iter().find(|(_, s)| *s == &child)
                    {
                        if let Some(window) = state.window_for_id(*id) {
                            if let Some(tl) = window.toplevel() {
                                tl.send_close();
                            }
                        }
                    }
                    #[cfg(feature = "bench")]
                    eprintln!(
                        "XDG_PARENT t={} parent_destroyed child_closed=true",
                        crate::frame_diagnostics::now_ns()
                    );
                },
            );
        }
    }
}

/// `app_id` e modo de decoração para o IPC (somente leitura; a decisão de CSD
/// continua no shell, não aqui).
pub fn app_id_for(window: &Window) -> String {
    window
        .toplevel()
        .and_then(|t| {
            with_states(t.wl_surface(), |states| {
                states
                    .data_map
                    .get::<XdgToplevelSurfaceData>()
                    .and_then(|data| data.lock().ok().and_then(|attrs| attrs.app_id.clone()))
            })
        })
        .unwrap_or_default()
}
