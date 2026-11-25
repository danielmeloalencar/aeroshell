//! Desktop mínimo em TypeScript puro: nenhum framework e nenhum Rust do dev.
import { defineDesktop } from "../../packages/sdk/src/define";

const background = `<!doctype html>
<html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#e00000}
</style></head><body></body></html>`;

const panel = `<!doctype html>
<html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#00d000;color:#002800;font:14px sans-serif}
body{display:flex;align-items:center;justify-content:center}
</style></head><body><strong>blank-desktop · SDK sem framework</strong>
<script type="module">
try {
  const { createClient } = await import("./client.js");
  const client = createClient();
  client.subscribe((event) => {
    if ("Focus" in event) document.body.dataset.focus = String(event.Focus.id);
  });
  const rect = () => [{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }];
  await client.update({ input_region: rect(), blur_region: rect() });
  document.body.dataset.aeroClient = "ready";
  window.aero.postMessage({ type: "guest-ready", id: window.aeroSurfaceId });
} catch (error) {
  window.aero.postMessage({ type: "guest-error", message: String(error) });
}
</script></body></html>`;

export default defineDesktop({
  surfaces: [
    { kind: "Desktop", id: "blank-background", html: background },
    {
      kind: "Panel",
      id: "blank-panel",
      anchors: { left: true, right: true, top: false, bottom: true },
      height: 56,
      exclusive_zone: 56,
      html: panel,
    },
  ],
  frame: { enabled: true },
  rules: {
    default: "Server",
    rules: [
      { app_id: null, class: "gedit", policy: "Client" },
      { app_id: "aero-no-deco", class: null, policy: "None" },
    ],
  },
});
