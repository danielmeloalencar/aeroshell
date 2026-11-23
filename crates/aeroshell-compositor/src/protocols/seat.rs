//! Seat, data-device (clipboard/DnD) e output.

use std::time::Duration;

use smithay::{
    delegate_cursor_shape, delegate_data_device, delegate_fractional_scale,
    delegate_input_method_manager, delegate_output, delegate_primary_selection, delegate_seat,
    delegate_text_input_manager, delegate_viewporter, delegate_xdg_activation,
    desktop::{PopupKind, PopupManager},
    input::{pointer::CursorImageStatus, Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        compositor::with_states,
        fractional_scale::{with_fractional_scale, FractionalScaleHandler},
        input_method::{InputMethodHandler, PopupSurface as InputMethodPopupSurface},
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            primary_selection::{
                set_primary_focus, PrimarySelectionHandler, PrimarySelectionState,
            },
            SelectionHandler,
        },
        tablet_manager::TabletSeatHandler,
        text_input::TextInputSeat,
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
    },
};

use crate::state::AeroState;

impl SeatHandler for AeroState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<AeroState> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            match &image {
                CursorImageStatus::Hidden => eprintln!("CURSOR_SHAPE t={t} source=hidden"),
                CursorImageStatus::Named(icon) => {
                    eprintln!("CURSOR_SHAPE t={t} source=named shape={}", icon.name())
                }
                CursorImageStatus::Surface(surface) => {
                    eprintln!(
                        "CURSOR_SHAPE t={t} source=surface surface={:?}",
                        surface.id()
                    )
                }
            }
        }
        self.cursor_image = image;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let is_panel = focused
            .map(|s| self.is_interactive_layer_surface(s))
            .unwrap_or(false);
        tracing::info!(
            surface = ?focused.map(|s| s.id()),
            is_panel,
            "TESTE foco de teclado alterado"
        );
        let client = focused.and_then(|s| self.display_handle.get_client(s.id()).ok());
        set_data_device_focus(&self.display_handle, seat, client.clone());
        // F1-primary-selection: a primária segue o mesmo foco de teclado; o Smithay
        // recusa set_selection de cliente sem foco (device.rs:56-77).
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            let id = client
                .as_ref()
                .map(|c| format!("{:?}", c.id()))
                .unwrap_or_else(|| "none".to_owned());
            eprintln!("PRIMARY_FOCUS t={t} client={id}");
        }
        set_primary_focus(&self.display_handle, seat, client);

        // F1i: o foco de text_input acompanha o foco de teclado (enter/leave na
        // superfície focada). O Smithay encaminha `commit_string` ao IME ativo;
        // aqui só ligamos o foco — o protocolo em si é dele.
        let text_input = seat.text_input();
        text_input.leave();
        text_input.set_focus(focused.cloned());
        text_input.enter();
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            let id = focused
                .map(|s| format!("{:?}", s.id()))
                .unwrap_or_else(|| "none".to_owned());
            eprintln!("TI_FOCUS t={t} surface={id}");
        }
    }
}

delegate_seat!(AeroState);

// F1-cursor-shape: o `Dispatch` do device exige `SeatHandler + TabletSeatHandler`;
// o trait só tem métodos com corpo default (tablet_seat.rs:42-44).
impl TabletSeatHandler for AeroState {}
delegate_cursor_shape!(AeroState);

impl SelectionHandler for AeroState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for AeroState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

// F1-primary-selection: um único método obrigatório (mod.rs:92-95).
impl PrimarySelectionHandler for AeroState {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.primary_selection_state
    }
}
delegate_primary_selection!(AeroState);

impl ClientDndGrabHandler for AeroState {}
impl ServerDndGrabHandler for AeroState {}

delegate_data_device!(AeroState);

impl OutputHandler for AeroState {}
delegate_output!(AeroState);

// F1f: a escala preferida é propriedade da SUPERFÍCIE, não do output global.
// O cliente recebe `preferred_scale` quando cria o objeto e o render lê esse
// valor por superfície (render/scene.rs).
impl FractionalScaleHandler for AeroState {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self
            .outputs
            .first()
            .map(|output| output.current_scale().fractional_scale())
            .unwrap_or(1.0);
        with_states(&surface, |states| {
            with_fractional_scale(states, |fs| fs.set_preferred_scale(scale));
        });
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            eprintln!(
                "FSCALE_SURFACE t={t} surface={:?} scale={scale} source=preferred",
                surface.id()
            );
        }
    }
}
delegate_fractional_scale!(AeroState);
delegate_viewporter!(AeroState);

// F1i: text_input + input_method. O foco é propagado em `focus_changed` acima.
delegate_text_input_manager!(AeroState);

impl InputMethodHandler for AeroState {
    fn new_popup(&mut self, surface: InputMethodPopupSurface) {
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            eprintln!(
                "IME_POPUP t={t} acao=new surface={:?}",
                surface.wl_surface().id()
            );
        }
        let _ = self.popups.track_popup(PopupKind::InputMethod(surface));
    }

    fn dismiss_popup(&mut self, surface: InputMethodPopupSurface) {
        #[cfg(feature = "bench")]
        {
            let t = crate::frame_diagnostics::now_ns();
            eprintln!(
                "IME_POPUP t={t} acao=dismiss surface={:?}",
                surface.wl_surface().id()
            );
        }
        let popup = PopupKind::InputMethod(surface);
        let _ = PopupManager::dismiss_popup(popup.wl_surface(), &popup);
    }

    fn popup_repositioned(&mut self, _surface: InputMethodPopupSurface) {
        // A cena do Smithay lê a localização direto do PopupSurface; nada a fazer.
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .map(|t| t.wl_surface() == parent)
                    .unwrap_or(false)
            })
            .and_then(|window| self.space.element_geometry(window))
            .unwrap_or_default()
    }
}
delegate_input_method_manager!(AeroState);

// F1i/F7: xdg_activation. Política mínima anti-focus-stealing: exigir serial de
// evento de entrada, superfície de origem conhecida e token fresco (<= 5 s).
impl XdgActivationHandler for AeroState {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        #[cfg(feature = "bench")]
        let t = crate::frame_diagnostics::now_ns();

        let reason = if token_data.serial.is_none() {
            Some("sem_serial")
        } else if token_data.surface.is_none() {
            Some("sem_origem")
        } else if token_data.timestamp.elapsed() > Duration::from_secs(5) {
            Some("token_antigo")
        } else {
            None
        };
        if reason.is_some() {
            #[cfg(feature = "bench")]
            eprintln!(
                "XDG_ACTIVATE t={t} decisao=deny reason={} surface={:?}",
                reason.unwrap_or("desconhecido"),
                surface.id()
            );
            self.xdg_activation_state.remove_token(&token);
            return;
        }

        let window = self
            .space
            .elements()
            .find(|window| {
                window
                    .toplevel()
                    .map(|t| t.wl_surface() == &surface)
                    .unwrap_or(false)
            })
            .cloned();
        let Some(window) = window else {
            #[cfg(feature = "bench")]
            eprintln!(
                "XDG_ACTIVATE t={t} decisao=deny reason=sem_janela surface={:?}",
                surface.id()
            );
            self.xdg_activation_state.remove_token(&token);
            return;
        };

        self.space.raise_element(&window, true);
        let serial = SERIAL_COUNTER.next_serial();
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(self, Some(surface.clone()), serial);
        }
        #[cfg(feature = "bench")]
        eprintln!(
            "XDG_ACTIVATE t={t} decisao=allow reason=valido surface={:?}",
            surface.id()
        );
        self.xdg_activation_state.remove_token(&token);
    }
}
delegate_xdg_activation!(AeroState);

// F1-toplevel-icon-protocol: o handler próprio em `protocols/toplevel_icon.rs`
// copia os pixels no `add_buffer` e grava no cached state `IconPixels` da
// superfície. A sonda ICON_SET e o IPC `icon <id>` leem desse estado (via
// `window_ipc.rs`), sem registrar hooks de destruição em `wl_buffer`.
