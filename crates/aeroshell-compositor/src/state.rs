//! Estado global do compositor. Não conhece o backend concreto.

use std::{collections::HashMap, ffi::OsString, sync::Arc, time::Instant};

use smithay::{
    desktop::{PopupManager, Space, Window, WindowSurfaceType},
    input::{pointer::CursorImageStatus, Seat, SeatState},
    output::Output,
    reexports::{
        calloop::{EventLoop, LoopSignal},
        wayland_server::{
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
            Display, DisplayHandle, Resource,
        },
    },
    utils::{Logical, Point, Rectangle, Size},
    wayland::{
        compositor::{with_states, CompositorClientState, CompositorState},
        cursor_shape::CursorShapeManagerState,
        fractional_scale::{with_fractional_scale, FractionalScaleManagerState},
        input_method::InputMethodManagerState,
        output::OutputManagerState,
        selection::{data_device::DataDeviceState, primary_selection::PrimarySelectionState},
        shell::{
            wlr_layer::WlrLayerShellState,
            xdg::{decoration::XdgDecorationState, XdgShellState},
        },
        shm::ShmState,
        socket::ListeningSocketSource,
        text_input::TextInputManagerState,
        viewporter::ViewporterState,
        xdg_activation::XdgActivationState,
        xdg_toplevel_icon::XdgToplevelIconManager,
    },
};

use crate::{cli::Options, render::cursor::CursorImage, CalloopData};

/// Geometria salva de uma janela antes do maximize (02r): origem no Space e
/// tamanho do cliente.
pub type SavedGeometry = (Point<i32, Logical>, Size<i32, Logical>);

/// Crescimento excepcional do canvas, confirmado pelo paint do shell.
#[derive(Debug, Clone, Copy)]
pub struct CanvasGrowth {
    pub serial: u32,
    pub size: (i32, i32),
}

/// O canvas é independente do target apresentado. Seu tamanho só muda quando
/// o target não cabe mais e o shell confirma o paint daquele configure.
#[derive(Debug, Clone, Copy)]
pub struct FrameCanvas {
    pub size: (i32, i32),
    pub pending_growth: Option<CanvasGrowth>,
}

impl FrameCanvas {
    pub fn configured_size(&self) -> (i32, i32) {
        self.pending_growth
            .map(|growth| growth.size)
            .unwrap_or(self.size)
    }
}

/// Geometria declarada pelo cliente para um frame (ADR 0004): insets movem o
/// conteúdo; shadow_extents são apenas visuais e não movem o conteúdo.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameGeometry {
    /// left, top, right, bottom
    pub insets: (i32, i32, i32, i32),
    /// left, top, right, bottom
    pub shadow: (i32, i32, i32, i32),
    /// 02l: larguras (esquerda, direita) das zonas da titlebar que o 9-slice
    /// NÃO estica em X. Declaradas pelo shell via `set_titlebar_slices`;
    /// (0, 0) mantém o comportamento anterior da 02k.
    pub titlebar_slices: (i32, i32),
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
    /// Manager vinculado pelo shell: permite anunciar `frame_requested` quando uma
    /// janela nova mapeia sem frame (fonte do window_id no shell_privilegiado).
    pub shell_manager:
        Option<crate::aero_frame_protocol::aero_frame_manager_v1::AeroFrameManagerV1>,
    /// window_id lógico -> superfície do cliente. Estável mesmo quando o Space é
    /// reordenado: `elements()` é ordem z e muda a cada `raise_element`, então
    /// `nth(window_id)` apontaria para a janela errada após um clique.
    pub window_surfaces: HashMap<u32, WlSurface>,
    /// Correção F1: IDs monotônicos, NUNCA reutilizados. O `count()` do Space
    /// voltava a 0 ao fechar a janela e o novo `foot` recebia o mesmo id, fazendo
    /// o shell ignorar o `frame_requested` como "já apresentado".
    pub next_window_id: u32,
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
    /// 02j: diagnósticos nomeados (eventos e linhas de log só sob `bench`).
    #[cfg(feature = "bench")]
    pub frame_diag: crate::frame_diagnostics::FrameDiagnostics,
    /// Último serial de botão recebido (validação de move/resize).
    pub last_button_serial: Option<u32>,
    /// window_id sob resize ativo. O commit-hook não reconfigura a decoração
    /// enquanto o grab decide o tamanho: reenviar o tamanho atrasado do cliente
    /// "puxa" a moldura para trás e gera um storm de configures (02f).
    pub resize_grab_active: Option<u32>,
    /// Canvas fixo de cada frame; o target continua sendo conteúdo + extras.
    pub frame_canvases: HashMap<WlSurface, FrameCanvas>,

    /// Última ação de geometry para medir latência até o frame atualizado.
    pub last_geometry_action: Option<(String, std::time::Instant)>,
    /// Última ordem de render (frente->trás) logada; evita spam a cada redraw.
    pub last_render_order: Vec<String>,
    /// Permite testes de protocolo validarem IDs sem criar uma janela xdg real.
    pub test_window_count: usize,
    /// Tamanho SUGERIDO no configure inicial de cada toplevel (800×600 padrão
    /// lógico). Serve de geometria provisória ao placeholder enquanto o cliente
    /// não comita o conteúdo real — sem isso a janela mapeia 0×0 e só aparece a
    /// moldura das insets (12×36). Política padrão de sizing do compositor, não
    /// constante de tema nem decisão de privilégio.
    pub suggested_window_size: HashMap<u32, Size<i32, Logical>>,
    /// Geometria da decoração anunciada pelo shell no startup via
    /// `set_default_insets`. Usada pelo placeholder do compositor para janelas
    /// mapeadas que ainda não têm WebView associado; `set_insets` do frame
    /// sobrescreve quando ele associa. (0,0,0,0) até o shell declarar.
    pub default_frame_insets: (i32, i32, i32, i32),
    /// 02r: geometria (origem no Space + tamanho do cliente) guardada no maximize
    /// para o desmaximizar devolver a janela exatamente ao lugar de origem.
    pub maximized_restore: HashMap<u32, SavedGeometry>,
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
    /// F1-primary-selection: global `wp-primary-selection-v1`.
    pub primary_selection_state: PrimarySelectionState,
    /// F1f: global `wp_fractional_scale_v1`.
    pub fractional_scale_state: FractionalScaleManagerState,
    /// F1f/E: global `wp_viewporter` (necessário para escala fracionária).
    pub viewporter_state: ViewporterState,
    /// F1-cursor-shape: global `wp_cursor_shape_v1` e última forma pedida pelo cliente.
    pub cursor_shape_state: CursorShapeManagerState,
    /// F1i: global `zwp_text_input_manager_v3` (foco acompanha o teclado).
    pub text_input_state: TextInputManagerState,
    /// F1i: global `zwp_input_method_manager_v2`.
    pub input_method_state: InputMethodManagerState,
    /// F1i/F7: global `xdg_activation_v1` (token + pedido de ativação).
    pub xdg_activation_state: XdgActivationState,
    /// F1-toplevel-icon: global `xdg_toplevel_icon_v1` (ícone por toplevel).
    pub toplevel_icon_manager: XdgToplevelIconManager,
    pub cursor_image: CursorImageStatus,
    /// Imagens do tema XCursor por nome de forma, com o hotspot lido do arquivo.
    pub cursor_images: HashMap<String, Option<CursorImage>>,
    pub popups: PopupManager,
    pub seat: Seat<Self>,
}

impl AeroState {
    /// F1f: quando a escala do output muda, reenviar `preferred_scale` e
    /// reconfigurar os toplevels — modelo do domicile (`outputs.rs:224-233`).
    pub fn resend_scales(&mut self) {
        let Some(scale) = self
            .outputs
            .first()
            .map(|output| output.current_scale().fractional_scale())
        else {
            return;
        };
        let surfaces: Vec<_> = self.window_surfaces.values().cloned().collect();
        for surface in surfaces {
            with_states(&surface, |states| {
                with_fractional_scale(states, |fs| fs.set_preferred_scale(scale));
            });
        }
        let windows: Vec<_> = self.space.elements().cloned().collect();
        for window in windows {
            if let Some(toplevel) = window.toplevel() {
                let _ = toplevel.send_configure();
            }
        }
    }

    /// Resolve um window_id (atribuído no map) para a Window correspondente.
    /// NUNCA usar `space.elements().nth(window_id)`: `elements()` é ordem z e muda
    /// a cada `raise_element` (input.rs `focus_on_press`), fazendo o id 0 apontar
    /// para outra janela depois de um clique.
    pub fn window_for_id(&self, window_id: u32) -> Option<&Window> {
        let surface = self.window_surfaces.get(&window_id)?;
        self.space
            .elements()
            .find(|w| w.toplevel().map(|t| t.wl_surface()) == Some(surface))
    }

    /// 02r: área não exclusiva do primeiro output em coordenadas do Space.
    /// `non_exclusive_zone()` devolve um retângulo em coordenadas lógicas do
    /// output (origem no canto do output); somamos a origem do output no Space
    /// para obter a origem global. Fonte: Smithay 0.7.0
    /// `desktop/wayland/layer.rs:135-138` e `:254-267`.
    pub fn work_area(&self) -> Option<Rectangle<i32, Logical>> {
        let output = self.outputs.first()?;
        let origin = self.space.output_geometry(output)?.loc;
        let zone = smithay::desktop::layer_map_for_output(output).non_exclusive_zone();
        Some(Rectangle::new(
            Point::new(origin.x + zone.loc.x, origin.y + zone.loc.y),
            zone.size,
        ))
    }

    /// 02r: geometria da decoração de uma janela. O frame é layer surface, mas
    /// NÃO entra no LayerMap (exceção do ADR 0004): as insets vêm do `set_insets`
    /// do protocolo privado, não da zona. Sem frame associado, usamos o padrão
    /// anunciado pelo shell no startup.
    pub fn frame_geometry_for_window(&self, window_id: u32) -> FrameGeometry {
        self.associated_frames
            .iter()
            .find(|(_, _, id, _)| *id == window_id)
            .and_then(|(surface, _, _, _)| self.frame_geometries.get(surface).copied())
            .unwrap_or(FrameGeometry {
                insets: self.default_frame_insets,
                ..Default::default()
            })
    }

    /// True se o frame já tem cliente mapeado no space. Nesse caso o frame é
    /// desenhado pelo 9-slice no output do cliente (`push_frame_elements`) e
    /// NÃO deve cair no fallback de origem (0,0). Em N>1 esse fallback desenhava
    /// a layer surface do outro output inteira (canvas >= output + insets),
    /// atravessando a metade da tela — juiz visual F1c.
    pub fn frame_has_mapped_client(&self, frame_surface: &WlSurface) -> bool {
        let Some((_, _, window_id, _)) = self
            .associated_frames
            .iter()
            .find(|(surface, _, _, _)| surface == frame_surface)
        else {
            return false;
        };
        self.window_for_id(*window_id).is_some()
    }

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
        let primary_selection_state = PrimarySelectionState::new::<Self>(&dh);
        let fractional_scale_state = FractionalScaleManagerState::new::<Self>(&dh);
        let viewporter_state = ViewporterState::new::<Self>(&dh);
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(&dh);
        // F1i: o filtro de input-method segue o exemplo do Smithay 0.7.0 fixado
        // (`wayland/input_method/mod.rs:62`): qualquer cliente pode visualizar o
        // global. Não há processo de IME dedicado nesta fase (somente presença do
        // protocolo + digitação normal), então não há uma identidade para filtrar.
        let input_method_state = InputMethodManagerState::new::<Self, _>(&dh, |_client| true);
        let text_input_state = TextInputManagerState::new::<Self>(&dh);
        let xdg_activation_state = XdgActivationState::new::<Self>(&dh);
        let mut toplevel_icon_manager = XdgToplevelIconManager::new::<Self>(&dh);
        // Tamanhos que oferecemos aos clientes (xdg_toplevel_icon.rs:222); o
        // shell consome os pixels via IPC, sem precisar de dependência nova.
        for size in [16, 32, 48, 64] {
            toplevel_icon_manager.add_icon_size(size);
        }

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
            #[cfg(feature = "bench")]
            frame_diag: crate::frame_diagnostics::FrameDiagnostics::with_fault_injection(
                std::env::var("AERO_DIAG_SUPPRESS_ACK_ONCE").is_ok(),
                std::env::var("AERO_DIAG_SUPPRESS_DRAW_ONCE").is_ok(),
            ),
            last_button_serial: None,
            resize_grab_active: None,
            frame_canvases: HashMap::new(),
            shell_manager: None,
            window_surfaces: HashMap::new(),
            next_window_id: 0,
            frame_geometries: HashMap::new(),
            frame_configured_content: HashMap::new(),
            frame_configure_serial: 0,
            last_geometry_action: None,
            last_render_order: Vec::new(),
            test_window_count: 0,
            default_frame_insets: (0, 0, 0, 0),
            maximized_restore: HashMap::new(),
            suggested_window_size: HashMap::new(),
            test_client_count: 0,
            compositor_state,
            xdg_shell_state,
            xdg_decoration_state,
            layer_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            primary_selection_state,
            fractional_scale_state,
            viewporter_state,
            cursor_shape_state,
            text_input_state,
            input_method_state,
            xdg_activation_state,
            toplevel_icon_manager,
            cursor_image: CursorImageStatus::default_named(),
            cursor_images: HashMap::new(),
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
        // Frames: a decoração (total menos o conteúdo do cliente) tem PRIORIDADE
        // sobre o conteúdo de outras janelas. Se a barra da janela da frente cobre
        // o conteúdo da de trás, o hit é da barra — não do conteúdo. Percorre o
        // Space do topo (z-order) para baixo para a janela da frente ganhar.
        for window in self.space.elements().rev() {
            let Some(cs) = window.toplevel().map(|t| t.wl_surface()) else {
                continue;
            };
            let Some((frame_surface, _, _, _)) = self
                .associated_frames
                .iter()
                .find(|(_, _, _, c)| c.as_ref() == Some(cs))
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
            // O ponteiro usa as coordenadas do CANVAS, enquanto o hit-test é
            // no target apresentado. Caps direito/inferior são deslocados pela
            // mesma diferença do 9-slice; assim botões HTML e edges continuam
            // recebendo as posições das bordas corretas sem mudar move/resize.
            let local = pos - frame_loc.to_f64();
            let target = frame_rect.size;
            let canvas = self
                .frame_canvases
                .get(frame_surface)
                .map(|canvas| canvas.configured_size())
                .unwrap_or((target.w, target.h));
            let right_cap = fgeo.insets.2 + fgeo.titlebar_slices.1;
            let mut pointer_origin = frame_loc.to_f64();
            if local.x >= (target.w - right_cap) as f64 {
                pointer_origin.x -= (canvas.0 - target.w) as f64;
            }
            if local.y >= (target.h - fgeo.insets.3) as f64 {
                pointer_origin.y -= (canvas.1 - target.h) as f64;
            }
            // Smithay (input/pointer/mod.rs:795) calcula o local do cliente como
            // `event.location - loc`: o ponto devolvido aqui é a ORIGEM GLOBAL da
            // superfície (não o hit local), para que enter/motion cheguem certos.
            tracing::debug!(?pos, ?frame_rect, ?content_rect, ?pointer_origin, hit_frame = ?frame_surface.id(), "hit-test frame (região de decoração)");
            return Some((frame_surface.clone(), pointer_origin));
        }
        if let Some((window, location)) = self.space.element_under(pos) {
            if let Some((s, p)) =
                window.surface_under(pos - location.to_f64(), WindowSurfaceType::ALL)
            {
                tracing::debug!(?pos, hit_client = ?s.id(), "hit-test cliente");
                return Some((s, (p + location).to_f64()));
            }
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
        // 02j: timeouts dos configures REAIS (não apenas dos probes sintéticos)
        // e fechamento de episódios de escala ociosos.
        let now = Instant::now();
        self.frame_diag
            .tick(now, crate::frame_diagnostics::now_ns());
        for event in self.frame_diag.drain_events() {
            eprintln!("{}", event.render());
        }
    }

    /// 02j: no shutdown, fecha o último episódio de escala ainda aberto.
    #[cfg(feature = "bench")]
    pub fn flush_frame_diagnostics(&mut self) {
        let now = Instant::now();
        self.frame_diag
            .flush(now, crate::frame_diagnostics::now_ns());
        for event in self.frame_diag.drain_events() {
            eprintln!("{}", event.render());
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
