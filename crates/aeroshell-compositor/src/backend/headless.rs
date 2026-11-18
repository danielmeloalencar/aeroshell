//! Backend headless para CI/testes: socket e estado vivos, sem renderização.

use std::error::Error;

use smithay::reexports::calloop::EventLoop;

use crate::{cli::Options, CalloopData};

pub struct HeadlessBackend;

impl super::Backend for HeadlessBackend {
    fn name(&self) -> &'static str {
        "headless"
    }
    fn seat_name(&self) -> String {
        "aeroshell-headless".to_owned()
    }
}

pub fn init_headless(
    _opts: &Options,
    _event_loop: &mut EventLoop<CalloopData>,
    _data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    tracing::info!("backend headless ativo (sem saída gráfica)");
    Ok(())
}
