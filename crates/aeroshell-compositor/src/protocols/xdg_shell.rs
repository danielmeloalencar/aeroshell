//! xdg_wm_base: toplevels, popups e initial configure.

use std::sync::atomic::{AtomicBool, Ordering};

use smithay::{
    delegate_kde_decoration, delegate_xdg_shell,
    desktop::{
        find_popup_root_surface, get_popup_toplevel_coords, PopupKeyboardGrab, PopupKind,
        PopupManager, PopupPointerGrab, Space, Window,
    },
    input::pointer::Focus,
    reexports::{
        wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
            Mode as KdeMode, OrgKdeKwinServerDecoration,
        },
        wayland_server::{
            protocol::{wl_output::WlOutput, wl_seat, wl_surface::WlSurface},
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
use crate::protocols::toplevel_states::{
    apply_fullscreen, apply_maximize, apply_parent, clamp_size,
};

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

/// Evita loop de feedback: o GTK3 voltou a pedir Client cada vez que
/// respondemos Server; respondemos uma vez por superfície.
#[derive(Default)]
struct KdeAnsweredServer(AtomicBool);

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
        let wl_surface = surface.wl_surface().clone();
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
        // F1-xdg-toplevel-states: respeitar min/max já no configure inicial.
        let suggested = clamp_size(&wl_surface, Size::<i32, Logical>::new(800, 600), true);
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

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        #[cfg(feature = "bench")]
        {
            eprintln!(
                "POPUP_POSREQ surface={:?} size={:?} anchor={:?} gravity={:?} rect={:?} offset={:?} parent_size={:?}",
                surface.wl_surface().id(),
                positioner.rect_size,
                positioner.anchor_edges,
                positioner.gravity,
                positioner.anchor_rect,
                positioner.offset,
                positioner.parent_size
            );
        }
        #[cfg(not(feature = "bench"))]
        let _ = positioner;
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

    // F1-xdg-toplevel-states: os defaults do Smithay são no-op; honrar os
    // pedidos reaproveitando a mecânica da 02r (work area + frame geometry).
    fn maximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = window_id_for(self, surface.wl_surface()) {
            apply_maximize(self, id, true);
        }
    }

    fn unmaximize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = window_id_for(self, surface.wl_surface()) {
            apply_maximize(self, id, false);
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        if let Some(id) = window_id_for(self, surface.wl_surface()) {
            apply_fullscreen(self, id, true);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = window_id_for(self, surface.wl_surface()) {
            apply_fullscreen(self, id, false);
        }
    }

    fn parent_changed(&mut self, surface: ToplevelSurface) {
        apply_parent(self, surface.wl_surface());
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
        // `grab_popup()` só CRIA o `PopupGrab` (manager.rs:132): ele NÃO instala
        // grabs no seat. Sem `set_grab` no ponteiro/teclado, `is_grabbed()` é
        // false no clique do item de menu e o cliente Qt não recebe a ativação.
        // Fonte: Smithay 0.7.0 `desktop/wayland/popup/grab.rs:419-530`.
        let resultado = self
            .popups
            .grab_popup::<crate::state::AeroState>(root, popup, &self.seat, serial);
        match resultado {
            Ok(grab) => {
                let pointer_ok = if let Some(pointer) = self.seat.get_pointer() {
                    pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
                    true
                } else {
                    false
                };
                let keyboard_ok = if let Some(keyboard) = self.seat.get_keyboard() {
                    keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
                    true
                } else {
                    false
                };
                #[cfg(feature = "bench")]
                eprintln!(
                    "POPUP_GRAB t={} serial={} estado=ok instalado_pointer={} instalado_keyboard={}",
                    crate::frame_diagnostics::now_ns(),
                    u32::from(serial),
                    pointer_ok,
                    keyboard_ok
                );
                #[cfg(not(feature = "bench"))]
                let _ = (pointer_ok, keyboard_ok);
            }
            Err(err) => {
                tracing::warn!(?err, "grab de popup recusado");
                #[cfg(feature = "bench")]
                eprintln!(
                    "POPUP_GRAB t={} serial={} estado=erro",
                    crate::frame_diagnostics::now_ns(),
                    u32::from(serial)
                );
            }
        }
    }
}

delegate_xdg_shell!(AeroState);

// Respondemos ServerSide a todos, mas guardamos quando o cliente pediu CSD.
// O foot larga a própria barra; GTK4/gedit NÃO — para eles o shell não deve
// desenhar a barra HTML (guia do Domicile: decorations.rs / WRITING-A-SHELL.md).
impl XdgDecorationHandler for AeroState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        #[cfg(feature = "bench")]
        eprintln!(
            "CSD_DECO t={} window_id={} acao=new",
            crate::frame_diagnostics::now_ns(),
            window_id_for(self, toplevel.wl_surface())
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_owned())
        );
        set_client_frame(toplevel.wl_surface(), false);
        self.server_side_decoration(toplevel);
    }
    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: Mode) {
        #[cfg(feature = "bench")]
        eprintln!(
            "CSD_DECO t={} window_id={} acao=request mode={mode:?}",
            crate::frame_diagnostics::now_ns(),
            window_id_for(self, toplevel.wl_surface())
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_owned())
        );
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
        #[cfg(feature = "bench")]
        eprintln!(
            "CSD_DECO t={} window_id={} acao=kde-new",
            crate::frame_diagnostics::now_ns(),
            window_id_for(self, surface)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_owned())
        );
        set_client_frame(surface, false);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        #[cfg(feature = "bench")]
        eprintln!(
            "CSD_DECO t={} window_id={} acao=kde-request mode={mode:?}",
            crate::frame_diagnostics::now_ns(),
            window_id_for(self, surface)
                .map(|id| id.to_string())
                .unwrap_or_else(|| "?".to_owned())
        );
        let first_answer = with_states(surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(|| KdeAnsweredServer(AtomicBool::new(false)));
            states
                .data_map
                .get::<KdeAnsweredServer>()
                .map(|flag| !flag.0.swap(true, Ordering::Relaxed))
                .unwrap_or(false)
        });
        if let WEnum::Value(KdeMode::Client) = mode {
            set_client_frame(surface, true);
            #[cfg(feature = "bench")]
            if first_answer {
                eprintln!(
                    "CSD_DETECT t={} window_id={} client_frame=true reason=kde",
                    crate::frame_diagnostics::now_ns(),
                    window_id_for(self, surface)
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "?".to_owned())
                );
            }
        }
        if first_answer {
            decoration.mode(KdeMode::Server);
        }
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
    #[cfg(feature = "bench")]
    if popups.find_popup(surface).is_some() {
        let (region, buffer) = with_states(surface, |states| {
            let mut attrs = states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>();
            let current = attrs.current();
            let buf = match &current.buffer {
                Some(smithay::wayland::compositor::BufferAssignment::NewBuffer(b)) => {
                    smithay::backend::renderer::buffer_dimensions(b)
                        .map(|s| format!("{}x{}", s.w, s.h))
                        .unwrap_or_else(|| "?".to_owned())
                }
                Some(smithay::wayland::compositor::BufferAssignment::Removed) => {
                    "removed".to_owned()
                }
                None => "none".to_owned(),
            };
            (current.input_region.clone(), buf)
        });
        eprintln!(
            "POPUP_STATE t={} surface={:?} input_region={region:?} buffer={buffer}",
            crate::frame_diagnostics::now_ns(),
            surface.id()
        );
    }
    if let Some(PopupKind::Xdg(popup)) = popups.find_popup(surface) {
        if !popup.is_initial_configure_sent() {
            let _ = popup.send_configure();
            #[cfg(feature = "bench")]
            {
                let t = crate::frame_diagnostics::now_ns();
                let coords = get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
                eprintln!(
                    "POPUP_COMMIT t={t} surface={:?} toplevel_coords={coords:?}",
                    popup.wl_surface().id()
                );
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
            let before = state.positioner.get_geometry();
            #[cfg(not(feature = "bench"))]
            let _ = before;
            let after = state.positioner.get_unconstrained_geometry(target);
            #[cfg(feature = "bench")]
            {
                eprintln!(
                    "POPUP_UNCONSTRAIN surface={:?} before={before:?} after={after:?} target={target:?} window_geo={window_geo:?}",
                    popup.wl_surface().id()
                );
            }
            state.geometry = after;
        });
    }
}
