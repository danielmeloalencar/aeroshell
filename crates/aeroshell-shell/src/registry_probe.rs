//! Prova do caminho de produção: o supervisor entrega a ponta privilegiada do
//! socketpair ao shell via `WAYLAND_SOCKET`. Antes do GTK/GDK inicializar (e
//! consumir esse fd), enumeramos o registry nessa conexão real e logamos os
//! globals. Assim o log do próprio shell comprova que ele recebeu o fd e que o
//! compositor anuncia `aero_frame_manager_v1` apenas para essa conexão.
//!
//! Depois removemos `WAYLAND_SOCKET` do ambiente: o GDK conecta pelo socket
//! público (`WAYLAND_DISPLAY`), onde o manager NÃO é visível — exatamente a
//! fronteira de segurança que queremos demonstrar.

use wayland_client::{protocol::wl_registry, Connection, Dispatch, QueueHandle};

struct Globals;

impl Dispatch<wl_registry::WlRegistry, ()> for Globals {
    fn event(
        _state: &mut Self,
        _registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            let mark = if interface == "aero_frame_manager_v1" {
                " <<< privileged"
            } else {
                ""
            };
            tracing::info!(
                name,
                %interface,
                version,
                mark,
                "registry do shell (via WAYLAND_SOCKET)"
            );
        }
    }
}

/// Chamado no início de `main`, antes de qualquer GDK/GTK tocar no ambiente.
pub fn log_privileged_globals() {
    if std::env::var_os("WAYLAND_SOCKET").is_none() {
        return;
    }
    match Connection::connect_to_env() {
        Ok(connection) => {
            let mut queue = connection.new_event_queue();
            let qh = queue.handle();
            connection.display().get_registry(&qh, ());
            match queue.roundtrip(&mut Globals) {
                Ok(_) => tracing::info!("shell conectou via WAYLAND_SOCKET (privilegiado)"),
                Err(err) => tracing::warn!(%err, "roundtrip do registry falhou"),
            }
        }
        Err(err) => tracing::warn!(%err, "falha ao conectar via WAYLAND_SOCKET"),
    }
    // O fd privilegiado já foi usado; o GDK deve seguir pelo socket público.
    std::env::remove_var("WAYLAND_SOCKET");
}
