//! Backend udev/DRM/libinput/libseat para sessão real. F1.
//! F1d2 acrescenta apenas a sonda GBM de diagnose (`probe_gbm`), sem VT/master/sessão.

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

/// Enumera conectores/modos do nó primário (card0) sem adquirir master.
/// Usa apenas `smithay::reexports::drm` (já no grafo; nenhuma dependência nova).
#[cfg(feature = "backend_gbm")]
fn probe_drm_modes(node: &str) -> Result<serde_json::Value, String> {
    use std::{
        fs::File,
        os::fd::{AsFd, BorrowedFd, OwnedFd},
    };

    use smithay::reexports::drm::control::Device as _;

    struct Card(OwnedFd);
    impl AsFd for Card {
        fn as_fd(&self) -> BorrowedFd<'_> {
            self.0.as_fd()
        }
    }
    impl smithay::reexports::drm::Device for Card {}
    impl smithay::reexports::drm::control::Device for Card {}

    let file = File::options()
        .read(true)
        .write(true)
        .open(node)
        .map_err(|e| format!("abrir {node} (O_RDWR): {e}"))?;
    // NOTE: `acquire_master_lock()` é deliberadamente NÃO chamado: F1d2 não
    // troca VT e não interfere no master do compositor host.
    let card = Card(file.into());
    let handles = card
        .resource_handles()
        .map_err(|e| format!("drmModeGetResources({node}): {e}"))?;
    let mut connectors = Vec::new();
    for handle in handles.connectors() {
        let info = card
            .get_connector(*handle, false)
            .map_err(|e| format!("drmModeGetConnector({node}): {e}"))?;
        let modes = info
            .modes()
            .iter()
            .map(|mode| {
                let (width, height) = mode.size();
                serde_json::json!({
                    "name": mode.name().to_string_lossy(),
                    "width": width,
                    "height": height,
                    "vrefresh_hz": mode.vrefresh(),
                    "clock_khz": mode.clock(),
                })
            })
            .collect::<Vec<_>>();
        connectors.push(serde_json::json!({
            "connector": info.to_string(),
            "state": format!("{:?}", info.state()),
            "modes": modes,
        }));
    }
    Ok(serde_json::json!({
        "node": node,
        "connectors": connectors,
        "crtcs": handles.crtcs().len(),
        "encoders": handles.encoders().len(),
        "framebuffers": handles.framebuffers().len(),
    }))
}

/// Sonda GBM em modo diagnose (F1d2).
///
/// Abre um nó de renderização (`renderD128`) em O_RDWR, cria um `GbmDevice`
/// (chamada real a `gbm_create_device` — é isso que faz o `DT_NEEDED` de
/// `libgbm.so.1` aparecer, fechando a lacuna medida em F1d1), enumera formats e
/// modifiers, aloca um buffer object e um buffer via `GbmAllocator` (o caminho
/// que F1d3 vai usar). Imprime um JSON no stdout.
///
/// NÃO troca VT, NÃO adquire master, NÃO abre sessão libseat, NÃO escuta
/// libinput e NÃO cria socket Wayland: é seguro rodar dentro do compositor host.
#[cfg(feature = "backend_gbm")]
pub fn probe_gbm(node: &str) -> Result<(), Box<dyn Error>> {
    use std::{fs::File, os::fd::OwnedFd};

    use smithay::backend::allocator::{
        gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        Fourcc, Modifier,
    };

    let file = File::options()
        .read(true)
        .write(true)
        .open(node)
        .map_err(|e| format!("abrir {node} (O_RDWR): {e}"))?;
    let fd: OwnedFd = file.into();

    let device = GbmDevice::new(fd).map_err(|e| format!("gbm_create_device({node}): {e}"))?;
    let backend_name = device.backend_name().to_owned();

    let usage = GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT;

    // Lista curada: variantes existentes em drm-fourcc 2.2.0 (versão fixada no
    // Cargo.lock, compartilhada por smithay e gbm). `is_format_supported` é a
    // fonte da verdade — nada aqui é suposição sobre o hardware da VM.
    let candidates: &[(Fourcc, &str)] = &[
        (Fourcc::Xrgb8888, "Xrgb8888"),
        (Fourcc::Argb8888, "Argb8888"),
        (Fourcc::Xrgb2101010, "Xrgb2101010"),
        (Fourcc::Argb2101010, "Argb2101010"),
        (Fourcc::Rgb565, "Rgb565"),
        (Fourcc::Xrgb1555, "Xrgb1555"),
        (Fourcc::Abgr8888, "Abgr8888"),
        (Fourcc::Xbgr8888, "Xbgr8888"),
        (Fourcc::Nv12, "Nv12"),
        (Fourcc::Yuv420, "Yuv420"),
        (Fourcc::Yuyv, "Yuyv"),
    ];
    let modifiers: &[(Modifier, &str)] = &[
        (Modifier::Invalid, "Invalid"),
        (Modifier::Linear, "Linear"),
        (Modifier::I915_x_tiled, "I915_x_tiled"),
        (Modifier::I915_y_tiled, "I915_y_tiled"),
    ];

    let mut formats = Vec::new();
    for &(fourcc, name) in candidates {
        let render = device.is_format_supported(fourcc, GbmBufferFlags::RENDERING);
        let scanout = device.is_format_supported(fourcc, GbmBufferFlags::SCANOUT);
        let mut supported_mods = Vec::new();
        for &(modifier, mod_name) in modifiers {
            if let Some(planes) = device.format_modifier_plane_count(fourcc, modifier) {
                supported_mods.push(serde_json::json!({
                    "name": mod_name,
                    "value": u64::from(modifier),
                    "planes": planes,
                }));
            }
        }
        formats.push(serde_json::json!({
            "fourcc": name,
            "code": fourcc as u32,
            "render": render,
            "scanout": scanout,
            "modifiers": supported_mods,
        }));
    }

    // Primeiro formato realmente utilizável — a escolha é medida, não chute.
    let pick = candidates
        .iter()
        .find(|(fourcc, _)| {
            device.is_format_supported(*fourcc, GbmBufferFlags::RENDERING)
                && device.is_format_supported(*fourcc, GbmBufferFlags::SCANOUT)
        })
        .ok_or_else(|| "nenhum formato RENDERING|SCANOUT suportado".to_owned())?;
    let (pick_fourcc, pick_name) = *pick;

    // Alocação direta (gbm_bo_create) + export de dmabuf.
    let bo = device
        .create_buffer_object::<()>(64, 64, pick_fourcc, usage)
        .map_err(|e| format!("gbm_bo_create({pick_name}): {e}"))?;
    let allocation = serde_json::json!({
        "fourcc": pick_name,
        "width": bo.width(),
        "height": bo.height(),
        "stride": bo.stride(),
        "modifier": format!("{:?}", bo.modifier()),
        "dmabuf": bo.fd().is_ok(),
    });
    drop(bo);

    // Caminho do Smithay que F1d3 usa de fato.
    let mut allocator = GbmAllocator::new(device, usage);
    let buffer = allocator
        .create_buffer_with_flags(
            64,
            64,
            pick_fourcc,
            &[Modifier::Invalid, Modifier::Linear],
            usage,
        )
        .map_err(|e| format!("GbmAllocator::create_buffer_with_flags({pick_name}): {e}"))?;
    let via_allocator = serde_json::json!({
        "fourcc": pick_name,
        "width": buffer.width(),
        "height": buffer.height(),
        "stride": buffer.stride(),
        "modifier": format!("{:?}", buffer.modifier()),
        "dmabuf": buffer.fd().is_ok(),
    });

    // Modos são informativos: numa VM pode não haver conector ativo. Uma falha
    // aqui NÃO invalida a sonda GBM (que é o critério de F1d2).
    let card_node =
        std::env::var("AERO_DRM_CARD_NODE").unwrap_or_else(|_| "/dev/dri/card0".to_owned());
    let drm_modes = match probe_drm_modes(&card_node) {
        Ok(value) => value,
        Err(err) => serde_json::json!({ "node": card_node, "error": err }),
    };

    let report = serde_json::json!({
        "ok": true,
        "node": node,
        "backend_name": backend_name,
        "formats": formats,
        "allocation": allocation,
        "allocator": via_allocator,
        "drm_modes": drm_modes,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// `--no-default-features` compila sem `smithay/backend_gbm`; a sonda continua
/// existindo para a CLI, mas recusa a rodar com mensagem explícita.
#[cfg(not(feature = "backend_gbm"))]
pub fn probe_gbm(_node: &str) -> Result<(), Box<dyn Error>> {
    Err(
        "sonda GBM requer a feature backend_gbm (use o build padrão, sem --no-default-features)"
            .into(),
    )
}
