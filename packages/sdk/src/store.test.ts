import { expect, test } from "bun:test";

import type { Patch } from "./generated/Patch";
import type { Snapshot } from "./generated/Snapshot";
import type { WindowInfo } from "./generated/WindowInfo";
import { WindowStore } from "./store";

function win(id: number, title: string): WindowInfo {
  return { id, title, x: 0, y: 0, width: 100, height: 50 };
}

test("aplica snapshot e notifica assinantes", () => {
  const store = new WindowStore();
  const snapshot: Snapshot = { revision: 3, windows: [win(1, "foot A")] };
  let notifications = 0;
  const unsubscribe = store.subscribe(() => notifications++);

  store.applySnapshot(snapshot);

  expect(store.getSnapshot()).toEqual(snapshot);
  expect(notifications).toBe(1);
  unsubscribe();
  store.applySnapshot({ revision: 4, windows: [] });
  expect(notifications).toBe(1);
});

test("aplica patch de upsert e de remoção", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });

  store.applyPatch({ revision: 2, change: { Upsert: win(2, "B") } } satisfies Patch);
  expect(store.getSnapshot().revision).toBe(2);
  expect(store.getSnapshot().windows.map((w) => w.id)).toEqual([1, 2]);

  store.applyPatch({ revision: 3, change: { Remove: 1 } } satisfies Patch);
  expect(store.getSnapshot().revision).toBe(3);
  expect(store.getSnapshot().windows.map((w) => w.id)).toEqual([2]);
  expect(store.getSnapshot().windows[0]?.title).toBe("B");
});

test("ignora patch com revisão antiga ou igual", () => {
  const initial: Snapshot = { revision: 5, windows: [win(1, "A")] };
  const store = new WindowStore(initial);
  let notifications = 0;
  store.subscribe(() => notifications++);
  const before = store.getSnapshot();

  store.applyPatch({ revision: 4, change: { Upsert: win(2, "B") } } satisfies Patch);
  store.applyPatch({ revision: 5, change: { Remove: 1 } } satisfies Patch);

  expect(store.getSnapshot()).toBe(before);
  expect(notifications).toBe(0);
});
