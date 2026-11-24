//! Tipos do estado observável do compositor. Fonte única para os bindings TS
//! gerados por `ts-rs` (ver `package.json` → `generate:types`).
//!
//! NÃO escreva as interfaces TypeScript à mão: elas são geradas daqui.

#[cfg(feature = "export-bindings")]
use ts_rs::TS;

/// Revisão monotônica do snapshot. Patches com revisão <= a atual são ignorados.
pub type Revision = u32;

/// Uma janela do compositor, com o `window_id` lógico e estável.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Um workspace e as janelas que pertencem a ele. A janela continua existindo
/// quando seu workspace está inativo; apenas sua participação na cena muda.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: u32,
    pub name: String,
    pub window_ids: Vec<u32>,
}

/// Estado completo publicado pelo compositor.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub revision: Revision,
    pub windows: Vec<WindowInfo>,
}

/// Mudança incremental em relação ao snapshot anterior.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowChange {
    Upsert(WindowInfo),
    Remove(u32),
}

/// Patch revisionado aplicado ao store do SDK.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    pub revision: Revision,
    pub change: WindowChange,
}

/// Decisão de decoração devolvida pela política Bun (F2-decoration-policy).
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationPolicy {
    Server,
    Client,
    None,
}

/// Uma regra declarativa da política de decoração (F2-decoration-policy-ts).
/// `null` num seletor significa "casa qualquer janela"; a primeira regra que
/// casa vence. Tipos exportados por ts-rs: o runtime TS não escreve tipos à mão.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorationRule {
    /// app_id exato do xdg_toplevel.
    pub app_id: Option<String>,
    /// "Classe": Wayland não tem WM_CLASS; casa o app_id ou seu último segmento.
    pub class: Option<String>,
    pub policy: DecorationPolicy,
}

/// Configuração carregada pelo runtime Bun no startup (sem hot-reload neste cartão).
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecorationPolicyConfig {
    pub rules: Vec<DecorationRule>,
    pub default: DecorationPolicy,
}

/// Aplicativo instalado descoberto a partir de um arquivo `.desktop`.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    // O id é o nome do arquivo sem `.desktop` e a chave do comando `launch`.
    pub id: String,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub categories: Vec<String>,
}

/// Notificação interna e efêmera exibida pelo shell (sem histórico/D-Bus).
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: String,
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub timeout_ms: u64,
}

/// Tipo de superfície web declarada pelo runtime (contrato `aero.surfaces`).
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Desktop,
    Panel,
    Overlay,
    Frame,
}

/// Camada do protocolo `zwlr_layer_shell_v1` usada pela superfície.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceLayer {
    Background,
    Bottom,
    Top,
    Overlay,
}

/// Política de foco de teclado da superfície de camada.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKeyboardMode {
    None,
    OnDemand,
    Exclusive,
}

/// Âncoras de uma superfície de camada (bordas do output).
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceAnchors {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

/// Margens lógicas nos lados esquerdo, direito, superior e inferior.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceMargins {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}

/// Retângulo de input em coordenadas locais da superfície.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceInputRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Especificação completa de uma superfície web enviada pelo runtime.
/// `html` e `url` são mutuamente exclusivos; URLs só podem usar `aero://`.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceSpec {
    pub id: String,
    pub kind: SurfaceKind,
    pub output: Option<u32>,
    pub anchors: SurfaceAnchors,
    pub margins: SurfaceMargins,
    pub exclusive_zone: u32,
    pub layer: SurfaceLayer,
    pub keyboard_mode: SurfaceKeyboardMode,
    pub input_region: Option<Vec<SurfaceInputRegion>>,
    pub html: Option<String>,
    pub url: Option<String>,
}

/// Evento PUSH do compositor para shell/SDK (substitui o polling do S4).
/// A serialização na wire é o enum em forma de tagged object do ts-rs, ex.:
/// `{"Title":{"id":0,"title":"foot"}}`.
#[cfg_attr(feature = "export-bindings", derive(TS))]
#[cfg_attr(feature = "export-bindings", ts(export))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AeroEvent {
    Snapshot {
        snapshot: Snapshot,
    },
    Title {
        id: u32,
        title: String,
    },
    Icon {
        id: u32,
        name: Option<String>,
        has_pixels: bool,
    },
    Focus {
        id: Option<u32>,
    },
    Geometry {
        id: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    Csd {
        id: u32,
        client_frame: bool,
    },
    FramePainted {
        id: u32,
    },
    WorkspaceCreated {
        workspace: Workspace,
    },
    WorkspaceActivated {
        id: u32,
    },
    WorkspaceRenamed {
        id: u32,
        name: String,
    },
    WorkspaceDestroyed {
        id: u32,
    },
    WindowMovedToWorkspace {
        window_id: u32,
        workspace_id: u32,
    },
    AppListUpdated {
        apps: Vec<App>,
    },
    /// Pedido/estado de uma superfície web; o host consome pelo stream PUSH.
    SurfaceCreated {
        surface: SurfaceSpec,
    },
    SurfaceUpdated {
        surface: SurfaceSpec,
    },
    SurfaceDestroyed {
        id: String,
    },
    NotificationShown {
        notification: Notification,
    },
    NotificationDismissed {
        id: String,
    },
    /// Pergunta ao runtime Bun como decorar esta janela.
    DecorationRequest {
        id: u32,
        app_id: String,
        title: String,
        mode: String,
        client_frame: bool,
    },
    /// Decisão aplicada (explícita ou fallback).
    DecorationApplied {
        id: u32,
        decision: DecorationPolicy,
        fallback: bool,
        reason: Option<String>,
    },
}
