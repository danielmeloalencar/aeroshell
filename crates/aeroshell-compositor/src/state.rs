//! Estado global do compositor. Não conhece o backend concreto.

use std::{collections::HashMap, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{EventLoop, LoopSignal},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle, Resource,
        },
    },
    utils::{Logical, Point},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, XdgShellState},
        },
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

use crate::{cli::Options, CalloopData};

/// Geometria declarada pelo cliente para um frame (ADR 0004): insets movem o
/// conteúdo; shadow_extents são apenas visuais e não movem o conteúdo.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameGeometry {
    /// left, top, right, bottom
    pub insets: (i32, i32, i32, i32),
    /// left, top, right, bottom
    pub shadow: (i32, i32, i32, i32),
}

impl FrameGeometry {
    pub fn extra_w(&self) -> i32 {
        self.insets.0 + self.insets.2 + self.shadow.0 + self.shadow.2
    }
    pub fn extra_h(&self) -> i32 {
        self.insets.1 + self.insets.3 + self.shadow.1 + self.shadow.3
    }
    /// Deslocamento do frame em relação à origem do cliente.
    pub fn offset(&self) -> (i32, i32) {
        (
            -(self.insets.0 + self.shadow.0),
            -(self.insets.1 + self.shadow.1),
        )
    }
}

/// Estado de um configure pendente, usado para as métricas do protótipo.
#[cfg(feature = "bench")]
#[derive(Debug, Clone)]
pub struct FrameProbe {
    pub surface: WlSurface,
    pub window_id: u32,
    pub serial: u32,
    pub configured_at: Instant,
    pub size: (u32, u32),
    pub acked: bool,
    pub committed: bool,
    pub timeout_logged: bool,
}

pub struct AeroState {
    pub start_time: Instant,
    /// Nome do socket próprio; nunca sobrescreve o WAYLAND_DISPLAY do host.
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,
    pub space: Space<Window>,
    pub loop_signal: LoopSignal,
    pub outputs: Vec<Output>,
    /// Layer surfaces já embrulhadas no tipo de desktop, prontas para o `LayerMap`.
    pub layer_surfaces: Vec<smithay::desktop::LayerSurface>,
    /// Resultado do hit-test de input region: usado pelo backend para escolher o cursor.
    pub cursor_over_interactive: bool,
    /// Frames do protocolo privado (S5).
    pub frames: Vec<(crate::aero_frame_protocol::aero_frame_v1::AeroFrameV1, u32)>,
    /// Associações frame ↔ superfície. Marca SEM role: layer_surface é suportado;
    /// xdg_toplevel recebe `closed` e fica deferido (ADR 0004).
    pub associated_frames: Vec<(
        WlSurface,
        crate::aero_frame_protocol::aero_frame_v1::AeroFrameV1,
        u32,
        Option<WlSurface>,
    )>,
    /// Geometria declarada por cada frame (indexada pela superfície do frame).
    pub frame_geometries: HashMap<WlSurface, FrameGeometry>,
    /// Último tamanho de CONTEÚDO enviado em aero_frame.configure por frame.
    pub frame_configured_content: HashMap<WlSurface, (i32, i32)>,
    /// Serial monotônico dos aero_frame.configure (não reutiliza serial ackado).
    pub frame_configure_serial: u32,
    /// Layer surfaces que são frames: NÃO entram no LayerMap (exceção do ADR 0004).
    pub frame_layer_surfaces: Vec<smithay::desktop::LayerSurface>,
    /// Configurações enviadas e ainda não confirmadas (serial, instante).
    pub pending_configures: Vec<(u32, std::time::Instant)>,
    /// Sondas de configure: medem configure → commit e descompasso por passo.
    #[cfg(feature = "bench")]
    pub frame_probes: Vec<FrameProbe>,
    /// Amostras de configure → commit (serial, ms) para agregação no fim.
    #[cfg(feature = "bench")]
    pub frame_commit_samples: Vec<(u32, f64)>,
    /// Último serial de botão recebido (validação de move/resize).
    pub last_button_serial: Option<u32>,

    /// Última ação de geometry para medir latência até o frame atualizado.
    pub last_geometry_action: Option<(String, std::time::Instant)>,
    /// Última ordem de render (frente->trás) logada; evita spam a cada redraw.
    pub last_render_order: Vec<String>,
    /// Permite testes de protocolo validarem IDs sem criar uma janela xdg real.
    pub test_window_count: usize,
    /// Cliente inicial adiado até o frame_host associar (cold-start temporário).
    /// Consumido no primeiro associate; `None` depois disso ou sem `--shell`.
    pub pending_initial_command: Option<String>,
    /// Lista de clientes inseridos (usada só para sinalizar prontidão em testes).
    #[allow(dead_code)]
    pub test_client_count: usize,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub xdg_decoration_state: XdgDecorationState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<AeroState>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,
    pub seat: Seat<Self>,
}

impl AeroState {
    pub fn new(
        event_loop: &mut EventLoop<CalloopData>,
        display: Display<Self>,
        opts: &Options,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let xdg_decoration_state = XdgDecorationState::new::<Self>(&dh);
        let layer_shell_state = WlrLayerShellState::new::<Self>(&dh);
        let _aero_frame_global = dh.create_global::<
            Self,
            crate::aero_frame_protocol::aero_frame_manager_v1::AeroFrameManagerV1,
            (),
        >(1, ());
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "aeroshell");
        seat.add_keyboard(Default::default(), 200, 25)?;
        seat.add_pointer();

        let socket = ListeningSocketSource::with_name(&opts.socket)?;
        let socket_name = socket.socket_name().to_os_string();
        let loop_handle = event_loop.handle();
        loop_handle.insert_source(socket, move |client_stream, _, data| {
            data.state.test_client_count += 1;
            tracing::info!("cliente Wayland conectado ao socket público do AeroShell");
            let _client = data
                .display_handle
                .insert_client(client_stream, Arc::new(ClientState::default()))
                .expect("failed to insert Wayland client");
        })?;

        // O Display é movido para dentro do event source (padrão canônico do
        // anvil/Smithay). Assim não há `Box::leak`, nem ponteiro bruto: o source
        // é dono do Display e o event loop o despacha a cada iteração.
        loop_handle.insert_source(
            smithay::reexports::calloop::generic::Generic::new(
                display,
                smithay::reexports::calloop::Interest::READ,
                smithay::reexports::calloop::Mode::Level,
            ),
            |_, display, data| {
                // SAFETY: `display` é owned pelo Generic source e permanece vivo
                // enquanto o event loop existe. O callback é chamado com acesso
                // exclusivo ao source, então `get_mut` é seguro aqui.
                unsafe { display.get_mut() }.dispatch_clients(&mut data.state)?;
                Ok(smithay::reexports::calloop::PostAction::Continue)
            },
        )?;

        Ok(Self {
            start_time: Instant::now(),
            socket_name,
            display_handle: dh,
            space: Space::default(),
            loop_signal: event_loop.get_signal(),
            outputs: Vec::new(),
            layer_surfaces: Vec::new(),
            cursor_over_interactive: false,
            frames: Vec::new(),
            associated_frames: Vec::new(),
            frame_layer_surfaces: Vec::new(),
            pending_configures: Vec::new(),
            #[cfg(feature = "bench")]
            frame_probes: Vec::new(),
            #[cfg(feature = "bench")]
            frame_commit_samples: Vec::new(),
            last_button_serial: None,
            frame_geometries: HashMap::new(),
            frame_configured_content: HashMap::new(),
            frame_configure_serial: 0,
            last_geometry_action: None,
            last_render_order: Vec::new(),
            test_window_count: 0,
            pending_initial_command: None,
            test_client_count: 0,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups: PopupManager::default(),
            seat,
        })
    }

    pub fn is_interactive_layer_surface(&self, surface: &WlSurface) -> bool {
        self.layer_surfaces
            .iter()
            .any(|layer| layer.wl_surface() == surface)
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        // Layer surfaces (painéis/overlay) vêm antes das janelas normais; `surface_under`
        // do Smithay respeita `wl_surface.set_input_region`.
        for output in &self.outputs {
            let map = smithay::desktop::layer_map_for_output(output);
            // `layer_under` usa o tamanho bruto do cliente (que pode ser 0 quando ancorado
            // nas duas bordas). Usamos a geometry calculada pelo LayerMap.
            for layer_surface in map.layers().rev() {
                let Some(geo) = map.layer_geometry(layer_surface) else {
                    continue;
                };
                if !geo.to_f64().contains(pos) {
                    continue;
                }
                let origin = geo.loc.to_f64();
                let hit = layer_surface.surface_under(pos - origin, WindowSurfaceType::ALL);
                tracing::debug!(?pos, ?origin, hit = ?hit.as_ref().map(|(s, _)| s.id()), "hit-test layer");
                if let Some((surface, local)) = hit {
                    return Some((surface, (local.to_f64() + origin)));
                }
            }
        }
        if let Some((window, location)) = self.space.element_under(pos) {
            if let Some((s, p)) =
                window.surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
            {
                tracing::debug!(?pos, hit_client = ?s.id(), "hit-test cliente (antes do frame)");
                return Some((s, (p + location).to_f64()));
            }
        }
        // Frames: a região de decoração (total menos o conteúdo do cliente)
        // roteia para a superfície do frame. O conteúdo já foi tratado acima.
        for (frame_surface, _, _, client_surface) in &self.associated_frames {
            let Some(cs) = client_surface else { continue };
            let Some(window) = self
                .space
                .elements()
                .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(cs))
            else {
                continue;
            };
            let Some(geo) = self.space.element_geometry(window) else {
                continue;
            };
            let fgeo = self
                .frame_geometries
                .get(frame_surface)
                .copied()
                .unwrap_or_default();
            let content = self
                .frame_configured_content
                .get(frame_surface)
                .copied()
                .unwrap_or((geo.size.w, geo.size.h));
            let (dx, dy) = fgeo.offset();
            let frame_loc = Point::new(geo.loc.x + dx, geo.loc.y + dy);
            let frame_rect = smithay::utils::Rectangle::new(
                frame_loc,
                (content.0 + fgeo.extra_w(), content.1 + fgeo.extra_h()).into(),
            );
            if !frame_rect.to_f64().contains(pos) {
                continue;
            }
            let content_rect = smithay::utils::Rectangle::new(geo.loc, geo.size);
            if content_rect.to_f64().contains(pos) {
                continue;
            }
            // Smithay (input/pointer/mod.rs:795) calcula o local do cliente como
            // `event.location - loc`: o ponto devolvido aqui é a ORIGEM GLOBAL da
            // superfície (não o hit local), para que enter/motion cheguem certos.
            tracing::debug!(?pos, ?frame_rect, ?content_rect, hit_frame = ?frame_surface.id(), "hit-test frame (região de decoração)");
            return Some((frame_surface.clone(), frame_loc.to_f64()));
        }
        None
    }

    /// Agrega média e p95 de configure → commit ao encerrar.
    #[cfg(feature = "bench")]
    pub fn log_frame_summary(&self) {
        if self.frame_commit_samples.is_empty() {
            return;
        }
        let mut values: Vec<f64> = self
            .frame_commit_samples
            .iter()
            .map(|(_, ms)| *ms)
            .collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let p95 = crate::frame_metrics::percentile(&mut values, 0.95);
        eprintln!(
            "FRAME_SUMMARY samples={} mean_ms={mean:.3} p95_ms={p95:.3}",
            values.len()
        );
    }

    /// Chamado no idle do event loop: registra uma vez cada `ack_configure`
    /// que estourou o timeout de 100 ms (métrica iii).
    #[cfg(feature = "bench")]
    pub fn check_frame_timeouts(&mut self) {
        for probe in self.frame_probes.iter_mut() {
            if !probe.acked
                && !probe.timeout_logged
                && probe.configured_at.elapsed().as_millis() >= 100
            {
                probe.timeout_logged = true;
                eprintln!(
                    "FRAME_TIMEOUT serial={} window_id={} waited_ms={}",
                    probe.serial,
                    probe.window_id,
                    probe.configured_at.elapsed().as_millis()
                );
            }
        }
    }
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    /// Marca do supervisor: cliente conectado pela ponta privada do socketpair.
    pub is_shell: bool,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, reason: DisconnectReason) {
        if self.is_shell {
            tracing::warn!(?reason, "conexão privilegiada do shell caiu");
        }
    }
}
