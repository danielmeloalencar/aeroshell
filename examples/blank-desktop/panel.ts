//! Entry HMR do painel; em produção o config usa o HTML inline equivalente.
import { createClient } from "../../packages/sdk/src/client";

const client = createClient();
client.subscribe((event) => {
  if ("Focus" in event) document.body.dataset.focus = String(event.Focus.id);
});
client.enableHmr();
document.body.dataset.module = "MODULO_ANTES";
const rect = () => [{ x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }];
await client.update({ input_region: rect(), blur_region: rect() });
document.body.dataset.aeroClient = "ready";
window.aero.postMessage({
  type: "guest-ready",
  id: window.aeroSurfaceId,
  text: document.body.innerText,
  module: document.body.dataset.module,
});

// Fixture de segurança do harness: a decisão de navegação pertence ao host.
if (new URLSearchParams(location.search).has("external-test")) {
  location.href = "https://example.com/";
}
