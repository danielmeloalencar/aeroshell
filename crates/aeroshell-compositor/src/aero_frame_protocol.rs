//! Protocolo privado `aero_frame_v1`, gerado do XML (lado servidor).

mod core_interfaces {
    use wayland_backend;
    wayland_scanner::generate_interfaces!("/usr/share/wayland/wayland.xml");
}

mod generated {
    use crate::wayland_server;
    use wayland_backend;
    #[allow(unused_imports)]
    use super::core_interfaces::*;
    pub use wayland_server::protocol::{wl_seat, wl_surface};

    wayland_scanner::generate_interfaces!("../../protocols/aero_frame_v1.xml");
    wayland_scanner::generate_server_code!("../../protocols/aero_frame_v1.xml");
}

pub use generated::*;
