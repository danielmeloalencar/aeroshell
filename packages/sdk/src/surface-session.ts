//! Sessão reativa da WebView atual, compartilhada pelos adapters de framework.

import type { AeroEvent } from "./generated/AeroEvent";
import { createClient } from "./client";
import { WindowStore } from "./store";

export type SurfaceClient = ReturnType<typeof createClient>;

/** Store, eventos e capability restrita de uma única superfície. */
export type SurfaceSession = {
  readonly id: string;
  readonly store: WindowStore;
  readonly client: SurfaceClient;
  subscribe(callback: (event: AeroEvent) => void): () => void;
  ready(): Promise<void>;
  dispose(): void;
};

/**
 * Cria uma sessão sem framework para `window.aeroSurfaceId`. Todos os hooks de
 * uma árvore devem compartilhar a mesma instância, evitando listeners duplicados.
 *
 * @example
 * const session = createSurfaceSession();
 * const off = session.subscribe(event => console.log(event));
 * await session.ready(); // o runtime responde com o replay do estado atual
 * off();
 * session.dispose();
 */
export function createSurfaceSession(host?: Parameters<typeof createClient>[0]): SurfaceSession {
  const client = createClient(host);
  const store = new WindowStore();
  const listeners = new Set<(event: AeroEvent) => void>();
  let disposed = false;
  const unsubscribe = client.subscribe((event) => {
    if (disposed) return;
    store.applyEvent(event);
    for (const listener of [...listeners]) listener(event);
  });

  return {
    id: client.id,
    store,
    client,
    subscribe(callback) {
      if (disposed) return () => undefined;
      listeners.add(callback);
      return () => { listeners.delete(callback); };
    },
    async ready() {
      if (!disposed) await client.update({});
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      listeners.clear();
      unsubscribe();
    },
  };
}

/**
 * Ativa HMR somente num documento HTTP do dev-server; em `aero://` é um no-op.
 * @returns Função idempotente que fecha o WebSocket de desenvolvimento.
 */
export function enableSessionHmr(session: SurfaceSession): () => void {
  if (typeof location === "undefined" || location.protocol !== "http:") return () => undefined;
  const close = session.client.enableHmr();
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    close();
  };
}
