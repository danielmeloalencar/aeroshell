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
    utils::{Logical, Serial, Size},
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
        let client_surface = window.toplevel().map(|t| t.wl_surface().clone());
        // Margem para a decoração do frame (insets superiores) ficar visível e
        // cascata para janelas empilhadas não coincidirem exatamente.
        // Correção F1: ID monotônico, nunca reutilizado após o fechamento.
        let window_id = self.next_window_id;
        self.next_window_id = self.next_window_id.saturating_add(1);
        let n = window_id as i32;
        let loc = (60 + n * 40, 80 + n * 40);
        // Mecanismo padrão do xdg-shell: o compositor SUGERE um tamanho no
        // configure inicial. O cliente pode escolher outro, mas até o primeiro
        // commit a janela tem geometria 0×0 — é isso que fazia o placeholder do
        // compositor virar só as insets (12×36). 800×600 é a sugestão padrão
        // neutra (o cliente é livre para escolher o seu): não é constante do
        // tema, não depende do output e não estoura o canvas fixo da 02p.
        let suggested = Size::<i32, Logical>::new(800, 600);
        self.suggested_window_size.insert(window_id, suggested);
        if let Some(toplevel) = window.toplevel() {
            toplevel.with_pending_state(|state| state.size = Some(suggested));
            let _ = toplevel.send_configure();
        }
        #[cfg(feature = "bench")]
        eprintln!(
            "INITIAL_CONFIGURE t={} window_id={window_id} w={} h={}",
            crate::frame_diagnostics::now_ns(),
            suggested.w,
            suggested.h
        );
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
        if let Some(wl) = client_surface {
            self.window_surfaces.insert(n as u32, wl.clone());
            self.bind_pending_frame_client(n as u32, wl);
        }
        // Janelas que mapeiam SEM frame precisam ser anunciadas ao shell para que
        // ele crie uma WebView por window_id. O associate do cliente inicial cobre
        // só o id 0 (cold-start); sem isto o segundo cliente nunca recebe decoração.
        if !self.frames.iter().any(|(_, id)| *id == window_id) {
            if let Some(manager) = &self.shell_manager {
                let size = self.client_content_size(window_id).unwrap_or((0, 0));
                manager.frame_requested(
                    window_id,
                    size.0,
                    size.1,
                    crate::aero_frame_protocol::aero_frame_v1::State::empty(),
                );
            }
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            eprintln!("POPUP_NEW t={t} surface={:?}", surface.wl_surface().id());
        }
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

    fn grab(&mut self, surface: PopupSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(surface.clone())) else {
            #[cfg(feature = "bench")]
            eprintln!(
                "POPUP_GRAB t={} estado=sem_root",
                crate::frame_diagnostics::now_ns()
            );
            return;
        };
        let popup = PopupKind::Xdg(surface);
        // O `PopupGrab` vive no user_data do seat (grab.rs:99-102); o handle
        // local só guarda o estado até o dismiss, não precisa ser persistido.
        let resultado = self
            .popups
            .grab_popup::<crate::state::AeroState>(root, popup, &self.seat, serial);
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            let estado = match &resultado {
                Ok(_) => "ok",
                Err(err) => {
                    tracing::warn!(?err, "grab de popup recusado");
                    "erro"
                }
            };
            eprintln!(
                "POPUP_GRAB t={t} serial={} estado={estado}",
                u32::from(serial)
            );
        }
        let _ = resultado;
    }
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
            #[cfg(feature = "bench")]
            {
                let t = crate::frame_diagnostics::now_ns();
                eprintln!("POPUP_COMMIT t={t} surface={:?}", popup.wl_surface().id());
            }
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
