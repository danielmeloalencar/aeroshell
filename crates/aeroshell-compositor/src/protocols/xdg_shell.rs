//! xdg_wm_base: toplevels, popups e initial configure.

use smithay::{
    delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKind, PopupManager, Space, Window,
    },
    reexports::wayland_server::{
        protocol::{wl_seat, wl_surface::WlSurface},
        Resource,
    },
    utils::Serial,
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
            XdgToplevelSurfaceData,
        },
    },
};

use crate::state::AeroState;
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

impl XdgShellHandler for AeroState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // ADR 0004: superfície associada a frame NÃO entra na lista de janelas.
        // Suporte a xdg_toplevel como frame está deferido: enviar `closed`.
        let assoc = self
            .associated_frames
            .iter()
            .find(|(s, _, _, _)| s == surface.wl_surface())
            .map(|(_, frame, window_id, _)| (frame.clone(), *window_id));
        if let Some((frame, window_id)) = assoc {
            frame.closed();
            tracing::warn!(
                window_id,
                "xdg_toplevel associado a frame: closed enviado (suporte deferido)"
            );
            return;
        }
        tracing::info!("novo toplevel xdg-surface mapeado");
        let window = Window::new_wayland_window(surface);
        // Margem para a decoração do frame (insets superiores) ficar visível e
        // cascata para janelas empilhadas não coincidirem exatamente.
        let n = self.space.elements().count() as i32;
        let loc = (60 + n * 40, 80 + n * 40);
        self.space.map_element(window, loc, false);
        if let Some(window) = self.space.elements().last() {
            tracing::info!(
                surface = ?window.toplevel().map(|t| t.wl_surface().id()),
                geometry = ?self.space.element_geometry(window),
                "toplevel mapeado no space"
            );
        }
        // Cliente inicial adiado: o frame associou window_id=n antes de o cliente
        // existir; liga agora à superfície recém-mapeada.
        let wl = self
            .space
            .elements()
            .nth(n as usize)
            .and_then(|w| w.toplevel().map(|t| t.wl_surface().clone()));
        if let Some(wl) = wl {
            self.bind_pending_frame_client(n as u32, wl);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    // S1: grabs de move/resize entram em F3. Mantemos as janelas imóveis.
    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
    }

    fn resize_request(
        &mut self,
        _surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        _edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {}
}

delegate_xdg_shell!(AeroState);

// Decoração server-side: o frame HTML é a ÚNICA barra de título. Clientes com
// CSD (foot) recebem mode=ServerSide e largam a própria barra, alinhando o
// render (render_loc = geo.loc - geometry().loc) com a região arrastável.
impl XdgDecorationHandler for AeroState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.server_side_decoration(toplevel);
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        self.server_side_decoration(toplevel);
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.server_side_decoration(toplevel);
    }
}

impl AeroState {
    fn server_side_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        let _ = toplevel.send_configure();
    }
}

smithay::delegate_xdg_decoration!(AeroState);

/// Envia o configure inicial no primeiro commit do cliente.
pub fn handle_commit(popups: &mut PopupManager, space: &Space<Window>, surface: &WlSurface) {
    if let Some(window) = space
        .elements()
        .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(surface))
        .cloned()
    {
        let initial_configure_sent = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .expect("XdgToplevelSurfaceData ausente")
                .lock()
                .expect("lock envenenado")
                .initial_configure_sent
        });
        if !initial_configure_sent {
            let _ = window
                .toplevel()
                .expect("janela sem toplevel")
                .send_configure();
        }
    }

    popups.commit(surface);
    if let Some(PopupKind::Xdg(popup)) = popups.find_popup(surface) {
        if !popup.is_initial_configure_sent() {
            let _ = popup.send_configure();
        }
    }
}

impl AeroState {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(&root))
        else {
            return;
        };
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };
        let Some(window_geo) = self.space.element_geometry(window) else {
            return;
        };

        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
