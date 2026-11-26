//! Menu Iniciar em Overlay própria: não é cortado pela altura da taskbar.
import { defineComponent, h, ref, watch } from "vue";
import { mountSurface, useApps, useEvent } from "../../packages/vue/src/index";
import { createSurfaceSession } from "../../packages/sdk/src/surface-session";

const session = createSurfaceSession();
const menuChannel = new BroadcastChannel("aero-win11-menu");
let nextSeq = 1;

const StartMenu = defineComponent({
  setup() {
    const open = ref(false);
    const apps = useApps();
    const toggle = () => { open.value = !open.value; };
    // Apenas esta superfície declarou actions:["LaunchApp"] no config; a bridge
    // valida o ID e encaminha ao comando privilegiado `launch` do compositor.
    const launch = (id: string) => {
      window.aero.postMessage({ type: "launch-app", id, seq: nextSeq++ });
    };
    menuChannel.onmessage = (event) => {
      if (event.data === "toggle") toggle();
    };
    useEvent((event) => {
      if ("KeyBind" in event && event.KeyBind.keys === "Super+3") toggle();
    });
    watch([open, apps], () => {
      window.aero.postMessage({
        type: "guest-ready", id: window.aeroSurfaceId, framework: "vue",
        menu: open.value ? "open" : "closed", appCount: apps.value.length,
      });
      if (!open.value) return;
      requestAnimationFrame(() => {
        const rects = Array.from(document.querySelectorAll<HTMLElement>(".app"))
          .map((element) => {
            const rect = element.getBoundingClientRect();
            return { x: rect.left, y: rect.top, w: rect.width, h: rect.height };
          })
          .filter((rect) => rect.w > 0 && rect.h > 0);
        window.aero.postMessage({ type: "interactive", rects });
      });
    }, { immediate: true, flush: "post" });
    return () => open.value
      ? h("section", { class: "start-menu", "data-testid": "start-menu" }, [
        h("strong", null, "Iniciar · Vue"),
        ...apps.value.slice(0, 12).map((app) => h(
          "button",
          { class: "app", key: app.id, onClick: () => launch(app.id) },
          app.name,
        )),
      ])
      : null;
  },
});

await mountSurface(StartMenu, { session });
await session.client.setInputRegion([{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }]);
