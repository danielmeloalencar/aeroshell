//! F1-toplevel-icon-protocol: `xdg_toplevel_icon_v1` sem reter `wl_buffer`.
//!
//! O `XdgToplevelIconManager` do Smithay 0.7.0 registra um hook de destruição no
//! `wl_buffer` (`wayland/xdg_toplevel_icon.rs:360-369`) que devia ser removido
//! quando o ícone morre, mas `ShmBufferUserData::remove_destruction_hook` usa
//! `hook.id != hook_id` (`shm/mod.rs:496`) e remove o hook ERRADO. O resultado:
//! o cliente Qt (kwrite) faz `add_buffer -> set_icon -> icon.destroy() ->
//! wl_buffer.destroy()` e o hook dispara `xdg_toplevel_icon_v1.no_buffer`,
//! matando a conexão — a janela aparece decorada mas vazia.
//!
//! Correção: NÃO usamos o manager do Smithay. Implementamos o global e os
//!Dispatch próprios; no `add_buffer` copiamos os pixels para um `Vec<u8>` e
//! guardamos no cached state da superfície. Nenhum hook é registrado, nenhum
//! `wl_buffer` é retido. `no_buffer` é emitido somente se a cópia falhar no
//! `add_buffer` (buffer morreu antes de ser lido), como manda o protocolo.

use std::sync::{Arc, Mutex};

use smithay::{
    reexports::wayland_protocols::xdg::toplevel_icon::v1::server::{
        xdg_toplevel_icon_manager_v1::{self, XdgToplevelIconManagerV1},
        xdg_toplevel_icon_v1::{self, XdgToplevelIconV1},
    },
    reexports::wayland_server::{
        backend::GlobalId,
        protocol::{wl_buffer::WlBuffer, wl_shm::Format},
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
    },
    wayland::{
        compositor::{self, Cacheable},
        shm::with_buffer_contents,
    },
};

use crate::state::AeroState;

/// Um buffer de ícone já copiado: (largura, altura, escala, formato, pixels BGRA).
pub type CopiedIconBuffer = (i32, i32, i32, &'static str, Vec<u8>);

/// Pixels copiados de um ícone (nada de `WlBuffer` retido).
#[derive(Debug, Clone, Default)]
pub struct IconPixels {
    /// Maior buffer copiado.
    pub biggest: Option<CopiedIconBuffer>,
    pub name: Option<String>,
}

impl IconPixels {
    pub fn from_buffers_and_name(buffers: &[CopiedIconBuffer], name: Option<&str>) -> Self {
        let biggest = buffers.iter().cloned().max_by_key(|(w, h, _, _, _)| w * h);
        Self {
            biggest,
            name: name.map(str::to_owned),
        }
    }
}

impl Cacheable for IconPixels {
    fn commit(&mut self, _dh: &DisplayHandle) -> Self {
        self.clone()
    }
    fn merge_into(self, into: &mut Self, _dh: &DisplayHandle) {
        *into = self;
    }
}

/// Estado global do manager: tamanhos de ícones anunciados.
#[derive(Debug, Default)]
pub struct IconGlobalData {
    pub sizes: Arc<Mutex<Vec<i32>>>,
}

/// Userdata do `XdgToplevelIconV1`: pixels já copiados + nome.
#[derive(Debug, Default)]
pub struct IconUserData {
    buffers: Mutex<Vec<CopiedIconBuffer>>,
    name: Mutex<Option<String>>,
    frozen: Mutex<bool>,
}

impl IconUserData {
    fn is_immutable(&self) -> bool {
        *self.frozen.lock().unwrap()
    }
    fn freeze(&self) {
        *self.frozen.lock().unwrap() = true;
    }

    /// Copia os pixels de um `wl_buffer` SHM agora, enquanto o closure é válido.
    /// Nenhuma referência ao mmap sobrevive: o buffer pode ser destruído pelo
    /// cliente logo depois do `set_icon` sem disparar nada.
    fn add_buffer(&self, buffer: &WlBuffer, scale: i32) -> Result<(), ()> {
        let copied = with_buffer_contents(buffer, |ptr, len, info| {
            let (w, h, stride) = (info.width, info.height, info.stride);
            let format = match info.format {
                Format::Argb8888 => "Argb8888",
                Format::Xrgb8888 => "Xrgb8888",
                _ => return None,
            };
            if w <= 0 || h <= 0 || stride < w * 4 {
                return None;
            }
            let row = w as usize * 4;
            if stride as usize * h as usize > len {
                return None;
            }
            let mut pixels = vec![0u8; row * h as usize];
            // SAFETY: `ptr` é válido enquanto este closure executa (documentado
            // em smithay/wayland/shm/mod.rs:219-239). Copiamos linha a linha
            // dentro do stride; `row*h` foi validado contra `len`. Nenhuma
            // referência ao SHM sobrevive ao closure.
            unsafe {
                for y in 0..h as usize {
                    std::ptr::copy_nonoverlapping(
                        ptr.add(y * stride as usize),
                        pixels[y * row..].as_mut_ptr(),
                        row,
                    );
                }
            }
            Some((w, h, scale, format, pixels))
        })
        .ok()
        .flatten();

        match copied {
            Some(p) => {
                self.buffers.lock().unwrap().push(p);
                Ok(())
            }
            None => Err(()), // buffer morreu antes de ser lido ou formato inválido
        }
    }

    fn snapshot(&self) -> IconPixels {
        IconPixels::from_buffers_and_name(
            &self.buffers.lock().unwrap(),
            self.name.lock().unwrap().as_deref(),
        )
    }
}

/// Delegate do global: anuncia o manager e os tamanhos de ícone.
#[derive(Debug)]
pub struct IconManager {
    global: GlobalId,
    sizes: Arc<Mutex<Vec<i32>>>,
}

impl IconManager {
    pub fn new(display: &DisplayHandle) -> Self {
        let sizes = Arc::new(Mutex::new(Vec::new()));
        let global = display.create_global::<AeroState, XdgToplevelIconManagerV1, _>(
            1,
            IconGlobalData {
                sizes: sizes.clone(),
            },
        );
        Self { global, sizes }
    }

    pub fn add_icon_size(&self, size: i32) {
        self.sizes.lock().unwrap().push(size);
    }

    #[allow(dead_code)] // simetria com a API do Smithay; útil para diagnóstico futuro
    pub fn global(&self) -> GlobalId {
        self.global.clone()
    }
}

impl GlobalDispatch<XdgToplevelIconManagerV1, IconGlobalData, AeroState> for IconManager {
    fn bind(
        _state: &mut AeroState,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<XdgToplevelIconManagerV1>,
        data: &IconGlobalData,
        data_init: &mut DataInit<'_, AeroState>,
    ) {
        let manager = data_init.init(resource, ());
        for size in data.sizes.lock().unwrap().iter() {
            manager.icon_size(*size);
        }
        manager.done();
    }
}

impl Dispatch<XdgToplevelIconManagerV1, (), AeroState> for IconManager {
    fn request(
        state: &mut AeroState,
        _client: &Client,
        _resource: &XdgToplevelIconManagerV1,
        request: xdg_toplevel_icon_manager_v1::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, AeroState>,
    ) {
        match request {
            xdg_toplevel_icon_manager_v1::Request::CreateIcon { id } => {
                data_init.init(id, IconUserData::default());
            }
            xdg_toplevel_icon_manager_v1::Request::SetIcon { toplevel, icon } => {
                // XdgShellSurfaceUserData.wl_surface é pub(crate) no Smithay;
                // achamos a WlSurface pelo XdgToplevel via space.elements().
                let surface = state.space.elements().find_map(|w| {
                    w.toplevel()
                        .filter(|t| t.xdg_toplevel() == &toplevel)
                        .map(|t| t.wl_surface().clone())
                });
                let Some(surface) = surface else {
                    return;
                };
                let pixels = match &icon {
                    Some(icon) => {
                        let Some(data) = icon.data::<IconUserData>() else {
                            return;
                        };
                        data.freeze();
                        data.snapshot()
                    }
                    None => IconPixels::default(),
                };
                #[cfg(feature = "bench")]
                {
                    let t = crate::frame_diagnostics::now_ns();
                    let id = state
                        .window_surfaces
                        .iter()
                        .find(|(_, s)| **s == surface)
                        .map(|(id, _)| id.to_string())
                        .unwrap_or_else(|| "?".to_owned());
                    match &pixels.biggest {
                        Some((w, h, scale, _, _)) => {
                            eprintln!("ICON_SET t={t} window_id={id} w={w} h={h} scale={scale}")
                        }
                        None => eprintln!(
                            "ICON_SET t={t} window_id={id} w=? h=? scale=? name={}",
                            pixels.name.as_deref().unwrap_or("?")
                        ),
                    }
                }
                compositor::with_states(&surface, |states| {
                    let mut cached = states.cached_state.get::<IconPixels>();
                    *cached.pending() = pixels;
                });
                let _ = state;
            }
            xdg_toplevel_icon_manager_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
    }
}

impl Dispatch<XdgToplevelIconV1, IconUserData, AeroState> for IconManager {
    fn request(
        _state: &mut AeroState,
        _client: &Client,
        icon: &XdgToplevelIconV1,
        request: xdg_toplevel_icon_v1::Request,
        data: &IconUserData,
        dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, AeroState>,
    ) {
        match request {
            xdg_toplevel_icon_v1::Request::SetName { icon_name } => {
                if data.is_immutable() {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::Immutable,
                        "set_name após set_icon",
                    );
                    return;
                }
                *data.name.lock().unwrap() = Some(icon_name);
            }
            xdg_toplevel_icon_v1::Request::AddBuffer { buffer, scale } => {
                if data.is_immutable() {
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::Immutable,
                        "add_buffer após set_icon",
                    );
                    return;
                }
                if data.add_buffer(&buffer, scale).is_err() {
                    // Único caso legítimo de `no_buffer`: o buffer morreu antes
                    // de termos copiado os pixels. Não registramos hooks, então
                    // um destroy DEPOIS da cópia nunca mais dispara isto.
                    icon.post_error(
                        xdg_toplevel_icon_v1::Error::NoBuffer,
                        "The provided buffer has been destroyed before the toplevel icon",
                    );
                }
            }
            xdg_toplevel_icon_v1::Request::Destroy => {}
            _ => unreachable!(),
        }
        let _ = dh;
    }
}

// O `create_global` / `data_init.init` exigem que o STATE implemente os traits;
// as macros encaminham para os nossos impls parametrizados em `AeroState`.
smithay::reexports::wayland_server::delegate_global_dispatch!(AeroState: [
    XdgToplevelIconManagerV1: IconGlobalData
] => IconManager);
smithay::reexports::wayland_server::delegate_dispatch!(AeroState: [
    XdgToplevelIconManagerV1: ()
] => IconManager);
smithay::reexports::wayland_server::delegate_dispatch!(AeroState: [
    XdgToplevelIconV1: IconUserData
] => IconManager);
