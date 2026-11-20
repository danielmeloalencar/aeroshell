//! Adapter React do SDK. O store é a fonte da verdade; o React só assina.
//! Contrato: docs/plano-mestre.md:57 (useSyncExternalStore).

import { useSyncExternalStore } from "react";

import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import type { WindowStore } from "../../sdk/src/store";

export function useWindows(store: WindowStore): WindowInfo[] {
  const snapshot = useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
    () => store.getSnapshot(),
  );
  return snapshot.windows;
}
