import { expect, mock, test } from "bun:test";

import type { Snapshot } from "../../sdk/src/generated/Snapshot";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";

type Subscribe = (listener: () => void) => () => void;
type GetSnapshot = () => Snapshot;

// Sem DOM no gate: substituímos só useSyncExternalStore para observar como o
// adapter liga o store ao React (subscribe, getSnapshot e unsubscribe).
let notified = 0;
let unsubscribeFromReact: (() => void) | undefined;
let capturedGetSnapshot: GetSnapshot | undefined;
let subscribeCalls = 0;

mock.module("react", () => ({
  useSyncExternalStore: (subscribe: Subscribe, getSnapshot: GetSnapshot) => {
    capturedGetSnapshot = getSnapshot;
    subscribeCalls++;
    unsubscribeFromReact = subscribe(() => {
      notified++;
    });
    return getSnapshot();
  },
}));

const { useWindows } = await import("./useWindows");
const { WindowStore } = await import("../../sdk/src/store");

function win(id: number, title: string): WindowInfo {
  return { id, title, x: 0, y: 0, width: 100, height: 50 };
}

test("useWindows passa subscribe/getSnapshot do store e devolve as janelas", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  subscribeCalls = 0;

  const windows = useWindows(store);

  expect(windows.map((w) => w.id)).toEqual([1]);
  expect(subscribeCalls).toBe(1);
  expect(capturedGetSnapshot).toBeTypeOf("function");
});

test("useWindows acompanha patch revisionado após a notificação", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  useWindows(store);
  notified = 0;

  store.applyPatch({ revision: 2, change: { Upsert: win(2, "B") } });

  expect(notified).toBe(1);
  expect(capturedGetSnapshot?.().revision).toBe(2);
  expect(capturedGetSnapshot?.().windows.map((w) => w.id)).toEqual([1, 2]);
});

test("unsubscribe do hook desliga o listener do store", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  useWindows(store);
  notified = 0;

  unsubscribeFromReact?.();
  store.applyPatch({ revision: 2, change: { Remove: 1 } });

  expect(notified).toBe(0);
  expect(store.getSnapshot().windows.map((w) => w.id)).toEqual([]);
});
