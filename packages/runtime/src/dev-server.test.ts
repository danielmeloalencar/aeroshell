import { afterEach, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { normalizeDevOrigin, startDevServer, type DevServer } from "./dev-server";

const servers: DevServer[] = [];
afterEach(async () => {
  for (const server of servers.splice(0)) await server.close();
});

test("origem dev é HTTP loopback exato, sem credenciais/path nem rede externa", () => {
  expect(normalizeDevOrigin("http://127.0.0.1:4173")).toBe("http://127.0.0.1:4173");
  for (const origin of [
    "https://127.0.0.1:4173",
    "http://localhost:4173",
    "http://0.0.0.0:4173",
    "http://example.com:4173",
    "http://user@127.0.0.1:4173",
    "http://127.0.0.1:4173/path",
  ]) {
    expect(() => normalizeDevOrigin(origin)).toThrow("loopback");
  }
});

test("Bun.serve entrega HTML, bundle da superfície, reset e WebSocket de reload", async () => {
  const root = await mkdtemp(join("/tmp/opencode", "aero-dev-server-test-"));
  const configPath = join(root, "desktop.config.tsx");
  const entry = join(root, "panel.tsx");
  await writeFile(configPath, "export default {};\n");
  await writeFile(entry, "document.body.dataset.fromBundle = 'sim';\n");
  try {
    const server = await startDevServer({
      origin: "http://127.0.0.1:0",
      root,
      configPath,
      shellResetPath: join(process.cwd(), "assets/shell-reset.css"),
      sdkClientPath: join(process.cwd(), "packages/sdk/src/client.ts"),
    });
    servers.push(server);
    await server.update([
      { kind: "Panel", id: "panel", html: "<!doctype html><html><head></head><body>texto</body></html>", dev_entry: entry },
    ], 1);
    const pageUrl = server.surfaceUrl("panel", 1);
    const page = await fetch(pageUrl);
    expect(page.status).toBe(200);
    const html = await page.text();
    expect(html).toContain("texto");
    expect(html).toContain("/assets/panel.js?v=1");
    expect(html).toContain("/shell-reset.css");
    const bundle = await fetch(`${server.origin}/assets/panel.js?v=1`);
    expect(bundle.status).toBe(200);
    expect(await bundle.text()).toContain("fromBundle");
    expect((await fetch(`${server.origin}/shell-reset.css`)).status).toBe(200);
    expect((await fetch(`${server.origin}/fora`)).status).toBe(404);
    expect((await fetch(`${server.origin}/ws`, {
      headers: { Origin: "https://example.com" },
    })).status).toBe(403);

    const received = new Promise<Record<string, unknown>>((resolve, reject) => {
      const socket = new WebSocket(`${server.origin.replace("http:", "ws:")}/ws`, {
        headers: { Origin: server.origin },
      });
      socket.onmessage = (event) => {
        resolve(JSON.parse(String(event.data)));
        socket.close();
      };
      socket.onerror = () => reject(new Error("WebSocket HMR falhou"));
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    server.publishReload(["panel"]);
    expect(await received).toEqual({ type: "reload", surfaceIds: ["panel"] });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
