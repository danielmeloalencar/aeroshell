//! F1-presentation-time: `wp_presentation` com feedback no ponto de apresentação.
//!
//! Invariantes:
//! - `seq` é monotônico POR OUTPUT (não global);
//! - o timestamp vem de CLOCK_MONOTONIC, o mesmo `clock_id` anunciado ao cliente;
//! - `refresh` vem do modo atual do output (mHz → Duration);
//! - os callbacks são drenados ANTES de `send_frame`, como manda a doc do Smithay
//!   (`wayland/presentation/mod.rs:28-74`).

use std::time::Duration;

use smithay::{
    delegate_presentation,
    desktop::{layer_map_for_output, utils::OutputPresentationFeedback},
    output::Output,
    reexports::{
        rustix::time::{clock_gettime, ClockId},
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind,
    },
    wayland::presentation::{PresentationFeedbackCachedState, Refresh},
};

use crate::state::AeroState;

/// `clock_id` anunciado no global e usado para todos os timestamps.
pub fn clock_id() -> u32 {
    ClockId::Monotonic as u32
}

/// Tempo REAL do relógio monotônico (não é `Instant::elapsed` nem epoch).
pub fn monotonic_now() -> Duration {
    let ts = clock_gettime(ClockId::Monotonic);
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

pub fn refresh_for(output: &Output) -> Refresh {
    match output.current_mode().map(|mode| mode.refresh) {
        Some(mhz) if mhz > 0 => Refresh::fixed(Duration::from_secs_f64(1000.0 / mhz as f64)),
        _ => Refresh::Unknown,
    }
}

fn pending_count(state: &AeroState, output: &Output) -> usize {
    let mut n = 0;
    for window in state.space.elements_for_output(output) {
        if let Some(surface) = window.toplevel().map(|t| t.wl_surface().clone()) {
            n += smithay::wayland::compositor::with_states(&surface, |states| {
                states
                    .data_map
                    .get::<PresentationFeedbackCachedState>()
                    .map(|s| s.callbacks.len())
                    .unwrap_or(0)
            });
        }
    }
    for layer in layer_map_for_output(output).layers() {
        n += smithay::wayland::compositor::with_states(layer.wl_surface(), |states| {
            states
                .data_map
                .get::<PresentationFeedbackCachedState>()
                .map(|s| s.callbacks.len())
                .unwrap_or(0)
        });
    }
    n
}

/// Cole os callbacks das superfícies apresentadas neste output. Usado pelo
/// backend udev, que apresenta no vblank (`frame_submitted`).
pub fn take_feedback(state: &AeroState, output: &Output) -> OutputPresentationFeedback {
    let mut feedback = OutputPresentationFeedback::new(output);
    for window in state.space.elements_for_output(output) {
        window.take_presentation_feedback(
            &mut feedback,
            |_, _| Some(output.clone()),
            |_, _| Kind::Vsync,
        );
    }
    for layer in layer_map_for_output(output).layers() {
        layer.take_presentation_feedback(
            &mut feedback,
            |_, _| Some(output.clone()),
            |_, _| Kind::Vsync,
        );
    }
    feedback
}

/// Sequência monotônica POR OUTPUT (não global).
pub fn next_seq(state: &mut AeroState, output: &Output) -> u64 {
    let name = output.name();
    let entry = state.presentation_seq.entry(name).or_insert(0);
    *entry = entry.saturating_add(1);
    *entry
}

/// Apresenta os callbacks pendentes de um output AGORA.
///
/// No headless o "agora" é o tick do timer (não há vblank real): isto é
/// deliberadamente documentado como uma aproximação, não como scanout.
/// No winit é o retorno de `backend.submit()`; no udev o backend usa
/// `take_feedback()` e chama `presented()` no vblank.
pub fn present_output(state: &mut AeroState, output: &Output, flags: Kind) {
    let _name = output.name();
    let seq = next_seq(state, output);
    let n = pending_count(state, output);
    let mut feedback = take_feedback(state, output);
    let when = monotonic_now();
    feedback.presented::<Duration, smithay::utils::Monotonic>(
        when,
        refresh_for(output),
        seq,
        flags,
    );
    #[cfg(feature = "bench")]
    {
        let t = crate::frame_diagnostics::now_ns();
        eprintln!(
            "PRESENT_FEEDBACK t={t} output={_name} seq={seq} n={n} clock={} secs={} nsecs={}",
            clock_id(),
            when.as_secs(),
            when.subsec_nanos()
        );
    }
    #[cfg(not(feature = "bench"))]
    let _ = (n, when);
}

delegate_presentation!(AeroState);
