//! Fundo estático: nenhum loop de redraw enquanto a cena não muda.
document.body.dataset.aeroClient = "ready";
window.aero.postMessage({ type: "guest-ready", id: window.aeroSurfaceId, framework: "static" });
