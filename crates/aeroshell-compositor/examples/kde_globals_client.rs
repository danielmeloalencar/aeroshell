//! Cliente de teste: lista globals e exige org_kde_kwin_server_decoration_manager.
//! Sem dependência nova: só wayland-client (dev-dependency existente).

use wayland_client::{
    protocol::wl_registry::{Event as RegistryEvent, WlRegistry},
    Connection, Dispatch, QueueHandle,
};

const KDE_DECORATION: &str = "org_kde_kwin_server_decoration_manager";

struct App {
    globals: Vec<String>,
}

impl Dispatch<WlRegistry, ()> for App {
    fn event(
        state: &mut App,
        _: &WlRegistry,
        event: RegistryEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<App>,
    ) {
        if let RegistryEvent::Global { interface, .. } = event {
            eprintln!("KDE_CLIENT global {interface}");
            state.globals.push(interface);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::connect_to_env()?;
    let display = connection.display();
    let mut queue = connection.new_event_queue::<App>();
    let qh = queue.handle();
    let _registry = display.get_registry(&qh, ());
    let mut app = App {
        globals: Vec::new(),
    };
    queue.roundtrip(&mut app)?;
    let present = app.globals.iter().any(|name| name == KDE_DECORATION);
    eprintln!(
        "KDE_CLIENT CHECK {KDE_DECORATION} {}",
        if present { "presente" } else { "AUSENTE" }
    );
    if present {
        eprintln!("KDE_CLIENT OK");
        Ok(())
    } else {
        Err("org_kde_kwin_server_decoration_manager ausente".into())
    }
}
