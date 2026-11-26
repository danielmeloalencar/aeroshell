import { expect, mock, test } from "bun:test";
import { createRoot } from "solid-js";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import { WindowStore } from "../../sdk/src/store";
import type { SurfaceSession } from "../../sdk/src/surface-session";

let mountDisposals = 0;
mock.module("@solidjs/web", () => ({
  render: (code: () => unknown) => {
    let disposeRoot = () => undefined;
    createRoot((dispose) => {
      disposeRoot = dispose;
      code();
    });
    return () => {
      disposeRoot();
      mountDisposals++;
    };
  },
}));

const { mountSurface, useEvent, useOutputs, useSurface, useWindows, useWorkspaces } = await import("./index");

function fakeSession() {
  const store = new WindowStore();
  const listeners = new Set<(event: AeroEvent) => void>();
  let disposed = 0;
  const session = {
    id: "solid-panel",
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

test("signals atualizam janela/workspace/output e própria surface", () => {
  const session = fakeSession();
  createRoot((dispose) => {
    const windows = useWindows(session);
    const workspaces = useWorkspaces(session);
    const outputs = useOutputs(session);
    const surface = useSurface(session);

    session.emit({ Snapshot: { snapshot: { revision: 1, windows: [{ id: 2, title: "foot", x: 0, y: 0, width: 10, height: 10 }] } } });
    session.emit({ WorkspaceCreated: { workspace: { id: 1, name: "um", window_ids: [2] } } });
    session.emit({ WorkspaceActivated: { id: 1 } });
    session.emit({ OutputAdded: { output } });

    expect(windows().map((window) => window.id)).toEqual([2]);
    expect(workspaces().activeId).toBe(1);
    expect(outputs().map((item) => item.id)).toEqual([0]);
    expect(surface().id).toBe("solid-panel");
    dispose();
  });
});

test("KeyBind dispara ação e dispose encerra a assinatura", () => {
  const session = fakeSession();
  const actions: string[] = [];
  createRoot((dispose) => {
    useEvent((event) => {
      if ("KeyBind" in event) actions.push(event.KeyBind.id);
    }, session);
    session.emit({ KeyBind: { id: "solid-bind", keys: "Super+2" } });
    dispose();
    session.emit({ KeyBind: { id: "solid-bind", keys: "Super+2" } });
  });
  expect(actions).toEqual(["solid-bind"]);
});

test("mountSurface não avalia o filho antes do provider; WebKit prova a montagem", async () => {
  const session = fakeSession();
  let evaluations = 0;
  const unmount = await mountSurface(() => {
    evaluations++;
    return null;
  }, { session, target: {} as HTMLElement, hmr: false });
  // O renderer server do Bun não resolve children como o browser. Aqui medimos
  // somente que não houve avaliação ansiosa; o harness f2_adapters_examples
  // exige guest-ready do componente Solid real no WebKitGTK.
  expect(evaluations).toBe(0);
  unmount();
});

test("mountSurface limpa render e sessão", async () => {
  mountDisposals = 0;
  const session = fakeSession();
  const unmount = await mountSurface(() => null, { session, target: {} as HTMLElement, hmr: false });
  unmount();
  expect(mountDisposals).toBe(1);
  expect(session.disposed).toBe(1);
});
