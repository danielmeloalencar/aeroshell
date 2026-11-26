import { expect, test } from "bun:test";

import { createSurfaceSession } from "./surface-session";

type Listener = (event: { detail: unknown }) => void;

function fakeWindow(id = "panel") {
  const messages: Record<string, unknown>[] = [];
  const listeners = new Map<string, Set<Listener>>();
  const host = {
    aeroSurfaceId: id,
    __aeroReply: undefined as undefined | ((seq: number, lines: string[]) => void),
    aero: {
      postMessage(message: Record<string, unknown>) {
        messages.push(message);
        queueMicrotask(() => host.__aeroReply?.(Number(message.seq), ["OK"]));
      },
    },
    addEventListener(type: string, listener: Listener) {
      const set = listeners.get(type) ?? new Set<Listener>();
      set.add(listener);
      listeners.set(type, set);
    },
    removeEventListener(type: string, listener: Listener) {
      listeners.get(type)?.delete(listener);
    },
    emit(detail: unknown) {
      for (const listener of [...(listeners.get("aeroEvent") ?? [])]) listener({ detail });
    },
  };
  return { host, messages };
}

test("SurfaceSession aplica o evento antes de notificar e faz cleanup", () => {
  const { host } = fakeWindow();
  const session = createSurfaceSession(host as never);
  const seen: number[][] = [];
  session.subscribe(() => seen.push(session.store.getSnapshot().windows.map((window) => window.id)));

  host.emit({ Snapshot: { snapshot: { revision: 1, windows: [{ id: 7, title: "foot", x: 0, y: 0, width: 640, height: 480 }] } } });
  session.dispose();
  host.emit({ Snapshot: { snapshot: { revision: 2, windows: [] } } });

  expect(seen).toEqual([[7]]);
  expect(session.store.getSnapshot().windows.map((window) => window.id)).toEqual([7]);
});

test("ready emite somente o update vazio da própria surface", async () => {
  const { host, messages } = fakeWindow("blank-panel");
  const session = createSurfaceSession(host as never);

  await session.ready();

  expect(session.id).toBe("blank-panel");
  expect(messages).toHaveLength(1);
  expect(String(messages[0].command)).toBe('surface-update {"id":"blank-panel"}');
  session.dispose();
});
