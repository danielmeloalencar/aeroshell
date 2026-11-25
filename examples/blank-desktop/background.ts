//! Entry HMR mínimo do background; nenhum framework ou estado privilegiado.
document.body.dataset.aeroClient = "ready";
window.aero.postMessage({ type: "guest-ready", id: window.aeroSurfaceId });
