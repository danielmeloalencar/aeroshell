//! Taskbar Vue: store reativo; ações privilegiadas ficam fora do guest.
import { defineComponent, h, ref, watch } from "vue";
import {
  mountSurface, useApps, useOutputs, useWindows, useWorkspaces,
} from "../../packages/vue/src/index";
import { createSurfaceSession } from "../../packages/sdk/src/surface-session";

const session = createSurfaceSession();
const menuChannel = new BroadcastChannel("aero-win11-menu");
const Taskbar = defineComponent({
  setup() {
    const windows = useWindows();
    const workspaces = useWorkspaces();
    const outputs = useOutputs();
    const apps = useApps();
    const open = ref(false);
    // A layer da taskbar tem 72 px; o menu vive numa Overlay fullscreen.
    const toggle = () => menuChannel.postMessage("toggle");
    watch([open, windows, apps, workspaces, outputs], () => {
      window.aero.postMessage({
        type: "guest-ready", id: window.aeroSurfaceId, framework: "vue",
        menu: open.value ? "open" : "closed", windowCount: windows.value.length,
        appCount: apps.value.length, outputCount: outputs.value.length,
      });
    }, { immediate: true });
    return () => h("div", null, [
      open.value ? h("section", { class: "start-menu", "data-testid": "start-menu" }, [
        h("strong", null, "Iniciar · Vue"),
        ...apps.value.map((app) => h("button", { class: "app", key: app.id }, app.name)),
      ]) : null,
      h("nav", { class: "taskbar", "data-testid": "taskbar" }, [
        h("button", { onClick: toggle, "data-testid": "start-button" }, "⊞ Iniciar"),
        ...windows.value.map((window) => h("button", { class: "window", key: window.id }, window.title)),
      ]),
    ]);
  },
});

await mountSurface(Taskbar, { session });
await session.client.setInputRegion([{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }]);
window.aero.postMessage({
  type: "guest-ready", id: window.aeroSurfaceId, framework: "vue",
  menu: "closed", windowCount: session.store.getSnapshot().windows.length,
});
