//! Adapter Solid: signals finos sobre o store imutável do SDK.

import {
  createComponent, createContext, createSignal, onCleanup, useContext,
  type Accessor, type Component, type ParentProps,
} from "solid-js";
import { render } from "@solidjs/web";

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import type { App } from "../../sdk/src/generated/App";
import type { OutputInfo } from "../../sdk/src/generated/OutputInfo";
import type { SurfaceSpec } from "../../sdk/src/generated/SurfaceSpec";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import {
  createSurfaceSession, enableSessionHmr, type SurfaceSession,
} from "../../sdk/src/surface-session";
import type { WorkspacesState } from "../../sdk/src/store";

const AeroContext = createContext<SurfaceSession | null>(null);

type MountOptions = { session?: SurfaceSession; target?: HTMLElement; hmr?: boolean };

function sessionOf(source?: SurfaceSession): SurfaceSession {
  const session = source ?? useContext(AeroContext);
  if (!session) throw new Error("AeroProvider ausente: forneça uma SurfaceSession");
  return session;
}

/** Fornece a sessão da surface a todos os accessors Solid descendentes. */
export function AeroProvider(props: ParentProps<{ session: SurfaceSession }>) {
  return createComponent(AeroContext, { value: props.session, children: props.children });
}

/** Accessor das janelas; uma janela aberta produz um novo snapshot. */
export function useWindows(source?: SurfaceSession): Accessor<WindowInfo[]> {
  const session = sessionOf(source);
  const [snapshot, setSnapshot] = createSignal(session.store.getSnapshot());
  onCleanup(session.store.subscribe(() => setSnapshot(session.store.getSnapshot())));
  return () => snapshot().windows;
}

/** Accessor dos workspaces e do workspace ativo. */
export function useWorkspaces(source?: SurfaceSession): Accessor<WorkspacesState> {
  const session = sessionOf(source);
  const [state, setState] = createSignal(session.store.getWorkspaces());
  onCleanup(session.store.subscribeWorkspaces(() => setState(session.store.getWorkspaces())));
  return state;
}

/** Accessor dos outputs disponíveis no compositor. */
export function useOutputs(source?: SurfaceSession): Accessor<OutputInfo[]> {
  const session = sessionOf(source);
  const [outputs, setOutputs] = createSignal(session.store.getOutputs());
  onCleanup(session.store.subscribeOutputs(() => setOutputs(session.store.getOutputs())));
  return outputs;
}

/** Accessor dos aplicativos `.desktop` publicados pelo compositor. */
export function useApps(source?: SurfaceSession): Accessor<App[]> {
  const session = sessionOf(source);
  const [apps, setApps] = createSignal(session.store.getApps());
  onCleanup(session.store.subscribeApps(() => setApps(session.store.getApps())));
  return apps;
}

/** Assina eventos tipados; o owner Solid cancela a inscrição ao ser destruído. */
export function useEvent(callback: (event: AeroEvent) => void, source?: SurfaceSession): void {
  onCleanup(sessionOf(source).subscribe(callback));
}

/** Accessor da própria surface e de suas regiões de input/blur. */
export function useSurface(source?: SurfaceSession): Accessor<{
  id: string;
  spec: SurfaceSpec | undefined;
  setInputRegion: SurfaceSession["client"]["setInputRegion"];
  setBlurRegion: SurfaceSession["client"]["setBlurRegion"];
}> {
  const session = sessionOf(source);
  const [surfaces, setSurfaces] = createSignal(session.store.getSurfaces());
  onCleanup(session.store.subscribeSurfaces(() => setSurfaces(session.store.getSurfaces())));
  return () => ({
    id: session.id,
    spec: surfaces().find((surface) => surface.id === session.id),
    setInputRegion: session.client.setInputRegion,
    setBlurRegion: session.client.setBlurRegion,
  });
}

/**
 * Monta um componente Solid na surface declarada no config e devolve cleanup.
 * @example await mountSurface(Panel);
 */
export async function mountSurface(
  Component: Component,
  options: MountOptions = {},
): Promise<() => void> {
  const session = options.session ?? createSurfaceSession();
  const disposeRender = render(
    () => createComponent(AeroProvider, {
      session,
      children: createComponent(Component, {}),
    }),
    options.target ?? document.body,
  );
  const stopHmr = options.hmr === false ? () => undefined : enableSessionHmr(session);
  try {
    await session.ready();
  } catch (error) {
    stopHmr();
    disposeRender();
    session.dispose();
    throw error;
  }
  let active = true;
  return () => {
    if (!active) return;
    active = false;
    stopHmr();
    disposeRender();
    session.dispose();
  };
}

export type { SurfaceSession } from "../../sdk/src/surface-session";
