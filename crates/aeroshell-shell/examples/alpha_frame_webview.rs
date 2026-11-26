//! Fixture do F2-alpha-confirm: frame real com áreas opacas e transparentes.
//! O cap direito/inferior fica opaco só nos 6 px externos; o restante é alfa zero.

const HTML: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent;user-select:none}
#bar{position:absolute;left:0;top:0;width:120px;height:30px;background:#20c040;color:#fff;cursor:all-scroll}
#right{position:absolute;right:0;top:0;width:6px;height:100%;background:#20c040}
#bottom{position:absolute;left:0;bottom:0;width:120px;height:6px;background:#20c040}
#title,#icon{display:none}
</style></head><body>
<div id="bar" data-aero-interactive><span id="title"></span><img id="icon" alt=""></div>
<div id="right"></div><div id="bottom"></div>
<script>
const post = (value) => window.webkit.messageHandlers.aero.postMessage(value);
window.aeroHideFrame = () => {
  for (const id of ['bar','right','bottom']) document.getElementById(id).style.opacity='0';
  post({action:'csd_applied',client_frame:true,bar:'hidden_opacity',border:'hidden_opacity'});
};
window.aeroShowFrame = () => {
  for (const id of ['bar','right','bottom']) document.getElementById(id).style.opacity='1';
  post({action:'frame_restored',client_frame:false,bar:'1',border:'1'});
};
window.aeroSetTitle = (title) => {
  document.getElementById('title').textContent=title;
  post({action:'title_applied',title});
};
window.aeroSetIcon = () => {};
window.aeroFramePainted = (serial) => {
  requestAnimationFrame(() => requestAnimationFrame(() => post({action:'frame_painted',serial})));
};
</script></body></html>"#;

fn main() {
    aeroshell_shell::surfaces::frame::main_with_html(HTML);
}
