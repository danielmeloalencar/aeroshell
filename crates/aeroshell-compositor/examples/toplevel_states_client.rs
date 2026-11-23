//! F1-xdg-toplevel-states: cliente sintético que exercita fullscreen, maximize,
//! set_parent e min/max por comandos recebidos em stdin (uma linha por comando).
//! Sem dependência nova: wayland-client + bindings reexportados pelo Smithay.
//!
//! Comandos: fullscreen, unfullscreen, maximize, unmaximize, parent,
//! destroy_parent, quit. Logs com prefixo XDG_CLIENT são usados pelo harness
//! `tests/integration/f1_toplevel_states.py`.

use std::{
    fs::OpenOptions,
    io::{BufRead, Write},
    os::fd::AsFd,
    sync::mpsc,
    time::{Duration, Instant},
};

use smithay::reexports::wayland_protocols::xdg::shell::client::{
    xdg_surface::{Event as XdgSurfaceEvent, XdgSurface},
    xdg_toplevel::{Event as ToplevelEvent, State as ToplevelState, XdgToplevel},
    xdg_wm_base::{Event as WmBaseEvent, XdgWmBase},
};
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_buffer::WlBuffer, wl_compositor::WlCompositor, wl_registry::Event as RegistryEvent,
        wl_registry::WlRegistry, wl_shm::Format, wl_shm::WlShm, wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, QueueHandle,
};

struct Win {
    surface: WlSurface,
    toplevel: XdgToplevel,
    buffer: WlBuffer,
    buffer_size: (i32, i32),
    child: bool,
}

struct App {
    compositor: Option<WlCompositor>,
    wm_base: Option<XdgWmBase>,
    shm: Option<WlShm>,
    windows: Vec<Win>,
    serial: u32,
    quit: bool,
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
            "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(6), qh, ())),
            "xdg_wm_base" => state.wm_base = Some(registry.bind(name, version.min(3), qh, ())),
            "wl_shm" => state.shm = Some(registry.bind(name, version.min(2), qh, ())),
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

impl Dispatch<XdgSurface, usize> for App {
    fn event(
        state: &mut App,
        xdg: &XdgSurface,
        event: XdgSurfaceEvent,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let XdgSurfaceEvent::Configure { serial } = event {
            xdg.ack_configure(serial);
            state.serial = serial;
            eprintln!("XDG_CLIENT XDG_SURFACE_CONFIGURE win={index} serial={serial}");
        }
    }
}

impl Dispatch<XdgToplevel, usize> for App {
    fn event(
        state: &mut App,
        _: &XdgToplevel,
        event: ToplevelEvent,
        index: &usize,
        _: &Connection,
        qh: &QueueHandle<App>,
    ) {
        if let ToplevelEvent::Configure {
            width,
            height,
            states,
        } = event
        {
            // Cliente real: responde ao configure com um buffer do tamanho pedido.
            if width > 0 && height > 0 {
                let _ = state.resize_buffer(*index, width, height, qh);
            }
            let flags: Vec<&str> = states
                .iter()
                .filter_map(|s| match ToplevelState::try_from(*s as u32) {
                    Ok(ToplevelState::Fullscreen) => Some("fullscreen"),
                    Ok(ToplevelState::Maximized) => Some("maximized"),
                    Ok(ToplevelState::Activated) => Some("activated"),
                    _ => None,
                })
                .collect();
            eprintln!(
                "XDG_CLIENT CONFIGURE win={index} w={width} h={height} states={}",
                flags.join(",")
            );
        }
    }
}

delegate_noop!(App: ignore WlCompositor);
delegate_noop!(App: ignore WlSurface);
delegate_noop!(App: ignore WlShm);
delegate_noop!(App: ignore WlShmPool);
delegate_noop!(App: ignore WlBuffer);

impl App {
    fn resize_buffer(
        &mut self,
        index: usize,
        width: i32,
        height: i32,
        qh: &QueueHandle<App>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(win) = self.windows.get_mut(index) else {
            return Ok(());
        };
        if win.buffer_size == (width, height) {
            return Ok(());
        }
        let shm = self.shm.clone().ok_or("sem wl_shm")?;
        let stride = width * 4;
        let path = std::env::temp_dir().join(format!(
            "aero-states-buf-{}-{}-{width}x{height}",
            std::process::id(),
            index
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len((stride * height) as u64)?;
        file.write_all(&vec![50u8; (stride * height) as usize])?;
        let pool = shm.create_pool(file.as_fd(), stride * height, qh, ());
        let buffer = pool.create_buffer(0, width, height, stride, Format::Argb8888, qh, ());
        win.surface.attach(Some(&buffer), 0, 0);
        win.surface.commit();
        win.buffer = buffer;
        win.buffer_size = (width, height);
        eprintln!("XDG_CLIENT RESIZED win={index} w={width} h={height}");
        Ok(())
    }

    fn create_window(
        &mut self,
        qh: &QueueHandle<App>,
        child: bool,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let compositor = self.compositor.clone().ok_or("sem wl_compositor")?;
        let wm_base = self.wm_base.clone().ok_or("sem xdg_wm_base")?;
        let shm = self.shm.clone().ok_or("sem wl_shm")?;
        let index = self.windows.len();
        let surface = compositor.create_surface(qh, ());
        let xdg = wm_base.get_xdg_surface(&surface, qh, index);
        let toplevel = xdg.get_toplevel(qh, index);
        toplevel.set_title(format!("aero-states-{index}"));
        if !child {
            toplevel.set_min_size(400, 300);
            toplevel.set_max_size(900, 700);
        }
        if child {
            if let Some(parent) = self.windows.first() {
                toplevel.set_parent(Some(&parent.toplevel));
                eprintln!("XDG_CLIENT PARENT_SET child={index} parent=0");
            }
        }
        let stride = 200 * 4;
        let path =
            std::env::temp_dir().join(format!("aero-states-buf-{}-{index}", std::process::id()));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)?;
        file.set_len((stride * 100) as u64)?;
        file.write_all(&vec![50u8; (stride * 100) as usize])?;
        let pool = shm.create_pool(file.as_fd(), stride * 100, qh, ());
        let buffer = pool.create_buffer(0, 200, 100, stride, Format::Argb8888, qh, ());
        surface.attach(Some(&buffer), 0, 0);
        surface.commit();
        self.windows.push(Win {
            surface,
            toplevel,
            buffer,
            buffer_size: (200, 100),
            child,
        });
        eprintln!("XDG_CLIENT WINDOW_CREATED win={index} child={child}");
        Ok(index)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let mut app = App {
        compositor: None,
        wm_base: None,
        shm: None,
        windows: Vec::new(),
        serial: 0,
        quit: false,
    };
    queue.roundtrip(&mut app)?;
    app.create_window(&qh, false)?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline && !app.quit {
        while let Ok(cmd) = rx.try_recv() {
            let cmd = cmd.trim();
            eprintln!("XDG_CLIENT COMMAND {cmd}");
            match cmd {
                "fullscreen" => {
                    if let Some(w) = app.windows.first() {
                        w.toplevel.set_fullscreen(None);
                    }
                }
                "unfullscreen" => {
                    if let Some(w) = app.windows.first() {
                        w.toplevel.unset_fullscreen();
                    }
                }
                "maximize" => {
                    if let Some(w) = app.windows.first() {
                        w.toplevel.set_maximized();
                    }
                }
                "unmaximize" => {
                    if let Some(w) = app.windows.first() {
                        w.toplevel.unset_maximized();
                    }
                }
                "parent" => {
                    let _ = app.create_window(&qh, true);
                }
                "destroy_parent" => {
                    if app.windows.len() > 1 {
                        let child = app.windows.remove(1);
                        let _ = child.buffer;
                        eprintln!("XDG_CLIENT CHILD_DESTROYED index=1 child={}", child.child);
                    }
                }
                "quit" => app.quit = true,
                _ => {}
            }
        }
        queue.roundtrip(&mut app)?;
        std::thread::sleep(Duration::from_millis(25));
    }
    eprintln!("XDG_CLIENT OK");
    Ok(())
}
