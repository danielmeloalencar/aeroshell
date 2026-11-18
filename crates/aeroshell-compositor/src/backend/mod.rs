//! Único módulo que conhece a diferença entre nested, real e headless.

pub mod headless;
pub mod simulated;
pub mod udev;
pub mod winit;

use std::error::Error;

use smithay::reexports::calloop::EventLoop;

use crate::{cli::Options, CalloopData};

/// Contrato comum a todos os backends. O domínio nunca faz `match` no backend.
pub trait Backend {
    fn name(&self) -> &'static str;
    fn seat_name(&self) -> String;
}

/// Instancia o backend selecionado (usado para logging e para futuros objetos de domínio).
pub fn current(opts: &Options) -> Box<dyn Backend> {
    match opts.backend {
        crate::cli::BackendKind::Winit => Box::new(winit::WinitBackend),
        crate::cli::BackendKind::Headless => Box::new(headless::HeadlessBackend),
        crate::cli::BackendKind::Udev => Box::new(udev::UdevBackend),
    }
}

pub fn init(
    opts: &Options,
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    match opts.backend {
        crate::cli::BackendKind::Winit => winit::init_winit(opts, event_loop, data),
        crate::cli::BackendKind::Headless => headless::init_headless(opts, event_loop, data),
        crate::cli::BackendKind::Udev => udev::init_udev(opts, event_loop, data),
    }
}
