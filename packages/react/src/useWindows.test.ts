import { expect, mock, test } from "bun:test";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import type { Snapshot } from "../../sdk/src/generated/Snapshot";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import { WindowStore } from "../../sdk/src/store";
import type { SurfaceSession } from "../../sdk/src/surface-session";

type Subscribe = (listener: () => void) => () => void;
type GetSnapshot = () => Snapshot;

let notified = 0;
let unsubscribeFromReact: (() => void) | undefined;
let capturedGetSnapshot: GetSnapshot | undefined;
let subscribeCalls = 0;
const effectCleanups: Array<() => void> = [];
let unmounts = 0;
let rendered = 0;

// Um único mock evita que arquivos de teste substituam exports um do outro.
mock.module("react", () => ({
  createContext(defaultValue: unknown) {
    const context = {
      value: defaultValue,
      Provider(props: { value: unknown; children?: unknown }) {
        context.value = props.value;
        return props.children;
      },
    };
    return context;
  },
  useContext: (context: { value: unknown }) => context.value,
  useSyncExternalStore: (subscribe: Subscribe, getSnapshot: GetSnapshot) => {
    capturedGetSnapshot = getSnapshot;
    subscribeCalls++;
    unsubscribeFromReact = subscribe(() => { notified++; });
    return getSnapshot();
  },
  useEffect(effect: () => void | (() => void)) {
    const cleanup = effect();
    if (typeof cleanup === "function") effectCleanups.push(cleanup);
  },
  createElement: (type: unknown, props: Record<string, unknown> | null, ...children: unknown[]) => {
    const resolved = { ...props, children: children[0] };
    return typeof type === "function" ? type(resolved) : { type, props: resolved };
  },
}));

mock.module("react-dom/client", () => ({
  createRoot: () => ({
    render() { rendered++; },
    unmount() { unmounts++; },
  }),
}));

const { useWindows: useWindowsFromStore } = await import("./useWindows");
const {
  AeroProvider, mountSurface, useApps, useEvent, useOutputs, useSurface, useWindows, useWorkspaces,
} = await import("./index");

function win(id: number, title: string): WindowInfo {
  return { id, title, x: 0, y: 0, width: 100, height: 50 };
}

function fakeSession() {
  const store = new WindowStore();
  const listeners = new Set<(event: AeroEvent) => void>();
  let disposed = 0;
  const session = {
    id: "panel",
    store,
    subscribe(callback: (event: AeroEvent) => void) {
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    async ready() {},
    dispose() { disposed++; },
    client: { async setInputRegion() {}, async setBlurRegion() {} },
    emit(event: AeroEvent) {
      store.applyEvent(event);
      for (const listener of listeners) listener(event);
    },
    get disposed() { return disposed; },
  };
  return session as typeof session & SurfaceSession;
}

const output = {
  id: 0, name: "winit", scale: 1,
  geometry: { x: 0, y: 0, width: 800, height: 600 },
  mode: { width: 800, height: 600, refresh: 60000 },
};

test("useWindows passa subscribe/getSnapshot do store e devolve as janelas", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  subscribeCalls = 0;
  const windows = useWindowsFromStore(store);
  expect(windows.map((w) => w.id)).toEqual([1]);
  expect(subscribeCalls).toBe(1);
  expect(capturedGetSnapshot).toBeTypeOf("function");
});

test("useWindows acompanha patch revisionado após a notificação", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  useWindowsFromStore(store);
  notified = 0;
  store.applyPatch({ revision: 2, change: { Upsert: win(2, "B") } });
  expect(notified).toBe(1);
  expect(capturedGetSnapshot?.().revision).toBe(2);
  expect(capturedGetSnapshot?.().windows.map((w) => w.id)).toEqual([1, 2]);
});

test("unsubscribe do hook desliga o listener do store", () => {
  const store = new WindowStore({ revision: 1, windows: [win(1, "A")] });
  useWindowsFromStore(store);
  notified = 0;
  unsubscribeFromReact?.();
  store.applyPatch({ revision: 2, change: { Remove: 1 } });
  expect(notified).toBe(0);
  expect(store.getSnapshot().windows.map((w) => w.id)).toEqual([]);
});

test("provider entrega store reativo e todos os selectors do contrato", () => {
  const session = fakeSession();
  AeroProvider({ session, children: null });
  expect(useWindows().map((window) => window.id)).toEqual([]);
  expect(useWorkspaces().activeId).toBeNull();
  expect(useOutputs()).toEqual([]);
  expect(useApps()).toEqual([]);
  expect(useSurface().id).toBe("panel");

  session.emit({ Snapshot: { snapshot: { revision: 1, windows: [win(1, "foot")] } } });
  session.emit({ WorkspaceCreated: { workspace: { id: 1, name: "um", window_ids: [1] } } });
  session.emit({ WorkspaceActivated: { id: 1 } });
  session.emit({ OutputAdded: { output } });
  session.emit({ AppListUpdated: { apps: [{ id: "foot", name: "Foot", icon: "foot", exec: "foot", categories: [] }] } });

  expect(useWindows().map((window) => window.id)).toEqual([1]);
  expect(useWorkspaces().activeId).toBe(1);
  expect(useOutputs().map((item) => item.id)).toEqual([0]);
  expect(useApps().map((item) => item.id)).toEqual(["foot"]);
});

test("useEvent recebe KeyBind e cleanup cancela a ação", () => {
  const session = fakeSession();
  const actions: string[] = [];
  AeroProvider({ session, children: null });
  useEvent((event) => {
    if ("KeyBind" in event) actions.push(event.KeyBind.id);
  });
  session.emit({ KeyBind: { id: "abrir", keys: "Super+1" } });
  for (const cleanup of effectCleanups.splice(0)) cleanup();
  session.emit({ KeyBind: { id: "abrir", keys: "Super+1" } });
  expect(actions).toEqual(["abrir"]);
});

test("mountSurface monta, hidrata e desmonta uma única vez", async () => {
  const session = fakeSession();
  const unmount = await mountSurface(() => null, { session, target: {} as HTMLElement, hmr: false });
  unmount();
  expect(rendered).toBe(1);
  expect(unmounts).toBe(1);
  expect(session.disposed).toBe(1);
});
