//! Desktop mínimo React: nenhum Rust e nenhum IPC privilegiado no guest.
import { defineDesktop } from "../../packages/sdk/src/define";

const background = `<!doctype html>
<html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#e00000}
</style></head><body></body></html>`;

const panel = `<!doctype html>
<html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#00d000;color:#002800;font:14px sans-serif}
body{display:flex;align-items:center;justify-content:center}
main{text-align:center} #window-count{font-size:12px}
#config-text{position:absolute;left:-10000px}
</style></head><body><div id="config-text">blank-desktop · SDK sem framework</div><div id="root"></div></body></html>`;

export default defineDesktop({
  surfaces: [
    {
      kind: "Desktop",
      id: "blank-background",
      html: background,
      dev_entry: "./background.ts",
    },
    {
      kind: "Panel",
      id: "blank-panel",
      output: 0,
      anchors: { left: true, right: true, top: false, bottom: true },
      height: 56,
      exclusive_zone: 56,
      html: panel,
      dev_entry: "./panel.ts",
    },
    {
      kind: "Panel",
      id: "blank-panel-output-1",
      output: 1,
      anchors: { left: true, right: true, top: false, bottom: true },
      height: 56,
      exclusive_zone: 56,
      html: panel,
      dev_entry: "./panel.ts",
    },
  ],
  frame: { enabled: true },
  keybindings: [
    { keys: "Super+1", command: "workspace-activate 1" },
    { keys: "Super+2", command: "workspace-activate 2" },
  ],
  rules: {
    default: "Server",
    rules: [
      { app_id: null, class: "gedit", policy: "Client" },
      { app_id: "aero-no-deco", class: null, policy: "None" },
    ],
  },
});
