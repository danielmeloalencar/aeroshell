//! Seat, data-device (clipboard/DnD) e output.

use smithay::{
    delegate_data_device, delegate_output, delegate_seat,
    input::{Seat, SeatHandler, SeatState},
    reexports::wayland_server::{
        protocol::wl_surface::WlSurface,
        Resource,
    },
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{
                set_data_device_focus, ClientDndGrabHandler, DataDeviceHandler, DataDeviceState,
                ServerDndGrabHandler,
            },
            SelectionHandler,
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

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}

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
        set_data_device_focus(&self.display_handle, seat, client);
    }
}

delegate_seat!(AeroState);

impl SelectionHandler for AeroState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for AeroState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for AeroState {}
impl ServerDndGrabHandler for AeroState {}

delegate_data_device!(AeroState);

impl OutputHandler for AeroState {}
delegate_output!(AeroState);
