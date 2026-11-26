import { expect, mock, test } from "bun:test";
import * as vueActual from "vue";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import { WindowStore } from "../../sdk/src/store";
import type { SurfaceSession } from "../../sdk/src/surface-session";

let appUnmounts = 0;
const { effectScope } = vueActual;
mock.module("vue", () => ({
  ...vueActual,
  createApp: () => ({
    mount() {},
    unmount() { appUnmounts++; },
  }),
}));

const { mountSurface, useApps, useEvent, useSurface, useWindows, useWorkspaces } = await import("./index");

function fakeSession() {
  const store = new WindowStore();
  const listeners = new Set<(event: AeroEvent) => void>();
  let disposed = 0;
  const session = {
    id: "vue-panel",
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

test("refs/computed atualizam janelas, workspaces, apps e surface", () => {
  const session = fakeSession();
  const scope = effectScope();
  scope.run(() => {
    const windows = useWindows(session);
    const workspaces = useWorkspaces(session);
    const apps = useApps(session);
    const surface = useSurface(session);

    session.emit({ Snapshot: { snapshot: { revision: 1, windows: [{ id: 3, title: "foot", x: 0, y: 0, width: 10, height: 10 }] } } });
    session.emit({ WorkspaceCreated: { workspace: { id: 1, name: "um", window_ids: [3] } } });
    session.emit({ WorkspaceActivated: { id: 1 } });
    session.emit({ AppListUpdated: { apps: [{ id: "foot", name: "Foot", icon: "foot", exec: "foot", categories: [] }] } });

    expect(windows.value.map((window) => window.id)).toEqual([3]);
    expect(workspaces.value.activeId).toBe(1);
    expect(apps.value.map((app) => app.id)).toEqual(["foot"]);
    expect(surface.value.id).toBe("vue-panel");
  });
  scope.stop();
});

test("KeyBind dispara ação e scope.stop cancela", () => {
  const session = fakeSession();
  const actions: string[] = [];
  const scope = effectScope();
  scope.run(() => {
    useEvent((event) => {
      if ("KeyBind" in event) actions.push(event.KeyBind.id);
    }, session);
  });

  session.emit({ KeyBind: { id: "vue-bind", keys: "Super+3" } });
  scope.stop();
  session.emit({ KeyBind: { id: "vue-bind", keys: "Super+3" } });

  expect(actions).toEqual(["vue-bind"]);
});

test("mountSurface desmonta app e sessão", async () => {
  const session = fakeSession();
  const unmount = await mountSurface({ render: () => null }, { session, target: {} as HTMLElement, hmr: false });
  unmount();
  expect(appUnmounts).toBe(1);
  expect(session.disposed).toBe(1);
});
