import { expect, test } from "bun:test";

import { createClient } from "./client";

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
      for (const listener of listeners.get("aeroEvent") ?? []) listener({ detail });
    },
  };
  return { host, messages };
}

test("createClient assina eventos e atualiza somente input/blur da própria superfície", async () => {
  const { host, messages } = fakeWindow();
  const client = createClient(host as never);
  const events: unknown[] = [];
  const unsubscribe = client.subscribe((event) => events.push(event));

  host.emit({ Focus: { id: 3 } });
  await client.update({
    input_region: [{ x: 0, y: 0, width: 640, height: 56 }],
    blur_region: [{ x: 0, y: 0, width: 640, height: 56 }],
  });
  unsubscribe();
  host.emit({ Focus: { id: 4 } });

  expect(events).toEqual([{ Focus: { id: 3 } }]);
  expect(messages).toHaveLength(1);
  expect(messages[0].type).toBe("ipc");
  const command = String(messages[0].command);
  expect(command.startsWith("surface-update ")).toBe(true);
  expect(JSON.parse(command.slice(15))).toEqual({
    id: "panel",
    input_region: [{ x: 0, y: 0, width: 640, height: 56 }],
    blur_region: [{ x: 0, y: 0, width: 640, height: 56 }],
  });
});

test("createClient rejeita campos privilegiados e não vaza a identidade da superfície", async () => {
  const { host, messages } = fakeWindow("panel-a");
  const client = createClient(host as never);
  await expect(client.update({ html: "não permitido" } as never)).rejects.toThrow("input_region");
  await expect(client.update({ id: "panel-b", input_region: [] } as never)).rejects.toThrow("própria");
  expect(messages).toEqual([]);
});
