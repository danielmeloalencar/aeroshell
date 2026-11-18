//! Mensagens do IPC (a implementar no S4).

/// Revisão de um snapshot. Patches com revisão obsoleta são rejeitados.
pub type Revision = u64;
