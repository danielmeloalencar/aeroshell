//! xdg_wm_base: toplevels, popups e initial configure.

use std::sync::atomic::{AtomicBool, Ordering};

use smithay::{
    delegate_kde_decoration, delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKind, PopupManager, Space, Window,
    },
    reexports::{
        wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
            Mode as KdeMode, OrgKdeKwinServerDecoration,
        },
        wayland_server::{
            protocol::{wl_seat, wl_surface::WlSurface},
            Resource, WEnum,
        },
    },
    utils::{Logical, Serial, Size},
    wayland::{
        compositor::with_states,
        shell::{
            kde::decoration::{KdeDecorationHandler, KdeDecorationState},
            xdg::{
                PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
                XdgToplevelSurfaceData,
            },
        },
    },
};

use crate::state::AeroState;
use smithay::reexports::wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    shell::server::xdg_toplevel::ResizeEdge as XdgResizeEdge,
};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

use crate::aero_frame_protocol::aero_frame_v1::ResizeEdge as FrameResizeEdge;
use crate::grab::start_geometry_grab;
use crate::protocols::aero_frame::serial_valid;

/// Mapa superfície → window_id lógico (inverso de `window_surfaces`).
fn window_id_for(state: &AeroState, surface: &WlSurface) -> Option<u32> {
    state
        .window_surfaces
        .iter()
        .find(|(_, candidate)| *candidate == surface)
        .map(|(id, _)| *id)
}

/// O cliente pediu para desenhar a própria moldura (GTK4/gedit). O shell ainda
/// responde ServerSide, mas usa este sinal para NÃO desenhar a barra HTML —
/// mesmo invariante do Domicile (`decorations.rs:40-70,93-98`).
#[derive(Default)]
struct ClientDrawsFrame(AtomicBool);

fn set_client_frame(surface: &WlSurface, value: bool) {
    with_states(surface, |states| {
        states
            .data_map
            .insert_if_missing_threadsafe(|| ClientDrawsFrame(AtomicBool::new(false)));
        if let Some(flag) = states.data_map.get::<ClientDrawsFrame>() {
            flag.0.store(value, Ordering::Relaxed);
        }
    });
}

pub fn client_draws_frame(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states
            .data_map
            .get::<ClientDrawsFrame>()
            .is_some_and(|flag| flag.0.load(Ordering::Relaxed))
    })
}

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

    // F1-csd-gtk: o GTK chama xdg_toplevel.move/resize a partir do próprio
    // headerbar. Reusamos os grabs do frame, com o mesmo serial validado.
    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, serial: Serial) {
        let Some(window_id) = window_id_for(self, surface.wl_surface()) else {
            return;
        };
        let serial = u32::from(serial);
        if !serial_valid(self.last_button_serial, serial) {
            #[cfg(feature = "bench")]
            eprintln!(
                "CSD_MOVE t={} window_id={window_id} decisao=deny reason=serial",
                crate::frame_diagnostics::now_ns()
            );
            return;
        }
        if start_geometry_grab(self, window_id, serial, None) {
            #[cfg(feature = "bench")]
            eprintln!(
                "CSD_MOVE t={} window_id={window_id} decisao=allow acao=move",
                crate::frame_diagnostics::now_ns()
            );
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        serial: Serial,
        edges: XdgResizeEdge,
    ) {
        let Some(window_id) = window_id_for(self, surface.wl_surface()) else {
            return;
        };
        let serial = u32::from(serial);
        if !serial_valid(self.last_button_serial, serial) {
            #[cfg(feature = "bench")]
            eprintln!(
                "CSD_RESIZE t={} window_id={window_id} decisao=deny reason=serial",
                crate::frame_diagnostics::now_ns()
            );
            return;
        }
        let edges = match edges {
            XdgResizeEdge::Top => FrameResizeEdge::Top,
            XdgResizeEdge::Bottom => FrameResizeEdge::Bottom,
            XdgResizeEdge::Left => FrameResizeEdge::Left,
            XdgResizeEdge::Right => FrameResizeEdge::Right,
            XdgResizeEdge::TopLeft => FrameResizeEdge::TopLeft,
            XdgResizeEdge::BottomLeft => FrameResizeEdge::BottomLeft,
            XdgResizeEdge::TopRight => FrameResizeEdge::TopRight,
            XdgResizeEdge::BottomRight => FrameResizeEdge::BottomRight,
            other => {
                #[cfg(feature = "bench")]
                eprintln!(
                    "CSD_RESIZE t={} window_id={window_id} decisao=deny reason=edge edge={other:?}",
                    crate::frame_diagnostics::now_ns()
                );
                #[cfg(not(feature = "bench"))]
                let _ = other;
                return;
            }
        };
        if start_geometry_grab(self, window_id, serial, Some(edges)) {
            #[cfg(feature = "bench")]
            eprintln!(
                "CSD_RESIZE t={} window_id={window_id} decisao=allow acao=resize",
                crate::frame_diagnostics::now_ns()
            );
        }
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

// Respondemos ServerSide a todos, mas guardamos quando o cliente pediu CSD.
// O foot larga a própria barra; GTK4/gedit NÃO — para eles o shell não deve
// desenhar a barra HTML (guia do Domicile: decorations.rs / WRITING-A-SHELL.md).
impl XdgDecorationHandler for AeroState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        set_client_frame(toplevel.wl_surface(), false);
        self.server_side_decoration(toplevel);
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        let client_frame = mode == Mode::ClientSide;
        set_client_frame(toplevel.wl_surface(), client_frame);
        #[cfg(feature = "bench")]
        if client_frame {
            eprintln!(
                "CSD_DETECT t={} window_id={} client_frame=true",
                crate::frame_diagnostics::now_ns(),
                window_id_for(self, toplevel.wl_surface())
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "?".to_owned())
            );
        }
        self.server_side_decoration(toplevel);
    }
    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        set_client_frame(toplevel.wl_surface(), false);
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

// GTK3/gedit pede decoração por org_kde_kwin_server_decoration, não pelo
// zxdg_toplevel_decoration. Mesmo padrão do Domicile (decorations.rs:55-70):
// respondemos ServerSide mas guardamos o pedido ClientSide para esconder
// nossa barra HTML.
impl KdeDecorationHandler for AeroState {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.kde_decoration_state
    }

    fn new_decoration(&mut self, surface: &WlSurface, _decoration: &OrgKdeKwinServerDecoration) {
        set_client_frame(surface, false);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        if let WEnum::Value(KdeMode::Client) = mode {
            set_client_frame(surface, true);
            #[cfg(feature = "bench")]
            eprintln!(
                "CSD_DETECT t={} window_id={} client_frame=true reason=kde",
                crate::frame_diagnostics::now_ns(),
                window_id_for(self, surface)
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "?".to_owned())
            );
        }
        decoration.mode(KdeMode::Server);
    }

    fn release(&mut self, _decoration: &OrgKdeKwinServerDecoration, surface: &WlSurface) {
        set_client_frame(surface, false);
    }
}
delegate_kde_decoration!(AeroState);

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
