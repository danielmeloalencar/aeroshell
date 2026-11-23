#![cfg(feature = "backend_gbm")]
//! F1-dmabuf: cliente de teste com um buffer DMA-BUF real (GBM/PRIME).
//!
//! Sem dependência nova: `gbm` e os bindings do protocolo vêm dos reexports do
//! Smithay 0.7.0 fixado; `wayland-client` já é dev-dependency do workspace.
//!
//! Log (prefixo DMABUF_CLIENT) usado por tests/integration/f1_dmabuf.py.

use std::{
    fs::File,
    io::Read,
    os::fd::{AsFd, OwnedFd},
    time::{Duration, Instant},
};

use smithay::{
    backend::allocator::{Fourcc, Modifier},
    reexports::{
        gbm::{BufferObjectFlags, Device as GbmDevice, Format as GbmFormat},
        wayland_protocols::{
            wp::linux_dmabuf::zv1::client::{
                zwp_linux_buffer_params_v1::{Flags as ParamsFlags, ZwpLinuxBufferParamsV1},
                zwp_linux_dmabuf_feedback_v1::{Event as FeedbackEvent, ZwpLinuxDmabufFeedbackV1},
                zwp_linux_dmabuf_v1::{Event as DmabufEvent, ZwpLinuxDmabufV1},
            },
            xdg::shell::client::{
                xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
                xdg_toplevel::XdgToplevel,
                xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
            },
        },
    },
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_registry::Event as RegistryEvent,
        wl_registry::WlRegistry, wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

const W: u32 = 128;
const H: u32 = 96;
const RENDER_NODE: &str = "/dev/dri/renderD128";
const COLOR: (u8, u8, u8) = (220, 30, 30);

struct App {
    dmabuf: Option<ZwpLinuxDmabufV1>,
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    formats: Vec<(u32, u64)>,
    /// Tabela de formatos do `format_table` (16 bytes por entrada, nativo).
    format_table: Vec<u8>,
    /// Índices u16 das tranches (o evento carrega bytes em nativo).
    tranche_indices: Vec<u16>,
    configured: bool,
    fd: Option<OwnedFd>,
    buffer: Option<WlBuffer>,
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        state: &mut App,
        registry: &WlRegistry,
        event: RegistryEvent,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        let RegistryEvent::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "zwp_linux_dmabuf_v1" => {
                state.dmabuf = Some(registry.bind(name, version.min(5), qh, ()))
            }
            "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(6), qh, ())),
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufV1, ()> for App {
    fn event(
        state: &mut App,
        _: &ZwpLinuxDmabufV1,
        event: DmabufEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        match event {
            DmabufEvent::Format { format } => {
                eprintln!("DMABUF_CLIENT LEGACY_FORMAT fourcc=0x{format:08x}");
            }
            DmabufEvent::Modifier {
                format,
                modifier_hi,
                modifier_lo,
            } => {
                let modifier = ((modifier_hi as u64) << 32) | modifier_lo as u64;
                eprintln!("DMABUF_CLIENT FORMAT fourcc=0x{format:08x} modifier=0x{modifier:016x}");
                state.formats.push((format, modifier));
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxDmabufFeedbackV1, ()> for App {
    fn event(
        state: &mut App,
        _: &ZwpLinuxDmabufFeedbackV1,
        event: FeedbackEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        match event {
            FeedbackEvent::FormatTable { fd, size } => {
                let mut table = vec![0u8; size as usize];
                let mut file: File = fd.into();
                if file.read_exact(&mut table).is_ok() {
                    state.format_table = table;
                }
            }
            FeedbackEvent::TrancheFormats { indices } => {
                let mut i = 0;
                while i + 1 < indices.len() {
                    state
                        .tranche_indices
                        .push(u16::from_ne_bytes([indices[i], indices[i + 1]]));
                    i += 2;
                }
            }
            FeedbackEvent::Done => {
                for index in &state.tranche_indices {
                    let at = *index as usize * 16;
                    if at + 16 > state.format_table.len() {
                        continue;
                    }
                    let entry = &state.format_table[at..at + 16];
                    let format = u32::from_ne_bytes(entry[0..4].try_into().unwrap_or([0; 4]));
                    let modifier = u64::from_ne_bytes(entry[8..16].try_into().unwrap_or([0; 8]));
                    eprintln!(
                        "DMABUF_CLIENT FORMAT fourcc=0x{format:08x} modifier=0x{modifier:016x}"
                    );
                    if !state.formats.contains(&(format, modifier)) {
                        state.formats.push((format, modifier));
                    }
                }
                eprintln!("DMABUF_CLIENT TRANCHES formats={}", state.formats.len());
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgWmBase, ()> for App {
    fn event(
        _: &mut App,
        wm_base: &XdgWmBase,
        event: WmBaseEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let WmBaseEvent::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, u32> for App {
    fn event(
        state: &mut App,
        xdg: &XdgSurface,
        event: XdgSurfaceEvent,
        _: &u32,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let XdgSurfaceEvent::Configure { serial } = event {
            xdg.ack_configure(serial);
            if !state.configured {
                state.configured = true;
                eprintln!("DMABUF_CLIENT configured serial={serial}");
            }
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlBuffer);
delegate_noop!(App: ignore ZwpLinuxBufferParamsV1);
delegate_noop!(App: ignore XdgToplevel);

/// Bytes na memória para um fourcc ARGB8888/ABGR8888/XRGB8888 que resultam em
/// COLOR depois que o harness lê o dump BGRA (B,G,R,A).
fn pixel_bytes(code: Fourcc) -> [u8; 4] {
    let (r, g, b) = COLOR;
    match code {
        Fourcc::Abgr8888 => [r, g, b, 255],
        Fourcc::Argb8888 | Fourcc::Xrgb8888 => [b, g, r, 255],
        _ => [b, g, r, 255],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        dmabuf: None,
        compositor: None,
        wm_base: None,
        formats: Vec::new(),
        format_table: Vec::new(),
        tranche_indices: Vec::new(),
        configured: false,
        fd: None,
        buffer: None,
    };
    queue.roundtrip(&mut app)?;

    let Some(dmabuf) = app.dmabuf.clone() else {
        eprintln!("DMABUF_CLIENT GLOBAL AUSENTE");
        return Err("zwp_linux_dmabuf_v1 ausente no registry".into());
    };
    eprintln!("DMABUF_CLIENT GLOBAL version=5");
    // Caminho moderno: as tranches do feedback são a lista autoritativa de
    // formatos/modifiers que o compositor diz conseguir importar.
    let _feedback = dmabuf.get_default_feedback(&qh, ());
    queue.roundtrip(&mut app)?;
    let compositor = app
        .compositor
        .clone()
        .ok_or("wl_compositor ausente no registry")?;
    let wm_base = app
        .wm_base
        .clone()
        .ok_or("xdg_wm_base ausente no registry")?;

    // Escolhe um formato real da tranche anunciada. Preferimos Linear porque é
    // o único que podemos criar com GBM de forma determinística nesta VM.
    let preferred = [Fourcc::Xrgb8888, Fourcc::Argb8888, Fourcc::Abgr8888];
    let mut chosen: Option<(Fourcc, Modifier)> = None;
    for code in preferred {
        if app
            .formats
            .iter()
            .any(|(f, m)| *f == code as u32 && *m == 0)
        {
            chosen = Some((code, Modifier::Linear));
            break;
        }
    }
    if chosen.is_none() {
        for code in preferred {
            if let Some((_, m)) = app.formats.iter().find(|(f, _)| *f == code as u32) {
                chosen = Some((code, Modifier::from(*m)));
                break;
            }
        }
    }
    let (code, modifier) = chosen.ok_or("nenhum formato ARGB/ABGR/XRGB anunciado")?;
    let modifier_bits: u64 = modifier.into();
    let gbm_format: GbmFormat = code;

    let node = File::options()
        .read(true)
        .write(true)
        .open(RENDER_NODE)
        .map_err(|e| format!("abrir {RENDER_NODE}: {e}"))?;
    let device = GbmDevice::new(node).map_err(|e| format!("GbmDevice::new: {e}"))?;
    let mut bo: smithay::reexports::gbm::BufferObject<()> = device
        .create_buffer_object::<()>(W, H, gbm_format, BufferObjectFlags::RENDERING)
        .map_err(|e| format!("create_buffer_object: {e}"))?;
    let stride = bo.stride();
    let bytes = pixel_bytes(code);
    bo.map_mut(0, 0, W, H, |mapped| {
        let stride = mapped.stride() as usize;
        let data = mapped.buffer_mut();
        for y in 0..H as usize {
            for x in 0..W as usize {
                let i = y * stride + x * 4;
                if i + 4 <= data.len() {
                    data[i..i + 4].copy_from_slice(&bytes);
                }
            }
        }
    })
    .map_err(|e| format!("map_mut: {e}"))?;
    let fd = bo.fd().map_err(|e| format!("bo.fd: {e}"))?;
    eprintln!(
        "DMABUF_CLIENT BUFFER fd={:?} stride={stride} w={W} h={H} format=0x{:08x} modifier=0x{modifier_bits:016x}",
        fd.as_fd(),
        code as u32
    );
    eprintln!(
        "DMABUF_CLIENT COLOR r={} g={} b={}",
        COLOR.0, COLOR.1, COLOR.2
    );

    let params = dmabuf.create_params(&qh, ());
    params.add(
        fd.as_fd(),
        0,
        0,
        stride,
        (modifier_bits >> 32) as u32,
        modifier_bits as u32,
    );
    let buffer = params.create_immed(
        W as i32,
        H as i32,
        code as u32,
        ParamsFlags::empty(),
        &qh,
        (),
    );
    app.fd = Some(fd);
    app.buffer = Some(buffer.clone());

    let surface = compositor.create_surface(&qh, ());
    let xdg = wm_base.get_xdg_surface(&surface, &qh, 0);
    let toplevel = xdg.get_toplevel(&qh, ());
    toplevel.set_title("aero-dmabuf-client".into());
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();

    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(40));
    }
    eprintln!("DMABUF_CLIENT OK");
    Ok(())
}
