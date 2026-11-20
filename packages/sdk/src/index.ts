//! SDK mínimo: política TS sobre o transporte IPC do compositor.
//! Os tipos de estado vêm de `./generated` (gerados por ts-rs a partir do Rust).

import type { Snapshot } from "./generated/Snapshot";
import type { WindowInfo } from "./generated/WindowInfo";
import { WindowStore } from "./store";

/** Transporte textual: recebe um comando e devolve as linhas da resposta. */
export type Transport = (command: string) => Promise<string[]>;

/** Converte as linhas `W id x y w h <titulo-json>` do compositor em janelas. */
export function parseWindows(lines: string[]): WindowInfo[] {
  return lines
    .filter((line) => line.startsWith("W "))
    .map((line) => {
      const [id, x, y, width, height, ...titleParts] = line.slice(2).split(" ");
      return {
        id: Number(id),
        x: Number(x),
        y: Number(y),
        width: Number(width),
        height: Number(height),
        title: JSON.parse(titleParts.join(" ")) as string,
      };
    });
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

  return {
    store,
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
