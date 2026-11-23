//! F1-dmabuf: global `zwp_linux_dmabuf_v1` com tranches e import de buffer.
//!
//! O global só é criado depois que o renderer de cada backend existe: a lista de
//! formatos anunciada vem de `ImportDma::dmabuf_formats()` — invariante de
//! domicílio: formato anunciado = formato que o compositor consegue importar.
//! O import real acontece no render (`ImportAll`), então o handler apenas
//! notifica o cliente uma vez com `successful()`.

use std::os::unix::fs::MetadataExt;

use smithay::{
    backend::{allocator::dmabuf::Dmabuf, renderer::ImportDma},
    delegate_dmabuf,
    reexports::wayland_server::DisplayHandle,
    wayland::dmabuf::{
        DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
    },
};

use crate::state::AeroState;

impl DmabufHandler for AeroState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        _dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        #[cfg(feature = "bench")]
        {
            use smithay::backend::allocator::Buffer;
            let format = _dmabuf.format();
            let size = _dmabuf.size();
            let t = crate::frame_diagnostics::now_ns();
            eprintln!(
                "DMABUF_IMPORT t={t} format=0x{:08x} modifier=0x{:016x} w={} h={}",
                format.code as u32,
                Into::<u64>::into(format.modifier),
                size.w,
                size.h
            );
        }
        // O renderer importa no render via ImportAll; não importar duas vezes.
        let _ = notifier.successful::<AeroState>();
    }
}

delegate_dmabuf!(AeroState);

/// Anuncia o global usando SOMENTE os formatos que o renderer importa.
/// `main_device` vem do nó de renderização padrão; se o nó não existir, 0 é
/// explícito (sem `unwrap`, sem inventar device).
pub fn announce<R: ImportDma>(
    state: &mut AeroState,
    renderer: &R,
    dh: &DisplayHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    let formats: Vec<_> = renderer.dmabuf_formats().iter().copied().collect();
    let device = std::fs::metadata(crate::cli::DEFAULT_RENDER_NODE)
        .map(|meta| meta.rdev())
        .unwrap_or(0);
    let feedback = DmabufFeedbackBuilder::new(device, formats.iter().copied()).build()?;
    let global = state
        .dmabuf_state
        .create_global_with_default_feedback::<AeroState>(dh, &feedback);
    #[cfg(feature = "bench")]
    {
        let t = crate::frame_diagnostics::now_ns();
        eprintln!(
            "DMABUF_GLOBAL t={t} formats={} device={device}",
            formats.len()
        );
    }
    state.dmabuf_global = Some(global);
    Ok(())
}
