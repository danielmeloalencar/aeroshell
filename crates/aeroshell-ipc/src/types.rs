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
