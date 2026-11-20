//! Esquema `aero://`. Serve o reset compartilhado e os bundles das superfícies.
//! Navegação externa é bloqueada: apenas estas rotas existem.

use webkit6::gio::MemoryInputStream;
use webkit6::glib::Bytes;
use webkit6::{URISchemeRequest, WebContext};

pub const PANEL_URI: &str = "aero://panel/index.html";

const SHELL_RESET_CSS: &str = include_str!("../../../assets/shell-reset.css");
const PANEL_JS: &[u8] = include_bytes!("../../../packages/panel/dist/app.js");

/// HTML do painel React (04b). O bundle é servido em `aero://panel/app.js`.
const REACT_PANEL_HTML: &str = r##"
<!doctype html>
<html lang="pt-br">
  <head>
    <meta charset="utf-8" />
    <link rel="stylesheet" href="aero://shell-reset.css" />
    <style>
      #root { height: 100%; display: flex; align-items: center; padding: 0 8px;
        color: #e8f6ff; font: 14px sans-serif; }
      .aero-panel { display: flex; gap: 8px; width: 100%; }
      .aero-row { display: flex; gap: 6px; align-items: center; height: 18px;
        background: #23364a; color: #e8f6ff; border: 1px solid #4a6b8a;
        border-radius: 4px; padding: 0 6px; font: inherit; cursor: pointer; }
      .aero-id { opacity: 0.7; }
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script src="aero://panel/app.js"></script>
  </body>
</html>
"##;

/// Stub mantido para `?legacy` (testes S2/S3 de input region e overflow).
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
      #hot { padding: 6px 12px; border: 1px solid #001100; border-radius: 6px; }
    </style>
  </head>
  <body>
    <div id="panel">
      <span id="hot" data-aero-interactive data-aero-cursor="pointer">CLIQUE AQUI (input)</span>
      <span style="margin-left:16px">área transparente → passa clique</span>
    </div>
    <script>
      function postInteractive() {
        const els = Array.from(document.querySelectorAll('[data-aero-interactive]'));
        const rects = els.map(e => { const r = e.getBoundingClientRect();
          return { x:r.left, y:r.top, w:r.width, h:r.height }; });
        window.webkit.messageHandlers.aero.postMessage({ type:'interactive',
          rects, cursor: els[0]?.dataset.aeroCursor || 'default' });
      }
      let clicks = 0;
      document.addEventListener('click', () => {
        clicks++;
        const hot = document.getElementById('hot');
        if (hot) hot.textContent = 'CLIQUE AQUI (' + clicks + ')';
        window.webkit.messageHandlers.aero.postMessage({ type:'click', count:clicks });
        postInteractive();
      });
      window.addEventListener('load', postInteractive);
      window.addEventListener('resize', postInteractive);
      new MutationObserver(postInteractive).observe(document.body, {
        childList: true, subtree: true, characterData: true, attributes: true
      });
      if (window.ResizeObserver) {
        const ro = new ResizeObserver(postInteractive);
        document.querySelectorAll('[data-aero-interactive]').forEach(e => ro.observe(e));
      }
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
        let legacy = uri.contains("legacy");
        if uri.contains("app.js") {
            let stream = MemoryInputStream::from_bytes(&Bytes::from_static(PANEL_JS));
            request.finish(&stream, PANEL_JS.len() as i64, Some("text/javascript"));
            return;
        }
        let (body, mime) = if uri.contains("shell-reset.css") {
            (SHELL_RESET_CSS, "text/css")
        } else if uri.contains("/index.html") && !legacy {
            (REACT_PANEL_HTML, "text/html")
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
