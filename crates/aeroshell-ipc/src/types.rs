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
