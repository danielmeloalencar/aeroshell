//! Servidor HMR exclusivo de `--dev`: loopback, bundles browser e WebSocket.
//! Nunca é importado pelo caminho de produção sem que `startDesktop({dev:true})`
//! seja explícito; o host/compositor também precisam de AERO_DEV_ORIGIN.

import { watch, type FSWatcher } from "node:fs";
import { resolve } from "node:path";
import type { ServerWebSocket } from "bun";

import type { SurfaceDeclaration } from "../../sdk/src/generated/SurfaceDeclaration";
import { bundleSurfaceEntry, surfaceDocumentHtml } from "./surface-entry";

type BuiltSurface = {
  declaration: SurfaceDeclaration;
  html: string;
  javascript: string;
  revision: number;
};

export type DevServerOptions = {
  origin: string;
  root: string;
  configPath: string;
  shellResetPath: string;
  sdkClientPath: string;
};

/** Normaliza a única origem HTTP permitida no modo dev. */
export function normalizeDevOrigin(value: string): string {
  const url = new URL(value);
  const port = Number(url.port);
  if (
    url.protocol !== "http:" || url.hostname !== "127.0.0.1"
    || !Number.isInteger(port) || port < 0 || port > 65535
    || url.username || url.password || url.pathname !== "/" || url.search || url.hash
  ) {
    throw new Error("AERO_DEV_ORIGIN deve ser HTTP loopback exato (http://127.0.0.1:<porta>)");
  }
  return `http://127.0.0.1:${port}`;
}

function response(body: string | Uint8Array, contentType: string): Response {
  return new Response(body, {
    headers: { "content-type": contentType, "cache-control": "no-store" },
  });
}

export type DevServer = {
  readonly origin: string;
  /** Reconstrói os entries e troca os documentos atomicamente após sucesso. */
  update(surfaces: SurfaceDeclaration[], revision: number): Promise<void>;
  surfaceUrl(id: string, revision: number): string;
  /** Notifica reload de módulos; mudanças do config seguem pelo diff IPC. */
  publishReload(surfaceIds: string[]): void;
  close(): Promise<void>;
};

/**
 * Sobe o servidor de desenvolvimento. A porta 0 é permitida para testes; o
 * campo `origin` retornado contém sempre a porta real escolhida pelo Bun.
 */
export async function startDevServer(options: DevServerOptions): Promise<DevServer> {
  const requestedOrigin = normalizeDevOrigin(options.origin);
  const requestedPort = Number(new URL(requestedOrigin).port);
  const configPath = resolve(options.configPath);
  const surfaces = new Map<string, BuiltSurface>();
  const sockets = new Set<ServerWebSocket<undefined>>();
  const watchers = new Map<string, FSWatcher>();
  let sdkClient = "";
  let closed = false;
  let rebuilding: Promise<void> | null = null;
  let rebuildAgain = false;
  let reloadTimer: ReturnType<typeof setTimeout> | undefined;

  function publishReload(surfaceIds: string[]): void {
    const message = JSON.stringify({ type: "reload", surfaceIds });
    for (const socket of sockets) socket.send(message);
  }

  async function rebuildAndNotify(): Promise<void> {
    if (closed) return;
    if (rebuilding) {
      rebuildAgain = true;
      return rebuilding;
    }
    rebuilding = (async () => {
      try {
        const current = [...surfaces.values()];
        const declarations = current.map((surface) => surface.declaration);
        const revision = Math.max(0, ...current.map((surface) => surface.revision)) + 1;
        await applyUpdate(declarations, revision);
        console.log(`HMR_BUILD revision=${revision} surfaces=${declarations.map((item) => item.id).join(",")}`);
        publishReload(declarations.map((item) => item.id));
      } catch (error) {
        console.error(`HMR_BUILD_ERROR ${String(error)}`);
      } finally {
        rebuilding = null;
        if (rebuildAgain) {
          rebuildAgain = false;
          await rebuildAndNotify();
        }
      }
    })();
    return rebuilding;
  }

  function scheduleRebuild(): void {
    if (reloadTimer) clearTimeout(reloadTimer);
    reloadTimer = setTimeout(() => void rebuildAndNotify(), 40);
  }

  function syncWatchers(inputs: Set<string>): void {
    for (const [path, watcher] of watchers) {
      if (!inputs.has(path)) {
        watcher.close();
        watchers.delete(path);
      }
    }
    for (const path of inputs) {
      if (path === configPath || path.includes("/node_modules/") || watchers.has(path)) continue;
      try {
        const watcher = watch(path, scheduleRebuild);
        watcher.on("error", (error) => console.error(`HMR_WATCH_ERROR ${path}: ${String(error)}`));
        watchers.set(path, watcher);
      } catch (error) {
        console.error(`HMR_WATCH_ERROR ${path}: ${String(error)}`);
      }
    }
  }

  async function applyUpdate(declarations: SurfaceDeclaration[], revision: number): Promise<void> {
    const next = new Map<string, BuiltSurface>();
    const inputs = new Set<string>();
    for (const declaration of declarations) {
      if (!declaration.dev_entry) continue;
      const { javascript, inputs: dependencies } = await bundleSurfaceEntry(declaration.dev_entry);
      for (const dependency of dependencies) inputs.add(dependency);
      const built: BuiltSurface = {
        declaration: { ...declaration },
        html: "",
        javascript,
        revision,
      };
      const id = encodeURIComponent(declaration.id);
      const script = `<script type="module" src="/assets/${id}.js?v=${revision}"></script>`;
      built.html = surfaceDocumentHtml(
        declaration,
        script,
        '<link rel="stylesheet" href="/shell-reset.css">',
      );
      next.set(declaration.id, built);
    }
    surfaces.clear();
    for (const [id, surface] of next) surfaces.set(id, surface);
    syncWatchers(inputs);
  }

  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: requestedPort,
    development: false,
    async fetch(request, activeServer) {
      const url = new URL(request.url);
      if (url.pathname === "/ws") {
        // Loopback sozinho não impede WebSocket hijacking de uma página externa.
        if (request.headers.get("origin") !== `http://${url.host}`) {
          return new Response("Origin de WebSocket recusada", { status: 403 });
        }
        return activeServer.upgrade(request) ? undefined : new Response("WebSocket esperado", { status: 426 });
      }
      if (url.pathname === "/shell-reset.css") {
        return response(await Bun.file(options.shellResetPath).text(), "text/css; charset=utf-8");
      }
      if (url.pathname.endsWith("/client.js")) {
        if (!sdkClient) sdkClient = (await bundleSurfaceEntry(options.sdkClientPath)).javascript;
        return response(sdkClient, "text/javascript; charset=utf-8");
      }
      const asset = url.pathname.match(/^\/assets\/([^/]+)\.js$/);
      if (asset) {
        const id = decodeURIComponent(asset[1]);
        const surface = surfaces.get(id);
        return surface
          ? response(surface.javascript, "text/javascript; charset=utf-8")
          : new Response("bundle não encontrado", { status: 404 });
      }
      const page = url.pathname.match(/^\/surfaces\/([^/]+)\/$/);
      if (page) {
        const surface = surfaces.get(decodeURIComponent(page[1]));
        return surface
          ? response(surface.html, "text/html; charset=utf-8")
          : new Response("superfície não encontrada", { status: 404 });
      }
      return new Response("rota não encontrada", { status: 404 });
    },
    websocket: {
      open(socket) { sockets.add(socket); },
      close(socket) { sockets.delete(socket); },
    },
  });
  const origin = `http://127.0.0.1:${server.port}`;
  return {
    origin,
    async update(declarations, revision) {
      await applyUpdate(declarations, revision);
    },
    surfaceUrl(id, revision) {
      if (!surfaces.has(id)) throw new Error(`superfície dev sem dev_entry: ${id}`);
      return `${origin}/surfaces/${encodeURIComponent(id)}/?v=${revision}`;
    },
    publishReload,
    async close() {
      closed = true;
      if (reloadTimer) clearTimeout(reloadTimer);
      for (const watcher of watchers.values()) watcher.close();
      watchers.clear();
      for (const socket of sockets) socket.close();
      sockets.clear();
      await server.stop(true);
    },
  };
}
