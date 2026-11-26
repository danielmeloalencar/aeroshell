//! Adapter Vue: refs/computed finos sobre o store imutável do SDK.

import {
  computed, createApp, defineComponent, h, inject, onScopeDispose, provide,
  shallowRef, type Component, type PropType, type Ref,
} from "vue";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import type { App } from "../../sdk/src/generated/App";
import type { OutputInfo } from "../../sdk/src/generated/OutputInfo";
import type { SurfaceSpec } from "../../sdk/src/generated/SurfaceSpec";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import {
  createSurfaceSession, enableSessionHmr, type SurfaceSession,
} from "../../sdk/src/surface-session";
import type { WorkspacesState } from "../../sdk/src/store";

const aeroKey = Symbol("aeroshell-session");
type MountOptions = { session?: SurfaceSession; target?: HTMLElement; hmr?: boolean };

function sessionOf(source?: SurfaceSession): SurfaceSession {
  const session = source ?? inject<SurfaceSession | null>(aeroKey, null);
  if (!session) throw new Error("AeroProvider ausente: forneça uma SurfaceSession");
  return session;
}

/** Fornece a sessão no `setup()` de um componente Vue. */
export function provideAero(session: SurfaceSession): void {
  provide(aeroKey, session);
}

/** Componente provider para usar com slots em uma árvore Vue existente. */
export const AeroProvider = defineComponent({
  props: { session: { type: Object as PropType<SurfaceSession>, required: true } },
  setup(props, { slots }) {
    provideAero(props.session);
    return () => slots.default?.();
  },
});

function refFromStore<T>(session: SurfaceSession, read: () => T, subscribe: (listener: () => void) => () => void): Ref<T> {
  const value = shallowRef(read()) as Ref<T>;
  onScopeDispose(subscribe(() => { value.value = read(); }));
  return value;
}

/** Ref das janelas; um Snapshot novo substitui o valor reativo. */
export function useWindows(source?: SurfaceSession): Ref<WindowInfo[]> {
  const session = sessionOf(source);
  const snapshot = refFromStore(session, () => session.store.getSnapshot(), (listener) => session.store.subscribe(listener));
  return computed(() => snapshot.value.windows);
}

/** Ref dos workspaces e do ID ativo. */
export function useWorkspaces(source?: SurfaceSession): Ref<WorkspacesState> {
  const session = sessionOf(source);
  return refFromStore(session, () => session.store.getWorkspaces(), (listener) => session.store.subscribeWorkspaces(listener));
}

/** Ref dos outputs publicados pelo compositor. */
export function useOutputs(source?: SurfaceSession): Ref<OutputInfo[]> {
  const session = sessionOf(source);
  return refFromStore(session, () => session.store.getOutputs(), (listener) => session.store.subscribeOutputs(listener));
}

/** Ref dos aplicativos `.desktop` disponíveis para menus. */
export function useApps(source?: SurfaceSession): Ref<App[]> {
  const session = sessionOf(source);
  return refFromStore(session, () => session.store.getApps(), (listener) => session.store.subscribeApps(listener));
}

/** Assina eventos tipados até o effect scope atual ser encerrado. */
export function useEvent(callback: (event: AeroEvent) => void, source?: SurfaceSession): void {
  onScopeDispose(sessionOf(source).subscribe(callback));
}

/** Computed da própria surface e das capabilities input/blur. */
export function useSurface(source?: SurfaceSession) {
  const session = sessionOf(source);
  const surfaces = refFromStore(session, () => session.store.getSurfaces(), (listener) => session.store.subscribeSurfaces(listener));
  return computed(() => ({
    id: session.id,
    spec: surfaces.value.find((surface) => surface.id === session.id),
    setInputRegion: session.client.setInputRegion,
    setBlurRegion: session.client.setBlurRegion,
  }));
}

/**
 * Monta um componente Vue na surface declarada no config e devolve cleanup.
 * @example await mountSurface(Panel);
 */
export async function mountSurface(
  Component: Component,
  options: MountOptions = {},
): Promise<() => void> {
  const session = options.session ?? createSurfaceSession();
  const app = createApp({
    setup() {
      provideAero(session);
      return () => h(Component);
    },
  });
  app.mount(options.target ?? document.body);
  const stopHmr = options.hmr === false ? () => undefined : enableSessionHmr(session);
  try {
    await session.ready();
  } catch (error) {
    stopHmr();
    app.unmount();
    session.dispose();
    throw error;
  }
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    stopHmr();
    app.unmount();
    session.dispose();
  };
}

export type { SurfaceSession } from "../../sdk/src/surface-session";
