//! Protocolo IPC AeroShell: snapshot + patches revisionados e comandos atômicos.
//! Fase 0: transporte textual por linha no compositor; tipos TS gerados via ts-rs.
//! O codec binário (F0/S4) ainda não existe — não inventar API aqui.

pub mod messages;
pub mod types;

pub use types::{Patch, Revision, Snapshot, WindowChange, WindowInfo};
