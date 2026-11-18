//! Esquema `aero://`. Serve o reset compartilhado e os bundles das superfícies.
//! Navegação externa é bloqueada: apenas estas rotas existem.

use webkit6::gio::MemoryInputStream;
use webkit6::glib::Bytes;
use webkit6::{URISchemeRequest, WebContext};

pub const PANEL_URI: &str = "aero://panel/index.html";

const SHELL_RESET_CSS: &str = include_str!("../../../assets/shell-reset.css");

const PANEL_HTML: &str = r##"
<!doctype html>
<html lang="pt-br">
  <head>
    <meta charset="utf-8" />
    <link rel="stylesheet" href="aero://shell-reset.css" />
    <style>
      #panel { height: 100%; width: 100%; display: flex; align-items: center;
        padding: 0 12px; background: #33dd66; color: #001100;
        font-weight: bold; font-size: 16px; }
    </style>
  </head>
  <body>
    <div id="panel" data-aero-interactive>PANEL AEROSHELL S2 — verde = layer-shell + WebView</div>
    <script>
      if (location.search.includes('legacy')) {
        document.querySelector('link[href="aero://shell-reset.css"]').remove();
        document.getElementById('panel').style.margin = '4px';
      }
      if (location.search.includes('overflow')) {
        const d = document.createElement('div');
        d.style.height = '120px';
        d.textContent = 'overflow forçado';
        document.body.appendChild(d);
      }
      window.webkit.messageHandlers.aero.postMessage({
        type: 'metrics',
        overflow: getComputedStyle(document.documentElement).overflow,
        scrollHeight: document.documentElement.scrollHeight,
        clientHeight: document.documentElement.clientHeight,
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth
      });
    </script>
  </body>
</html>
"##;

pub fn install(context: &WebContext) {
    context.register_uri_scheme("aero", |request: &URISchemeRequest| {
        // `aero://panel/index.html` => host="panel", path="/index.html";
        // casar pela URI completa evita confundir host e path.
        let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
        tracing::info!(%uri, "aero:// request");
        let (body, mime) = if uri.contains("shell-reset.css") {
            (SHELL_RESET_CSS, "text/css")
        } else if uri.contains("/index.html") {
            (PANEL_HTML, "text/html")
        } else {
            ("", "text/plain")
        };
        let bytes = Bytes::from_static(body.as_bytes());
        let stream = MemoryInputStream::from_bytes(&bytes);
        request.finish(&stream, body.len() as i64, Some(mime));
    });
}
