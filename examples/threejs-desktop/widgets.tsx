//! Widget Solid: signals + DOM, sem exigir um plugin JSX específico do Solid 2.
import { createEffect } from "solid-js";
import { mountSurface, useWindows } from "../../packages/solid/src/index";
import { createSurfaceSession } from "../../packages/sdk/src/surface-session";

const session = createSurfaceSession();
const control = new BroadcastChannel("aero-three-control");
function toggleScene() {
  const token = `${Date.now()}-${Math.random()}`;
  control.postMessage(token);
  try { localStorage.setItem("aero-three-control", token); } catch { /* origem opaca */ }
}
function Widgets() {
  const windows = useWindows();
  const section = document.createElement("section");
  section.className = "widgets";
  section.dataset.framework = "solid";
  const button = document.createElement("button");
  button.dataset.testid = "three-toggle";
  button.textContent = "▶ / ■";
  button.addEventListener("click", toggleScene);
  const count = document.createElement("span");
  section.append(button, count);
  createEffect(() => windows().length, (windowCount) => {
    count.textContent = `${windowCount} janela(s) · Solid`;
    window.aero.postMessage({
      type: "guest-ready", id: window.aeroSurfaceId, framework: "solid",
      windowCount,
    });
  });
  return section;
}

await mountSurface(Widgets, { session });
await session.client.setInputRegion([{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }]);
window.aero.postMessage({
  type: "guest-ready", id: window.aeroSurfaceId, framework: "solid",
  windowCount: session.store.getSnapshot().windows.length,
});
