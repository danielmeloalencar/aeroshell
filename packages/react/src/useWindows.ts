//! Adapter React do SDK. O store é a fonte da verdade; o React só assina.
//! Contrato: docs/plano-mestre.md:57 (useSyncExternalStore).

import { useSyncExternalStore } from "react";

import type { App } from "../../sdk/src/generated/App";
import type { Notification } from "../../sdk/src/generated/Notification";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import type { PanelIndicators, WindowStore, WorkspacesState } from "../../sdk/src/store";

export function useWindows(store: WindowStore): WindowInfo[] {
  const snapshot = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
    () => store.getSnapshot(),
  );
  return snapshot.windows;
}

/** Indicadores por janela (foco/decoração), vindos do stream PUSH. */
export function useIndicators(store: WindowStore): PanelIndicators {
  return useSyncExternalStore(
    (listener) => store.subscribeIndicators(listener),
    () => store.getIndicators(),
    () => store.getIndicators(),
  );
}

export function useApps(store: WindowStore): App[] {
  return useSyncExternalStore(
    (listener) => store.subscribeApps(listener),
    () => store.getApps(),
    () => store.getApps(),
  );
}

export function useNotifications(store: WindowStore): Notification[] {
  return useSyncExternalStore(
    (listener) => store.subscribeNotifications(listener),
    () => store.getNotifications(),
    () => store.getNotifications(),
  );
}

export function useWorkspaces(store: WindowStore): WorkspacesState {
  return useSyncExternalStore(
    (listener) => store.subscribeWorkspaces(listener),
    () => store.getWorkspaces(),
    () => store.getWorkspaces(),
  );
}
