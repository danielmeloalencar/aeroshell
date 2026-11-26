//! Adapter React: contexto, selectors e mount sobre a sessão do SDK.

import {
  createContext, createElement, useContext, useEffect, useSyncExternalStore,
  type ComponentType, type ReactNode,
} from "react";
import { createRoot } from "react-dom/client";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import type { App } from "../../sdk/src/generated/App";
import type { OutputInfo } from "../../sdk/src/generated/OutputInfo";
import type { SurfaceSpec } from "../../sdk/src/generated/SurfaceSpec";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import {
  createSurfaceSession, enableSessionHmr, type SurfaceSession,
} from "../../sdk/src/surface-session";
import { WindowStore, type WorkspacesState } from "../../sdk/src/store";

const AeroContext = createContext<SurfaceSession | null>(null);

type Source = WindowStore | SurfaceSession;
type MountOptions = {
  session?: SurfaceSession;
  target?: HTMLElement;
  hmr?: boolean;
};

function useStore(source?: Source): WindowStore {
  const provided = useContext(AeroContext);
  if (source instanceof WindowStore) return source;
  const session = source ?? provided;
  if (!session) throw new Error("AeroProvider ausente: forneça uma SurfaceSession");
  return session.store;
}

function useSession(source?: SurfaceSession): SurfaceSession {
  const provided = useContext(AeroContext);
  const session = source ?? provided;
  if (!session) throw new Error("AeroProvider ausente: forneça uma SurfaceSession");
  return session;
}

/** Fornece uma sessão à árvore React sem criar outra conexão por hook. */
export function AeroProvider(props: { session: SurfaceSession; children?: ReactNode }): ReactNode {
  return createElement(AeroContext.Provider, { value: props.session }, props.children);
}

/** Janelas vivas do compositor. Aceita store legado ou sessão do provider. */
export function useWindows(source?: Source): WindowInfo[] {
  const store = useStore(source);
  return useSyncExternalStore(
    (listener) => store.subscribe(listener),
    () => store.getSnapshot(),
    () => store.getSnapshot(),
  ).windows;
}

/** Workspaces e ID ativo, atualizados pelos eventos do compositor. */
export function useWorkspaces(source?: Source): WorkspacesState {
  const store = useStore(source);
  return useSyncExternalStore(
    (listener) => store.subscribeWorkspaces(listener),
    () => store.getWorkspaces(),
    () => store.getWorkspaces(),
  );
}

/** Outputs lógicos e físicos disponíveis para posicionar superfícies. */
export function useOutputs(source?: Source): OutputInfo[] {
  const store = useStore(source);
  return useSyncExternalStore(
    (listener) => store.subscribeOutputs(listener),
    () => store.getOutputs(),
    () => store.getOutputs(),
  );
}

/** Aplicativos descobertos nos arquivos `.desktop`. */
export function useApps(source?: Source): App[] {
  const store = useStore(source);
  return useSyncExternalStore(
    (listener) => store.subscribeApps(listener),
    () => store.getApps(),
    () => store.getApps(),
  );
}

/**
 * Executa uma ação local para cada evento; por exemplo, reage a um bind já
 * registrado pelo runtime sem conceder IPC privilegiado à WebView.
 * @example useEvent(event => { if ("KeyBind" in event) setOpen(true); });
 */
export function useEvent(callback: (event: AeroEvent) => void, source?: SurfaceSession): void {
  const session = useSession(source);
  useEffect(() => session.subscribe(callback), [session, callback]);
}

/** A própria superfície, sua spec atual e suas capabilities input/blur. */
export function useSurface(source?: SurfaceSession): {
  id: string;
  spec: SurfaceSpec | undefined;
  setInputRegion: SurfaceSession["client"]["setInputRegion"];
  setBlurRegion: SurfaceSession["client"]["setBlurRegion"];
} {
  const session = useSession(source);
  const surfaces = useSyncExternalStore(
    (listener) => session.store.subscribeSurfaces(listener),
    () => session.store.getSurfaces(),
    () => session.store.getSurfaces(),
  );
  return {
    id: session.id,
    spec: surfaces.find((surface) => surface.id === session.id),
    setInputRegion: session.client.setInputRegion,
    setBlurRegion: session.client.setBlurRegion,
  };
}

/**
 * Monta um componente na superfície já declarada por `defineDesktop`.
 * Não cria layer nem aceita comandos privilegiados. O retorno desmonta tudo.
 * @example await mountSurface(Panel); // no entry da surface declarada
 */
export async function mountSurface(
  Component: ComponentType,
  options: MountOptions = {},
): Promise<() => void> {
  const session = options.session ?? createSurfaceSession();
  const root = createRoot(options.target ?? document.body);
  root.render(createElement(AeroProvider, { session }, createElement(Component)));
  const stopHmr = options.hmr === false ? () => undefined : enableSessionHmr(session);
  try {
    await session.ready();
  } catch (error) {
    stopHmr();
    root.unmount();
    session.dispose();
    throw error;
  }
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    stopHmr();
    root.unmount();
    session.dispose();
  };
}
