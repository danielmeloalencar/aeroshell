//! Taskbar centrada e Start em Vue, declarados sem Rust.
import { defineDesktop } from "../../packages/sdk/src/define";

const page = `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent;font:14px "Segoe UI",sans-serif}
.taskbar{position:absolute;left:50%;bottom:8px;transform:translateX(-50%);display:flex;gap:8px;align-items:center;
  min-width:320px;max-width:90vw;padding:8px 12px;border-radius:12px;background:rgba(32,36,48,.96);color:white;box-shadow:0 4px 20px #0008}
.taskbar button,.app{border:0;border-radius:6px;background:#3b82f6;color:white;padding:7px 12px;cursor:pointer}
.window{max-width:130px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;background:#485066!important}
.start-menu{position:absolute;left:50%;bottom:64px;transform:translateX(-50%);width:300px;max-height:300px;overflow:auto;
  padding:16px;border-radius:12px;background:#202430;color:white;box-shadow:0 6px 28px #000a}
.app{display:block;width:100%;margin:6px 0;text-align:left;background:#303748}
</style></head><body></body></html>`;

export default defineDesktop({
  surfaces: [
    {
      kind: "Desktop", id: "win11-background",
      html: "<!doctype html><html><body style='margin:0;background:#101828'></body></html>",
      dev_entry: "./background.ts",
    },
    {
      kind: "Panel", id: "win11-taskbar", output: 0,
      anchors: { left: true, right: true, top: false, bottom: true },
      height: 72, exclusive_zone: 0, html: page, dev_entry: "./panel.ts",
    },
    {
      kind: "Overlay", id: "win11-start",
      // Única capability concedida ao guest: launch de app já validado no compositor.
      actions: ["LaunchApp"],
      html: `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent;font:14px "Segoe UI",sans-serif}
.start-menu{position:absolute;left:50%;bottom:78px;transform:translateX(-50%);width:320px;max-height:420px;
 overflow:auto;padding:16px;border-radius:12px;background:#202430;color:white;box-shadow:0 6px 28px #000a}
.start-menu strong{display:block;margin-bottom:12px}
.app{display:block;width:100%;margin:6px 0;padding:8px 10px;border:0;border-radius:6px;
 text-align:left;background:#303748;color:white;cursor:pointer}
</style></head><body></body></html>`,
      dev_entry: "./start-menu.ts",
    },
  ],
  frame: { enabled: true },
  keybindings: [{ keys: "Super+3", command: "workspace-activate 1" }],
  rules: { default: "Server", rules: [] },
});
