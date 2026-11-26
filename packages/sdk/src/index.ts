//! SDK mínimo: política TS sobre o transporte IPC do compositor.
//! Os tipos de estado vêm de `./generated` (gerados por ts-rs a partir do Rust).

import type { AeroEvent } from "./generated/AeroEvent";
import type { App } from "./generated/App";
import type { Notification } from "./generated/Notification";
import type { Snapshot } from "./generated/Snapshot";
import type { SurfaceSpec } from "./generated/SurfaceSpec";
import type { WindowInfo } from "./generated/WindowInfo";
import type { Workspace } from "./generated/Workspace";
import { createInputApi } from "./api/input";
import { createOutputsApi } from "./api/outputs";
import { createSurfacesApi, surfaceFromWire } from "./api/surfaces";
import { createWindowsApi } from "./api/windows";
import { WindowStore } from "./store";

/** Transporte textual: recebe um comando e devolve as linhas da resposta. */
export type Transport = (command: string) => Promise<string[]>;

/** Converte as linhas `W id x y w h <titulo-json>` do compositor em janelas. */
export function parseWindows(lines: string[]): WindowInfo[] {
  return lines
    .filter((line) => line.startsWith("W "))
    .map((line) => {
      const [id, x, y, width, height] = line.slice(2).split(" ");
      // O título é uma string JSON (pode conter espaços); campos extras do F1
      // (app_id/csd/transient/mode) vêm DEPOIS e não devem entrar no parse.
      const quoted = line.match(/"(?:[^"\\]|\\.)*"/)?.[0];
      return {
        id: Number(id),
        x: Number(x),
        y: Number(y),
        width: Number(width),
        height: Number(height),
        title: quoted ? (JSON.parse(quoted) as string) : "",
      };
    });
}

/** Converte as linhas `WS id active=N windows=... <nome-json>` do IPC. */
export function parseWorkspaces(lines: string[]): Workspace[] {
  return lines.filter((line) => line.startsWith("WS ")).map((line) => {
    const match = line.match(/^WS (\d+) active=[01] windows=([0-9,]*) (".*")$/);
    if (!match) throw new Error(`linha de workspace inválida: ${line}`);
    return {
      id: Number(match[1]),
      name: JSON.parse(match[3]) as string,
      window_ids: match[2] ? match[2].split(",").map(Number) : [],
    };
  });
}

const EVENT_KEYS = [
  "Snapshot", "Title", "Icon", "Focus", "Geometry",
  "Minimized", "Maximized", "Fullscreen", "Csd",
  "FramePainted", "WorkspaceCreated", "WorkspaceActivated", "WorkspaceRenamed",
  "WorkspaceDestroyed", "WindowMovedToWorkspace", "AppListUpdated",
  "OutputAdded", "OutputChanged", "OutputRemoved", "KeyBind",
  "SurfaceCreated", "SurfaceUpdated", "SurfaceDestroyed",
  "NotificationShown", "NotificationDismissed", "DecorationRequest", "DecorationApplied",
] as const;

/**
 * Converte uma linha do stream PUSH no tipo gerado por ts-rs.
 * `bridge.rs:42` injeta o evento como **objeto JS** no `evaluate_javascript`,
 * enquanto o runtime Bun lê uma **string** do socket — aceitar os dois evita
 * que o painel descarte silenciosamente todos os eventos (`JSON.parse` de um
 * objeto vira "[object Object]").
 */
export function parseEvent(raw: string | unknown): AeroEvent | null {
  try {
    const value: unknown = typeof raw === "string" ? JSON.parse(raw) : raw;
    if (typeof value !== "object" || value === null) return null;
    if (!EVENT_KEYS.some((key) => key in value)) return null;
    if ("SurfaceCreated" in value) {
      return { SurfaceCreated: { surface: surfaceFromWire(value.SurfaceCreated.surface) } };
    }
    if ("SurfaceUpdated" in value) {
      return { SurfaceUpdated: { surface: surfaceFromWire(value.SurfaceUpdated.surface) } };
    }
    return value as AeroEvent;
  } catch {
    return null;
  }
}

function sameWindow(a: WindowInfo, b: WindowInfo): boolean {
  return (
    a.id === b.id &&
    a.title === b.title &&
    a.x === b.x &&
    a.y === b.y &&
    a.width === b.width &&
    a.height === b.height
  );
}

function expectOk(lines: string[]): void {
  if (lines[0] !== "OK") throw new Error(lines.join("\n") || "IPC sem resposta");
}

// O ID precisa existir sincronicamente para o retorno da API; o compositor apenas
// o ecoa no evento. Prefixo por instância evita colisão entre painel e scripts Bun.
const notificationPrefix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
let nextNotificationId = 1;

function normalized(value: string): string {
  return value.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLocaleLowerCase();
}

/** Subsequência case/diacritics-insensitive; substring direta tem prioridade. */
function fuzzyScore(query: string, text: string): number | null {
  const needle = normalized(query).trim();
  const haystack = normalized(text);
  if (!needle) return 0;
  const direct = haystack.indexOf(needle);
  if (direct !== -1) return direct;
  let cursor = 0;
  let score = 100;
  for (const char of needle) {
    const found = haystack.indexOf(char, cursor);
    if (found === -1) return null;
    score += found - cursor;
    cursor = found + 1;
  }
  return score;
}

export function createAero(transport: Transport) {
  const store = new WindowStore();
  const surfaces = createSurfacesApi(transport, store);
  const outputs = createOutputsApi(transport, store);
  const input = createInputApi(transport);
  const windowOperations = createWindowsApi(transport);

  /** Busca o estado no compositor e o publica como snapshot (1ª vez) ou patches. */
  async function refresh(): Promise<Snapshot> {
    const remote = parseWindows(await transport("list"));
    if (store.getSnapshot().revision === 0) {
      store.applySnapshot({ revision: 1, windows: remote });
      return store.getSnapshot();
    }

    let revision = store.getSnapshot().revision;
    for (const window of store.getSnapshot().windows) {
      if (!remote.some((w) => w.id === window.id)) {
        store.applyPatch({ revision: ++revision, change: { Remove: window.id } });
      }
    }
    for (const window of remote) {
      const previous = store.getSnapshot().windows.find((w) => w.id === window.id);
      if (!previous || !sameWindow(previous, window)) {
        store.applyPatch({ revision: ++revision, change: { Upsert: window } });
      }
    }
    return store.getSnapshot();
  }

  async function refreshWorkspaces(): Promise<Workspace[]> {
    const lines = await transport("workspace-list");
    if (lines[0]?.startsWith("ERR")) throw new Error(lines.join("\n"));
    const workspaces = parseWorkspaces(lines);
    const activeLine = lines.find((line) => line.startsWith("WS ") && line.includes(" active=1 "));
    const activeId = activeLine ? Number(activeLine.match(/^WS (\d+)/)?.[1]) : null;
    store.applyWorkspaces(workspaces, activeId);
    return workspaces;
  }

  function listApps(): App[] {
    return store.getApps();
  }

  function searchApps(query: string): App[] {
    if (!query.trim()) return listApps();
    return listApps()
      .map((app) => {
        const scores = [app.name, ...app.categories]
          .map((text) => fuzzyScore(query, text))
          .filter((score): score is number => score !== null);
        return { app, score: scores.length ? Math.min(...scores) : null };
      })
      .filter((item): item is { app: App; score: number } => item.score !== null)
      .sort((a, b) => a.score - b.score || a.app.name.localeCompare(b.app.name))
      .map((item) => item.app);
  }

  function launchApp(appId: string, workspaceId?: number): void {
    const command = workspaceId === undefined
      ? `launch ${appId}`
      : `launch ${appId} ${workspaceId}`;
    void transport(command).then(expectOk).catch((error: unknown) => {
      console.error(`falha ao lançar ${appId}:`, error);
    });
  }

  /**
   * Declara uma superfície web (`desktop|panel|overlay|frame`) no host.
   * O conteúdo só pode vir de `aero://` ou de HTML inline confiável.
   *
   * @example
   * aero.createSurface({
   *   id: "fundo", kind: "desktop", output: 0,
   *   anchors: { left: true, right: true, top: true, bottom: true },
   *   margins: { left: 0, right: 0, top: 0, bottom: 0 },
   *   exclusive_zone: 0, layer: "Background", keyboard_mode: "None",
   *   input_region: null, html: "<body style='background:#e00000'></body>", url: null,
   * });
   */
  function createSurface(spec: SurfaceSpec): void {
    const { kind, ...options } = spec;
    void surfaces.create(kind, options).catch(
      (error: unknown) => console.error(`falha ao criar superfície ${spec.id}:`, error),
    );
  }

  /** Atualiza somente os campos informados de uma superfície existente. */
  function updateSurface(id: string, patch: Partial<SurfaceSpec>): void {
    void surfaces.update(id, patch).catch(
      (error: unknown) => console.error(`falha ao atualizar superfície ${id}:`, error),
    );
  }

  /** Destrói uma superfície e libera a WebView associada no host. */
  function destroySurface(id: string): void {
    void surfaces.destroy(id).catch((error: unknown) => {
      console.error(`falha ao destruir superfície ${id}:`, error);
    });
  }

  function showNotification(
    title: string,
    body: string,
    icon?: string,
    timeoutMs?: number,
  ): string {
    const id = `${notificationPrefix}-${nextNotificationId++}`;
    const command = `notify ${JSON.stringify(title)} ${JSON.stringify(body)} `
      + `${JSON.stringify(icon ?? null)} ${timeoutMs ?? 5000} ${JSON.stringify(id)}`;
    void transport(command).then(expectOk).catch((error: unknown) => {
      console.error(`falha ao publicar notificação ${id}:`, error);
    });
    return id;
  }

  function dismissNotification(id: string): void {
    void transport(`dismiss-notification ${JSON.stringify(id)}`).then(expectOk).catch((error: unknown) => {
      console.error(`falha ao dispensar notificação ${id}:`, error);
    });
  }

  function onNotification(callback: (notification: Notification) => void): () => void {
    return store.subscribeNotification(callback);
  }

  async function listWorkspaces(): Promise<Workspace[]> {
    return refreshWorkspaces();
  }

  async function createWorkspace(name?: string): Promise<Workspace> {
    const command = name === undefined
      ? "workspace-create"
      : `workspace-create ${JSON.stringify(name)}`;
    const response = await transport(command);
    const id = Number(response[0]?.match(/^WORKSPACE (\d+)$/)?.[1]);
    if (!Number.isInteger(id)) throw new Error(response.join("\n") || "resposta inválida ao criar workspace");
    const workspace = (await refreshWorkspaces()).find((item) => item.id === id);
    if (!workspace) throw new Error(`workspace ${id} não apareceu após a criação`);
    return workspace;
  }

  async function renameWorkspace(id: number, name: string): Promise<void> {
    expectOk(await transport(`workspace-rename ${id} ${JSON.stringify(name)}`));
    await refreshWorkspaces();
  }

  async function activateWorkspace(id: number): Promise<void> {
    expectOk(await transport(`workspace-activate ${id}`));
    await refreshWorkspaces();
  }

  async function moveWindowToWorkspace(windowId: number, workspaceId: number): Promise<void> {
    expectOk(await transport(`window-workspace ${windowId} ${workspaceId}`));
    await refreshWorkspaces();
  }

  async function destroyWorkspace(id: number): Promise<void> {
    expectOk(await transport(`workspace-destroy ${id}`));
    await refreshWorkspaces();
  }

  return {
    store,
    listApps,
    searchApps,
    launchApp,
    surfaces,
    outputs,
    input,
    createSurface,
    updateSurface,
    destroySurface,
    showNotification,
    dismissNotification,
    onNotification,
    listWorkspaces,
    createWorkspace,
    renameWorkspace,
    activateWorkspace,
    moveWindowToWorkspace,
    destroyWorkspace,
    windows: {
      ...windowOperations,
      /** Lista ID, título e geometria lógica; estados chegam pelo stream/store. */
      list: async (): Promise<WindowInfo[]> => (await refresh()).windows,
      /** Eleva/foca uma janela do workspace ativo e restaura minimize. */
      focus: async (id: number): Promise<void> => {
        await transport(`focus ${id}`);
        await refresh();
      },
      /** Pede ao cliente que feche somente a janela com o ID informado. */
      close: async (id: number): Promise<void> => {
        await transport(`close ${id}`);
        await refresh();
      },
    },
  };
}
