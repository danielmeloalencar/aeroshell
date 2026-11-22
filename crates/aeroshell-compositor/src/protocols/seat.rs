//! Seat, data-device (clipboard/DnD) e output.

use smithay::{
    delegate_cursor_shape, delegate_data_device, delegate_fractional_scale, delegate_output,
    delegate_primary_selection, delegate_seat, delegate_viewporter,
    input::{pointer::CursorImageStatus, Seat, SeatHandler, SeatState},
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    wayland::{
        compositor::with_states,
        fractional_scale::{with_fractional_scale, FractionalScaleHandler},
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
