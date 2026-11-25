//! Host genérico de superfícies web (F2-surface-host-genérico).
//!
//! O runtime declara `surface-create/update/destroy` no IPC do compositor; o
//! compositor publica `SurfaceCreated/Updated/Destroyed` no stream PUSH e este
//! módulo materializa cada especificação como uma WebView GTK sobre uma layer
//! surface. Nenhum socket/env/protocolo novo é introduzido.

pub mod frame;
pub mod shared;

use std::{
    cell::RefCell,
    collections::HashMap,
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;
use webkit6::{UserContentManager, WebContext, WebView};

/// Uma superfície viva e sua WebView. O par é mantido pelo mapa do host.
struct Surface {
    window: gtk4::ApplicationWindow,
    webview: WebView,
    #[allow(dead_code)]
    content: UserContentManager,
    spec: serde_json::Value,
}

std::thread_local! {
    // O guard de `hold()` precisa sobreviver ao retorno de `run()`.
    static HOLD: RefCell<Option<gtk4::gio::ApplicationHoldGuard>> = const { RefCell::new(None) };
}

fn event_socket_path() -> Option<PathBuf> {
    let ipc = std::env::var_os("AERO_WINDOW_IPC_SOCK")?;
    let mut path = PathBuf::from(ipc);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("aero-ipc")
        .to_owned();
    path.set_file_name(format!("{name}.events"));
    Some(path)
}

fn field_str<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(serde_json::Value::as_str)
}

fn field_bool(value: &serde_json::Value, field: &str) -> bool {
    value
        .get(field)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn layer_of(name: Option<&str>) -> Layer {
    match name {
        Some("bottom") => Layer::Bottom,
        Some("top") => Layer::Top,
        Some("overlay") => Layer::Overlay,
        _ => Layer::Background,
    }
}

fn keyboard_of(name: Option<&str>) -> KeyboardMode {
    match name {
        Some("exclusive") => KeyboardMode::Exclusive,
        Some("ondemand") => KeyboardMode::OnDemand,
        _ => KeyboardMode::None,
    }
}

fn field_size(spec: &serde_json::Value, field: &str, default: i32) -> i32 {
    spec.get(field)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(default)
}

fn apply_geometry(window: &gtk4::ApplicationWindow, spec: &serde_json::Value) {
    window.set_layer(layer_of(field_str(spec, "layer")));
    let output = spec
        .get("output")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let monitor = gtk4::prelude::WidgetExt::display(window)
        .monitors()
        .item(output as u32)
        .and_then(|object| object.downcast::<gtk4::gdk::Monitor>().ok());
    window.set_monitor(monitor.as_ref());
    tracing::info!(
        output,
        monitor_found = monitor.is_some(),
        "SURFACE_HOST monitor selecionado"
    );
    let anchors = spec.get("anchors");
    for (name, edge) in [
        ("left", Edge::Left),
        ("right", Edge::Right),
        ("top", Edge::Top),
        ("bottom", Edge::Bottom),
    ] {
        let enabled = anchors
            .map(|a| field_bool(a, name))
            .unwrap_or(matches!(edge, Edge::Left | Edge::Right));
        window.set_anchor(edge, enabled);
        let margin = spec
            .get("margins")
            .and_then(serde_json::Value::as_array)
            .and_then(|m| {
                m.get(match name {
                    "left" => 0,
                    "right" => 1,
                    "top" => 2,
                    _ => 3,
                })
            })
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        window.set_margin(edge, margin as i32);
    }
    window.set_default_size(
        field_size(spec, "width", 1280),
        field_size(spec, "height", 800),
    );
    // WebView tem altura natural (~200 px); um painel fino precisa também do
    // size request, não apenas do default usado antes do primeiro configure.
    window.set_size_request(
        spec.get("width")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(-1),
        spec.get("height")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(-1),
    );
    let exclusive = spec
        .get("exclusive_zone")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    window.set_exclusive_zone(exclusive as i32);
    window.set_keyboard_mode(keyboard_of(field_str(spec, "keyboard_mode")));
}

fn install_input_region(window: &gtk4::ApplicationWindow, spec: &serde_json::Value) {
    let mut rects = Vec::new();
    if let Some(regions) = spec
        .get("input_region")
        .and_then(serde_json::Value::as_array)
    {
        for region in regions {
            let x = region
                .get("x")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0) as i32;
            let y = region
                .get("y")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0) as i32;
            let w = region
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as i32;
            let h = region
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as i32;
            rects.push((x, y, w, h));
        }
    }
    let interactive = crate::input_region::InteractiveRects {
        rects,
        cursor: "default".to_owned(),
    };
    crate::input_region::apply(window, &interactive);
}

/// O reset da shell vale para TODA superfície; injetamos o link quando o HTML
/// inline não o trouxe, preservando a política de `aero://` do esquema.
fn with_shell_reset(html: &str) -> String {
    const LINK: &str = "<link rel=\"stylesheet\" href=\"aero://shell-reset.css\">";
    if html.contains("aero://shell-reset.css") {
        return html.to_owned();
    }
    if let Some(at) = html.find("<head>") {
        let mut result = html.to_owned();
        result.insert_str(at + "<head>".len(), LINK);
        result
    } else {
        format!("{LINK}{html}")
    }
}

fn create_surface(
    app: &gtk4::Application,
    context: &WebContext,
    anchor: &WebView,
    spec: &serde_json::Value,
) -> Option<Surface> {
    let Some(id) = field_str(spec, "id") else {
        tracing::warn!("SurfaceCreated sem id");
        return None;
    };
    let window = gtk4::ApplicationWindow::builder().application(app).build();
    window.init_layer_shell();
    window.set_namespace(Some(&format!("aeroshell-surface-{id}")));
    // GTK 4.22 envia commit 0x0 sem tamanho inicial; o configure sobrescreve
    // os eixos ancorados com a geometria real do output.
    apply_geometry(&window, spec);

    let webview = WebView::builder()
        .web_context(context)
        .related_view(anchor)
        .build();
    webview.set_background_color(&gtk4::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
    let content = crate::bridge::install_surface(&window, &webview, id);
    install_navigation_policy(&webview, id);
    webview.connect_load_failed(move |_, event, uri, error| {
        tracing::warn!(?event, %uri, %error, "SURFACE_HOST carga falhou");
        true
    });
    if let Some(html) = field_str(spec, "html") {
        let content = with_shell_reset(html);
        tracing::info!(id = %id, bytes = content.len(), "SURFACE_HOST html id={id}");
        webview.load_html(&content, Some(&format!("aero://surface/{id}/")));
    } else if let Some(url) = field_str(spec, "url") {
        tracing::info!(id = %id, page_id = webview.page_id(), %url, "SURFACE_HOST load_uri");
        webview.load_uri(url);
    }
    window.set_child(Some(&webview));
    window.present();
    install_input_region(&window, spec);
    tracing::info!(
        id = %id,
        kind = %field_str(spec, "kind").unwrap_or("?"),
        "SURFACE_HOST criado"
    );
    Some(Surface {
        window,
        webview,
        content,
        spec: spec.clone(),
    })
}

fn broadcast_event(surfaces: &HashMap<String, Surface>, event: &serde_json::Value) {
    let event = event.to_string();
    for surface in surfaces.values() {
        let script = format!("window.__aeroEvent && window.__aeroEvent({event});");
        surface.webview.evaluate_javascript(
            &script,
            None,
            None,
            None::<&webkit6::gio::Cancellable>,
            |result| {
                if let Err(err) = result {
                    tracing::warn!(%err, "SURFACE_HOST evento ao guest falhou");
                }
            },
        );
    }
}

fn navigation_uri_allowed(uri: &str, dev_origin: Option<&str>) -> bool {
    if uri.starts_with("aero://") || uri == "about:blank" {
        return true;
    }
    let Some(dev_origin) = dev_origin else {
        return false;
    };
    let Ok(uri) = webkit6::glib::Uri::parse(uri, webkit6::glib::UriFlags::NONE) else {
        return false;
    };
    let Ok(origin) = webkit6::glib::Uri::parse(dev_origin, webkit6::glib::UriFlags::NONE) else {
        return false;
    };
    uri.scheme() == "http"
        && origin.scheme() == "http"
        && uri.host() == origin.host()
        && uri.host().as_deref() == Some("127.0.0.1")
        && uri.port() == origin.port()
        && uri.userinfo().is_none()
        && origin.userinfo().is_none()
        && (origin.path().is_empty() || origin.path() == "/")
        && origin.query().is_none()
        && origin.fragment().is_none()
}

fn install_navigation_policy(webview: &WebView, surface_id: &str) {
    let id = surface_id.to_owned();
    let dev_origin = std::env::var("AERO_DEV_ORIGIN").ok();
    webview.connect_decide_policy(move |_, decision, decision_type| {
        if decision_type == webkit6::PolicyDecisionType::NewWindowAction {
            decision.ignore();
            tracing::info!(id = %id, "SURFACE_HOST nova janela bloqueada");
            return true;
        }
        if decision_type != webkit6::PolicyDecisionType::NavigationAction {
            return false;
        }
        let uri = decision
            .dynamic_cast_ref::<webkit6::NavigationPolicyDecision>()
            .and_then(|navigation| navigation.navigation_action())
            .and_then(|action| action.request())
            .and_then(|request| request.uri())
            .map(|uri| uri.to_string());
        if let Some(uri) = uri {
            if navigation_uri_allowed(&uri, dev_origin.as_deref()) {
                decision.use_();
            } else {
                decision.ignore();
                tracing::info!(id = %id, %uri, "SURFACE_HOST navegação externa bloqueada");
            }
        } else {
            decision.ignore();
            tracing::info!(id = %id, "SURFACE_HOST navegação sem URI bloqueada");
        }
        true
    });
}

/// Instala o consumidor do stream PUSH e roda o host até o app sair.
pub fn run(app: &gtk4::Application) {
    // Sem isto o GTK pinta a janela de branco no winit, por cima do WebView
    // transparente (o mesmo CSS provider que o painel e o frame instalam).
    crate::host::install_transparent_css();
    let Some(path) = event_socket_path() else {
        tracing::warn!("AERO_WINDOW_IPC_SOCK ausente; host de superfícies desligado");
        return;
    };
    // ADR 0001: um WebContext e uma âncora construct-only para TODOS os kinds;
    // cada WebView nasce com related_view, não com um WebProcess próprio.
    let shared = shared::new(app);
    let app_weak = app.downgrade();
    let surfaces: Rc<RefCell<HashMap<String, Surface>>> = Rc::new(RefCell::new(HashMap::new()));
    // GTK 4.22 commita 0x0 sem âncoras ao fechar uma layer surface (erro já
    // registrado no projeto). Mantemos a janela viva até o host sair.
    let retired: Rc<RefCell<Vec<Surface>>> = Rc::new(RefCell::new(Vec::new()));
    let (tx, rx) = mpsc::channel::<serde_json::Value>();

    std::thread::spawn(move || {
        let stream = match UnixStream::connect(&path) {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!(%err, ?path, "SURFACE_HOST falha ao conectar no stream");
                return;
            }
        };
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
                if tx.send(event).is_err() {
                    break;
                }
            }
        }
    });
    tracing::info!("SURFACE_HOST pronto");

    HOLD.with(|hold| *hold.borrow_mut() = Some(app.hold()));
    gtk4::glib::timeout_add_local(Duration::from_millis(1), move || {
        let Some(app) = app_weak.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        while let Ok(event) = rx.try_recv() {
            broadcast_event(&surfaces.borrow(), &event);
            for (kind, payload) in event.as_object().map(|o| o.iter()).into_iter().flatten() {
                match kind.as_str() {
                    "SurfaceCreated" => {
                        if let Some(spec) = payload.get("surface") {
                            if let Some(id) = field_str(spec, "id") {
                                // O replay de um novo assinante chega também aos
                                // assinantes antigos; ID vivo torna o replay idempotente.
                                let exists = surfaces.borrow().contains_key(id);
                                tracing::info!(id = %id, exists, "SURFACE_HOST create/replay");
                                if exists {
                                    continue;
                                }
                                if let Some(surface) = create_surface(
                                    &app,
                                    &shared.context,
                                    &shared.anchor_webview,
                                    spec,
                                ) {
                                    surfaces.borrow_mut().insert(id.to_owned(), surface);
                                }
                            }
                        }
                    }
                    "SurfaceUpdated" => {
                        if let Some(spec) = payload.get("surface") {
                            if let Some(id) = field_str(spec, "id") {
                                if let Some(surface) = surfaces.borrow_mut().get_mut(id) {
                                    apply_geometry(&surface.window, spec);
                                    install_input_region(&surface.window, spec);
                                    let content_changed = surface.spec.get("html")
                                        != spec.get("html")
                                        || surface.spec.get("url") != spec.get("url");
                                    if content_changed {
                                        if let Some(html) = field_str(spec, "html") {
                                            surface.webview.load_html(
                                                &with_shell_reset(html),
                                                Some(&format!("aero://surface/{id}/")),
                                            );
                                        } else if let Some(url) = field_str(spec, "url") {
                                            tracing::info!(
                                                id = %id,
                                                page_id = surface.webview.page_id(),
                                                %url,
                                                "SURFACE_HOST load_uri"
                                            );
                                            surface.webview.load_uri(url);
                                        }
                                    }
                                    surface.spec = spec.clone();
                                }
                            }
                        }
                    }
                    "SurfaceDestroyed" => {
                        if let Some(id) = field_str(payload, "id") {
                            if let Some(surface) = surfaces.borrow_mut().remove(id) {
                                // GTK 4.22 envia commit 0x0 ao ocultar/fechar a
                                // layer; retired mantém a WebView viva sem erro.
                                retired.borrow_mut().push(surface);
                                tracing::info!(id = %id, "SURFACE_HOST destruido");
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        gtk4::glib::ControlFlow::Continue
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn navegacao_so_permite_aero_e_a_origem_dev_exata() {
        assert!(super::navigation_uri_allowed("aero://surface/panel/", None));
        assert!(super::navigation_uri_allowed("about:blank", None));
        assert!(!super::navigation_uri_allowed(
            "http://127.0.0.1:4173/panel",
            None
        ));
        let dev = Some("http://127.0.0.1:4173");
        assert!(super::navigation_uri_allowed(
            "http://127.0.0.1:4173/panel",
            dev
        ));
        assert!(!super::navigation_uri_allowed(
            "http://127.0.0.1:4174/panel",
            dev
        ));
        assert!(!super::navigation_uri_allowed(
            "http://localhost:4173/panel",
            dev
        ));
        assert!(!super::navigation_uri_allowed("https://example.com", dev));
    }
}
