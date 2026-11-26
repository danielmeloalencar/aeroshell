//! Painel React do blank-desktop; produção e HMR usam o mesmo entry.
import { createElement, useEffect } from "react";
import { mountSurface, useEvent, useWindows } from "../../packages/react/src/index";
import { createSurfaceSession } from "../../packages/sdk/src/surface-session";

const session = createSurfaceSession();

function Panel() {
  const windows = useWindows();
  useEvent((event) => {
    if ("Focus" in event) document.body.dataset.focus = String(event.Focus.id);
  });
  useEffect(() => {
    window.aero.postMessage({
      type: "guest-ready",
      id: window.aeroSurfaceId,
      framework: "react",
      windowCount: windows.length,
    });
  }, [windows.length]);
  return createElement(
    "main",
    { "data-framework": "react" },
    createElement("strong", null, "blank-desktop · SDK sem framework"),
    createElement("span", { id: "window-count" }, ` · React · ${windows.length} janela(s)`),
    createElement("small", null, " · abra outro foot: o contador muda; edite panel.ts: MODULO_ANTES → MODULO_DEPOIS"),
  );
}

document.body.dataset.module = "MODULO_ANTES";
await mountSurface(Panel, {
  session,
  target: document.getElementById("root") ?? document.body,
});
const rect = () => [{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }];
await session.client.update({ input_region: rect(), blur_region: rect() });
document.body.dataset.aeroClient = "ready";
window.aero.postMessage({
  type: "guest-ready",
  id: window.aeroSurfaceId,
  text: document.body.textContent,
  module: document.body.dataset.module,
  framework: "react",
  windowCount: session.store.getSnapshot().windows.length,
});

// Fixture de segurança do harness: a ponte deve recusar este comando global.
if (new URLSearchParams(location.search).has("input-bind-test")) {
  window.aero.postMessage({
    type: "ipc",
    command: 'input-bind {"id":"guest-bind","keys":"Super+9"}',
    seq: 900001,
  });
}

// Fixture de segurança do harness: a decisão de navegação pertence ao host.
if (new URLSearchParams(location.search).has("external-test")) {
  location.href = "https://example.com/";
}
