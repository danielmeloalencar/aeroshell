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

/// Backend de sessão real (F1d3). Abre sessão libseat, um dispositivo DRM,
/// cria renderer EGL/GBM, inicializa cada conector conectado como `Output` e
/// dirige o ciclo render/vblank/libinput.
#[cfg(all(
    feature = "backend_udev",
    feature = "backend_gbm",
    feature = "backend_libinput",
    feature = "backend_session_libseat"
))]
pub fn init_udev(
    _opts: &Options,
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    use std::{
        cell::{Cell, RefCell},
        path::PathBuf,
        rc::Rc,
        time::Duration,
    };

    use smithay::{
        backend::{
            allocator::{
                gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
                Format, Fourcc, Modifier,
            },
            drm::{
                compositor::FrameFlags,
                exporter::gbm::GbmFramebufferExporter,
                output::{DrmOutput, DrmOutputManager, DrmOutputRenderElements},
                DrmDevice, DrmDeviceFd, DrmEvent,
            },
            egl::{EGLContext, EGLDisplay},
            input::{InputEvent, KeyState, KeyboardKeyEvent},
            libinput::{LibinputInputBackend, LibinputSessionInterface},
            renderer::gles::GlesRenderer,
            session::{libseat::LibSeatSession, Event as SessionEvent, Session},
            udev::{all_gpus, primary_gpu, UdevBackend as UdevMonitor},
        },
        output::{Mode as WlMode, Output, PhysicalProperties},
        reexports::{
            calloop::timer::{TimeoutAction, Timer},
            drm::control::{connector::State, Device as _, ModeTypeFlags},
            input::Libinput,
            rustix::fs::OFlags,
        },
        utils::DeviceFd,
    };

    #[cfg(feature = "bench")]
    use smithay::wayland::compositor::{with_states, SurfaceAttributes};

    use crate::render::{elements::AeroRenderElements, scene::collect_output_elements};

    type Slot =
        DrmOutput<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;
    struct OutputSlot {
        crtc: smithay::reexports::drm::control::crtc::Handle,
        wl_output: Output,
        drm_output: Slot,
    }

    // 1. Sessão libseat (logind/seatd). Sem sudo nem /etc (AGENTS.md §3.8).
    let (mut session, session_notifier) =
        LibSeatSession::new().map_err(|e| format!("libseat: {e}"))?;
    let seat = session.seat();
    tracing::info!(active = session.is_active(), seat = %seat, "UDEV_SESSION");

    // 2. Descoberta de GPU via udev. AERO_DRM_DEVICE permite fixar o nó.
    let udev = UdevMonitor::new(&seat).map_err(|e| format!("udev: {e}"))?;
    let path: PathBuf = std::env::var_os("AERO_DRM_DEVICE")
        .map(PathBuf::from)
        .or_else(|| primary_gpu(&seat).ok().flatten())
        .or_else(|| all_gpus(&seat).ok().and_then(|v| v.into_iter().next()))
        .ok_or("nenhum dispositivo DRM encontrado para a seat")?;
    tracing::info!(?path, "dispositivo DRM selecionado");

    // 3. Abrir o device pela sessão (é isso que dá master no VT ativo).
    let raw = session
        .open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("abrir {path:?} via libseat: {e}"))?;
    let drm_fd = DrmDeviceFd::new(DeviceFd::from(raw));
    let (drm, drm_notifier) =
        DrmDevice::new(drm_fd.clone(), true).map_err(|e| format!("DrmDevice::new: {e}"))?;
    let gbm = GbmDevice::new(drm_fd.clone()).map_err(|e| format!("GbmDevice::new: {e}"))?;
    tracing::info!(active = drm.is_active(), "UDEV_DRM");

    // SAFETY: GbmDevice é uma native display EGL válida (EGLDisplay::new aceita
    // qualquer tipo que implemente `AsRawDisplay`); o GBM/fd permanecem vivos
    // enquanto o renderer existir, e nenhum outro EGLDisplay é iniciado nesta thread.
    let display = unsafe { EGLDisplay::new(gbm.clone())? };
    let context = EGLContext::new(&display)?;
    // SAFETY: contexto EGL recém-criado e válido; nenhum outro contexto ativo na thread.
    let mut renderer = unsafe { GlesRenderer::new(context)? };
    tracing::info!("udev: renderer EGL/GLES criado");

    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );
    let exporter = GbmFramebufferExporter::new(gbm.clone(), None);
    let color_formats = [Fourcc::Argb8888, Fourcc::Abgr8888, Fourcc::Xrgb8888];
    let mut render_formats: Vec<_> = renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect();
    if render_formats.is_empty() {
        // drivers software (llvmpipe nesta VM) podem não anunciar modifiers;
        // o fallback implícito é o único caminho seguro medido pela sonda F1d2.
        render_formats = color_formats
            .iter()
            .map(|code| Format {
                code: *code,
                modifier: Modifier::Invalid,
            })
            .collect();
    }
    tracing::info!(formats = render_formats.len(), "udev: formatos de render");
    let mut manager = DrmOutputManager::new(
        drm,
        allocator,
        exporter,
        Some(gbm),
        color_formats,
        render_formats,
    );

    // 4. Enumera conectores via drm crate (sem smithay_drm_extras).
    let handles = manager
        .device()
        .resource_handles()
        .map_err(|e| format!("drmModeGetResources: {e}"))?;
    let mut crtcs = handles.crtcs().iter().copied();
    let slots: Rc<RefCell<Vec<OutputSlot>>> = Rc::new(RefCell::new(Vec::new()));
    for conn_handle in handles.connectors() {
        let info = manager
            .device()
            .get_connector(*conn_handle, false)
            .map_err(|e| format!("drmModeGetConnector: {e}"))?;
        if info.state() != State::Connected {
            continue;
        }
        let Some(crtc) = crtcs.next() else {
            tracing::warn!("sem CRTC livre para {}", info);
            break;
        };
        let mode = info
            .modes()
            .iter()
            .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
            .copied()
            .or_else(|| info.modes().first().copied())
            .ok_or_else(|| format!("{} sem modo", info))?;
        let wl_mode = WlMode::from(mode);
        let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
        let output = Output::new(
            name,
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: info.subpixel().into(),
                make: "AeroShell".into(),
                model: "drm".into(),
            },
        );
        let _global = output.create_global::<crate::state::AeroState>(&data.display_handle);
        output.set_preferred(wl_mode);
        output.change_current_state(Some(wl_mode), None, None, None);
        data.state.space.map_output(&output, (0, 0));
        data.state.outputs.push(output.clone());

        let planes = manager
            .device()
            .planes(&crtc)
            .map_err(|e| format!("planes: {e}"))?;
        let drm_output = manager
            .initialize_output::<GlesRenderer, AeroRenderElements<GlesRenderer>>(
                crtc,
                mode,
                &[info.handle()],
                &output,
                Some(planes),
                &mut renderer,
                &DrmOutputRenderElements::default(),
            )
            .map_err(|e| format!("initialize_output: {e:?}"))?;
        slots.borrow_mut().push(OutputSlot {
            crtc,
            wl_output: output,
            drm_output,
        });
        tracing::info!(
            ?crtc,
            active = manager.device().is_active(),
            connectors = 1,
            "UDEV_OUTPUT"
        );
    }
    tracing::info!(
        outputs = slots.borrow().len(),
        "udev: conectores inicializados"
    );
    if slots.borrow().is_empty() {
        return Err("nenhum conector DRM conectado".into());
    }

    // 5. libinput ligado à mesma sessão (abre devices via libseat).
    let mut libinput =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    libinput
        .udev_assign_seat(&seat)
        .map_err(|_| "udev_assign_seat falhou")?;
    let input_backend = LibinputInputBackend::new(libinput.clone());
    // O libinput abre /dev/input/event* via libseat e os "graba": o manipulador
    // de VT do kernel não recebe Ctrl+Alt+F*n*. O compositor tem de chamar
    // `change_vt` ele próprio (padrão do anvil). Bit 1 = Ctrl, bit 2 = Alt.
    let mut vt_session = session.clone();
    let vt_modifiers = Rc::new(Cell::new(0u8));
    event_loop
        .handle()
        .insert_source(input_backend, move |event, _, data| {
            if let InputEvent::Keyboard { event } = &event {
                // ATENÇÃO: o libinput do Smithay devolve keycode XKB = evdev + 8
                // (smithay-0.7.0/src/backend/libinput/mod.rs:113-117). Não usar os
                // KEY_* crus de input-event-codes.h aqui.
                let code: u32 = event.key_code().into();
                let pressed = event.state() == KeyState::Pressed;
                let mask = match code {
                    37 | 105 => 1, // KEY_LEFTCTRL+8 / KEY_RIGHTCTRL+8
                    64 | 108 => 2, // KEY_LEFTALT+8 / KEY_RIGHTALT+8
                    _ => 0,
                };
                let modifiers = vt_modifiers.get();
                if mask != 0 {
                    vt_modifiers.set(if pressed {
                        modifiers | mask
                    } else {
                        modifiers & !mask
                    });
                }
                if mask != 0 || (67..=70).contains(&code) {
                    tracing::debug!(code, pressed, modifiers, "UDEV_KEY");
                }
                if pressed && modifiers == 3 && (67..=70).contains(&code) {
                    // KEY_F1..KEY_F4 em XKB => VT 1..4. Não repassar ao cliente.
                    let vt = (code - 66) as i32;
                    let result = vt_session.change_vt(vt);
                    tracing::info!(
                        key = format!("F{vt}"),
                        vt,
                        result = if result.is_ok() { "ok" } else { "err" },
                        "UDEV_VT"
                    );
                    return;
                }
            }
            data.state.process_input_event(event);
        })?;

    // 6. Pausa/retomada da sessão (VT switch feito pelo humano).
    // `activate(false)` preserva o estado atual do crank/connector (anvil udev.rs);
    // `true` faria modeset completo e poderia piscar.
    let notifier_session = session.clone();
    event_loop
        .handle()
        .insert_source(session_notifier, move |event, _, _| match event {
            SessionEvent::PauseSession => {
                libinput.suspend();
                manager.pause();
                tracing::info!(
                    active = notifier_session.is_active(),
                    "UDEV_SESSION_EVENT event=PauseSession"
                );
            }
            SessionEvent::ActivateSession => {
                if let Err(err) = manager.activate(false) {
                    tracing::warn!(?err, "falha ao reativar DRM");
                }
                if let Err(err) = libinput.resume() {
                    tracing::warn!(?err, "falha ao retomar libinput");
                }
                tracing::info!(
                    active = notifier_session.is_active(),
                    "UDEV_SESSION_EVENT event=ActivateSession"
                );
            }
        })?;

    // 7. VBlank: completa o ciclo do swapchain de cada output.
    let vblank_slots = slots.clone();
    let vblank_count = Rc::new(Cell::new(0u64));
    let vblank_source_count = vblank_count.clone();
    event_loop
        .handle()
        .insert_source(drm_notifier, move |event, _, _| match event {
            DrmEvent::VBlank(crtc) => {
                let n = vblank_source_count.get() + 1;
                vblank_source_count.set(n);
                let submitted = if let Some(slot) =
                    vblank_slots.borrow().iter().find(|slot| slot.crtc == crtc)
                {
                    match slot.drm_output.frame_submitted() {
                        Ok(_) => "ok",
                        Err(err) => {
                            tracing::warn!(?err, "frame_submitted falhou");
                            "err"
                        }
                    }
                } else {
                    "no-slot"
                };
                if n <= 10 || n.is_multiple_of(10) {
                    tracing::debug!(?crtc, n, submitted, "UDEV_VBLANK");
                }
            }
            DrmEvent::Error(err) => tracing::warn!(?err, "erro DRM"),
        })?;

    // 8. udev hotplug: reportado, não tratado nesta primeira versão.
    event_loop
        .handle()
        .insert_source(udev, move |event, _, _| match event {
            smithay::backend::udev::UdevEvent::Added { path, .. } => {
                tracing::info!(?path, "dispositivo DRM adicionado (reinicie para usar)")
            }
            smithay::backend::udev::UdevEvent::Removed { device_id, .. } => {
                tracing::info!(device_id, "dispositivo DRM removido")
            }
            smithay::backend::udev::UdevEvent::Changed { device_id } => {
                tracing::info!(device_id, "dispositivo DRM alterado")
            }
        })?;

    // 9. Loop de render por timer (mesma cena de headless/winit via render::scene).
    let mut tick: u64 = 0;
    let mut frame_seq: u64 = 0;
    event_loop.handle().insert_source(
        Timer::from_duration(Duration::from_millis(16)),
        move |_, _, data| {
            tick += 1;
            // Heartbeat: se o log parar aqui, o travamento está no ciclo de render.
            if tick.is_multiple_of(250) {
                tracing::info!(tick, "udev: loop de render vivo");
            }
            // F1d3: ler a fila de frame-callbacks SEM drenar. Quem drena e envia o
            // `done()` é `send_frames_surface_tree` (smithay 0.7.0
            // desktop/wayland/utils.rs:217) — este backend NÃO chama essa API.
            // A sonda só observa: pending>0 significa que o cliente espera um
            // `done()` que não chegou.
            #[cfg(feature = "bench")]
            if tick.is_multiple_of(60) {
                let surface = data
                    .state
                    .space
                    .elements()
                    .find_map(|window| window.toplevel().map(|toplevel| toplevel.wl_surface()));
                if let Some(surface) = surface {
                    let pending = with_states(surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .frame_callbacks
                            .len()
                    });
                    let t = crate::frame_diagnostics::now_ns();
                    eprintln!("UDEV_FRAME_CB t={t} surface=client pending={pending}");
                }
            }
            let mut borrowed = slots.borrow_mut();
            for slot in borrowed.iter_mut() {
                frame_seq += 1;
                let (elements, _) =
                    collect_output_elements(&mut data.state, &slot.wl_output, &mut renderer, false);
                match slot.drm_output.render_frame(
                    &mut renderer,
                    &elements,
                    [0.08, 0.08, 0.10, 1.0],
                    FrameFlags::DEFAULT,
                ) {
                    Ok(result) => {
                        let is_empty = result.is_empty;
                        let needs_sync = result.needs_sync();
                        let queued = if is_empty {
                            "empty"
                        } else {
                            match slot.drm_output.queue_frame(()) {
                                Ok(()) => "ok",
                                Err(err) => {
                                    tracing::warn!(?err, "queue_frame falhou");
                                    "err"
                                }
                            }
                        };
                        // Debug em RUST_LOG=debug: TODO frame com conteúdo (não vazio)
                        // entra no log; os vazios só amostram a cadência. Assim o log
                        // mostra se a cena nova realmente foi enfileirada para o DRM.
                        if frame_seq <= 10
                            || frame_seq.is_multiple_of(250)
                            || !is_empty
                            || queued == "err"
                        {
                            tracing::debug!(
                                seq = frame_seq,
                                is_empty,
                                needs_sync,
                                queued,
                                "UDEV_FRAME"
                            );
                        }
                    }
                    Err(err) => {
                        tracing::warn!(?err, "render_frame falhou");
                        if frame_seq <= 10 || frame_seq.is_multiple_of(250) {
                            tracing::debug!(
                                seq = frame_seq,
                                is_empty = true,
                                needs_sync = false,
                                queued = "err",
                                "UDEV_FRAME"
                            );
                        }
                    }
                }
            }
            drop(borrowed);
            data.state.space.refresh();
            data.state.popups.cleanup();
            let _ = data.display_handle.flush_clients();
            TimeoutAction::ToDuration(Duration::from_millis(16))
        },
    )?;

    tracing::info!("backend udev ativo (libseat + DRM/GBM + libinput)");
    Ok(())
}

/// Build sem as features de sessão real: mantém a assinatura e falha com clareza.
#[cfg(not(all(
    feature = "backend_udev",
    feature = "backend_gbm",
    feature = "backend_libinput",
    feature = "backend_session_libseat"
)))]
pub fn init_udev(
    _opts: &Options,
    _event_loop: &mut EventLoop<CalloopData>,
    _data: &mut CalloopData,
) -> Result<(), Box<dyn Error>> {
    Err("backend udev requer as features backend_udev/backend_gbm/backend_libinput/backend_session_libseat (use o build padrão)".into())
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
