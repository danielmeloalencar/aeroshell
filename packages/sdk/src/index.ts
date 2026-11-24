//! SDK mínimo: política TS sobre o transporte IPC do compositor.
//! Os tipos de estado vêm de `./generated` (gerados por ts-rs a partir do Rust).

import type { AeroEvent } from "./generated/AeroEvent";
import type { Snapshot } from "./generated/Snapshot";
import type { WindowInfo } from "./generated/WindowInfo";
import type { Workspace } from "./generated/Workspace";
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
  "Snapshot", "Title", "Icon", "Focus", "Geometry", "Csd",
  "FramePainted", "WorkspaceCreated", "WorkspaceActivated", "WorkspaceRenamed",
  "WorkspaceDestroyed", "WindowMovedToWorkspace", "DecorationRequest", "DecorationApplied",
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

export function createAero(transport: Transport) {
  const store = new WindowStore();

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
    listWorkspaces,
    createWorkspace,
    renameWorkspace,
    activateWorkspace,
    moveWindowToWorkspace,
    destroyWorkspace,
    windows: {
      list: async (): Promise<WindowInfo[]> => (await refresh()).windows,
      focus: async (id: number): Promise<void> => {
        await transport(`focus ${id}`);
        await refresh();
      },
      close: async (id: number): Promise<void> => {
        await transport(`close ${id}`);
        await refresh();
      },
    },
  };
}
