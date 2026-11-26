//! Cena Three.js de fundo e widgets Solid transparentes, sem Rust.
import { defineDesktop } from "../../packages/sdk/src/define";

const transparentPage = `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent;font:14px sans-serif}
.widgets{position:absolute;right:24px;top:24px;display:flex;gap:8px;align-items:center;padding:12px;
  border-radius:10px;background:#f3f4f6;color:#111827;box-shadow:0 4px 20px #0006}
.widgets button{border:0;border-radius:6px;padding:6px 10px;background:#2563eb;color:white;cursor:pointer}
</style></head><body></body></html>`;

export default defineDesktop({
  surfaces: [
    {
      kind: "Desktop", id: "three-background",
      html: `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:#080d1f}
canvas{position:absolute;inset:0;z-index:0}
.stage{position:absolute;inset:0;z-index:1;pointer-events:none;perspective:700px}
.cube{position:absolute;top:50%;width:150px;height:150px;margin-top:-75px;transform-style:preserve-3d;
 animation:spin 6s linear infinite}
.cube.left{left:8%}.cube.right{right:8%;animation-direction:reverse}
.face{position:absolute;width:150px;height:150px;border:2px solid #ffffff99;box-sizing:border-box}
.face.a{background:#8b5cf6;transform:translateZ(75px)}
.face.b{background:#2563eb;transform:rotateY(180deg) translateZ(75px)}
.face.c{background:#06b6d4;transform:rotateY(90deg) translateZ(75px)}
.face.d{background:#16a34a;transform:rotateY(-90deg) translateZ(75px)}
.face.e{background:#eab308;transform:rotateX(90deg) translateZ(75px)}
.face.f{background:#db2777;transform:rotateX(-90deg) translateZ(75px)}
@keyframes spin{from{transform:rotateX(-20deg) rotateY(0)}to{transform:rotateX(-20deg) rotateY(360deg)}}
body[data-running="stopped"] .cube{animation-play-state:paused}
</style></head><body><div class="stage"><div class="cube left">
<div class="face a"></div><div class="face b"></div><div class="face c"></div>
<div class="face d"></div><div class="face e"></div><div class="face f"></div></div>
<div class="cube right">
<div class="face a"></div><div class="face b"></div><div class="face c"></div>
<div class="face d"></div><div class="face e"></div><div class="face f"></div></div></div></body></html>`,
      dev_entry: "./background.js",
    },
    {
      kind: "Overlay", id: "three-widgets", html: transparentPage, dev_entry: "./widgets.tsx",
    },
  ],
  frame: { enabled: true },
  keybindings: [{ keys: "Super+4", command: "workspace-activate 1" }],
  rules: { default: "Server", rules: [] },
});
