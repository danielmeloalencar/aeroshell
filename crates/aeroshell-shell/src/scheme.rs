//! Esquema `aero://`. Serve o reset compartilhado e os bundles das superfícies.
//! Navegação externa é bloqueada: apenas estas rotas existem.

use gtk4::gio::prelude::FileExt;
use gtk4::{IconLookupFlags, IconTheme, TextDirection};
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
      #root { position: relative; height: 100%; color: #e8f6ff; font: 14px sans-serif; }
      .aero-panel { position: absolute; left: 0; right: 0; bottom: 0; height: 48px;
        display: flex; align-items: center; gap: 8px; padding: 0 8px; min-width: 0; }
      .aero-notifications { position: absolute; right: 16px; bottom: 56px;
        display: flex; flex-direction: column; align-items: flex-end; gap: 6px;
        max-height: calc(100% - 56px); overflow: hidden; }
      .aero-notification { display: flex; align-items: flex-start; gap: 8px;
        width: 320px; min-height: 68px; padding: 8px; border: 1px solid #4a6b8a;
        border-radius: 8px; background: #182635ee; box-shadow: 0 2px 10px #0008; }
      .aero-notification-icon { width: 28px; height: 28px; flex: 0 0 28px; }
      .aero-notification-content { display: flex; flex-direction: column; gap: 3px;
        flex: 1 1 auto; min-width: 0; }
      .aero-notification-title { font: 600 13px sans-serif; color: #e8f6ff; }
      .aero-notification-body { font: 12px sans-serif; color: #c4d8ea;
        overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .aero-notification-close { display: inline-flex; align-items: center;
        justify-content: center; flex: 0 0 24px; width: 24px; height: 24px;
        border: 1px solid transparent; border-radius: 4px; background: transparent;
        color: #e8f6ff; font: 13px sans-serif; cursor: pointer; }
      .aero-notification-close:hover { border-color: #9fd0ff; color: #ffd9d9; }
      .aero-workspaces { display: flex; align-items: center; gap: 4px; flex: 0 0 auto; }
      .aero-launcher-toggle { flex: 0 0 auto; height: 24px; padding: 0 8px;
        border: 1px solid #4a6b8a; border-radius: 4px; background: #23364a;
        color: #e8f6ff; font: 12px sans-serif; cursor: pointer; }
      .aero-launcher-toggle:hover { border-color: #9fd0ff; }
      .aero-launcher { display: flex; align-items: center; gap: 6px; flex: 1 1 auto; min-width: 0; }
      .aero-launcher-search { flex: 0 0 150px; width: 150px; height: 24px; padding: 0 7px;
        border: 1px solid #4a6b8a; border-radius: 4px; outline: none;
        background: #182635; color: #e8f6ff; font: 12px sans-serif; user-select: text; }
      .aero-launcher-search:focus { border-color: #9fd0ff; }
      .aero-apps { display: flex; align-items: center; gap: 5px; min-width: 0;
        overflow-x: auto; overflow-y: hidden; }
      .aero-app-row { display: flex; align-items: center; gap: 5px; flex: 0 0 auto;
        max-width: 160px; height: 26px; padding: 0 7px; border: 1px solid #4a6b8a;
        border-radius: 4px; background: #23364a; color: #e8f6ff; font: 12px sans-serif;
        cursor: pointer; }
      .aero-app-row:hover { background: #34618a; border-color: #9fd0ff; }
      .aero-app-icon { width: 16px; height: 16px; flex: 0 0 16px; }
      .aero-app-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .aero-apps-empty { color: #b8c9d9; font: 12px sans-serif; white-space: nowrap; }
      .aero-workspace { width: 88px; height: 20px; padding: 0 3px; border: 1px solid #4a6b8a;
        border-radius: 4px; background: #23364a; cursor: pointer; }
      .aero-workspace[data-active="1"] { background: #34618a; border-color: #9fd0ff; }
      .aero-workspace input { width: 100%; height: 18px; padding: 0; border: 0; outline: none;
        background: transparent; color: #e8f6ff; font: 12px sans-serif; text-align: center; }
      .aero-workspace-add { width: 24px; height: 22px; padding: 0; border: 1px solid #4a6b8a;
        border-radius: 4px; background: #23364a; color: #e8f6ff; font: 16px sans-serif; cursor: pointer; }
      .aero-workspace-add:hover { border-color: #9fd0ff; }
      .aero-row { display: flex; gap: 6px; align-items: center; height: 18px;
        background: #23364a; color: #e8f6ff; border: 1px solid #4a6b8a;
        border-radius: 4px; padding: 0 6px; font: inherit; cursor: pointer; }
      .aero-row[data-focused="1"] { background: #34618a; border-color: #9fd0ff; }
      .aero-focus { font-size: 10px; color: #9fd0ff; }
      .aero-id { opacity: 0.7; }
      .aero-decoration { font-size: 10px; opacity: 0.8; padding: 0 3px;
        border: 1px solid #4a6b8a; border-radius: 3px; }
      .aero-close { display: inline-flex; align-items: center; justify-content: center;
        margin-left: 2px; min-width: 16px; width: 16px; height: 16px; font-size: 11px;
        border: 1px solid transparent; border-radius: 3px; flex: 0 0 16px; }
      .aero-close:hover { border-color: #9fd0ff; color: #ffd9d9; }
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

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let text = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(text, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Resolve o campo Icon do `.desktop` para um recurso local do tema. O painel
/// continua restrito a `aero://`; somente SVG/PNG são servidos à WebView.
fn themed_icon(uri: &str) -> Option<(Vec<u8>, &'static str)> {
    let name = percent_decode(uri.strip_prefix("aero://icon/")?)?;
    if name.is_empty() {
        return None;
    }
    let path = if name.starts_with('/') {
        std::path::PathBuf::from(name)
    } else {
        let display = gtk4::gdk::Display::default()?;
        let theme = IconTheme::for_display(&display);
        theme
            .lookup_icon(
                &name,
                &[],
                20,
                1,
                TextDirection::Ltr,
                IconLookupFlags::empty(),
            )
            .file()?
            .path()?
    };
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => return None,
    };
    Some((std::fs::read(path).ok()?, mime))
}

pub fn install(context: &WebContext) {
    context.register_uri_scheme("aero", |request: &URISchemeRequest| {
        // `aero://panel/index.html` => host="panel", path="/index.html";
        // casar pela URI completa evita confundir host e path.
        let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
        tracing::info!(%uri, "aero:// request");
        if uri.starts_with("aero://icon/") {
            if let Some((bytes, mime)) = themed_icon(&uri) {
                let size = bytes.len() as i64;
                let stream = MemoryInputStream::from_bytes(&Bytes::from_owned(bytes));
                request.finish(&stream, size, Some(mime));
            } else {
                let stream = MemoryInputStream::new();
                request.finish(&stream, 0, Some("text/plain"));
            }
            return;
        }
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
