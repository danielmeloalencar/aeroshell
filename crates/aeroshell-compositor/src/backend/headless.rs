//! Backend headless para CI/testes: socket e estado vivos, sem renderização.
//! Aceita comandos de ponteiro sintético por Unix socket (`AERO_TEST_INPUT_SOCK`):
//!   motion <x> <y>\n   button press|release\n

use std::error::Error;

use calloop::EventLoop;
use smithay::{
    output::{Mode, Output, PhysicalProperties, Subpixel},
    utils::Transform,
};

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
    opts: &Options,
    _event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    let size = opts
        .size
        .map(|(w, h)| (w as i32, h as i32).into())
        .unwrap_or((1280, 800).into());
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        "aero-headless".to_owned(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "AeroShell".into(),
            model: "headless".into(),
        },
    );
    let _global = output.create_global::<crate::state::AeroState>(&data.display_handle);
    output.change_current_state(Some(mode), Some(Transform::Flipped180), None, Some((0, 0).into()));
    output.set_preferred(mode);
    data.state.space.map_output(&output, (0, 0));
    data.state.outputs = vec![output];

    tracing::info!("backend headless ativo");
    Ok(())
}
