//! Backend udev/DRM/libinput/libseat para sessão real. F1.

use std::error::Error;

use smithay::reexports::calloop::EventLoop;

use crate::{cli::Options, CalloopData};

pub struct UdevBackend;

impl super::Backend for UdevBackend {
    fn name(&self) -> &'static str {
        "udev"
    }
    fn seat_name(&self) -> String {
        "aeroshell-udev".to_owned()
    }
}

pub fn init_udev(
    _opts: &Options,
    _event_loop: &mut EventLoop<CalloopData>,
    _data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    Err("backend udev: implementação na F1 (requer libgbm/libseat e sessão real)".into())
}
