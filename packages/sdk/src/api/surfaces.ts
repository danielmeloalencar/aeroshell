//! API privilegiada `aero.surfaces` do runtime Bun.
//! Os tipos públicos vêm exclusivamente dos bindings ts-rs; a conversão da wire
//! textual (minúsculas/margins em array) fica isolada neste módulo.

import type { SurfaceAction } from "../generated/SurfaceAction";
import type { SurfaceInputRegion } from "../generated/SurfaceInputRegion";
import type { SurfaceKeyboardMode } from "../generated/SurfaceKeyboardMode";
import type { SurfaceKind } from "../generated/SurfaceKind";
import type { SurfaceLayer } from "../generated/SurfaceLayer";
import type { SurfaceOptions } from "../generated/SurfaceOptions";
import type { SurfaceSpec } from "../generated/SurfaceSpec";
import type { Transport } from "../index";
import type { WindowStore } from "../store";

export type SurfacePatch = Partial<Omit<SurfaceSpec, "id">>;

const KIND_WIRE: Record<SurfaceKind, string> = {
  Desktop: "desktop", Panel: "panel", Overlay: "overlay", Frame: "frame",
};
const LAYER_WIRE: Record<SurfaceLayer, string> = {
  Background: "background", Bottom: "bottom", Top: "top", Overlay: "overlay",
};
const KEYBOARD_WIRE: Record<SurfaceKeyboardMode, string> = {
  None: "none", OnDemand: "ondemand", Exclusive: "exclusive",
};

function pascal(value: unknown): string {
  const text = String(value ?? "");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function expectOk(lines: string[]): void {
  if (lines[0] !== "OK") throw new Error(lines.join("\n") || "IPC sem resposta");
}

/** Converte uma spec tipada no JSON aceito pelo IPC textual existente. */
export function surfaceToWire(spec: SurfaceSpec | (SurfacePatch & { id?: string })) {
  const wire: Record<string, unknown> = { ...spec };
  if (spec.kind !== undefined) wire.kind = KIND_WIRE[spec.kind];
  if (spec.layer !== undefined) wire.layer = LAYER_WIRE[spec.layer];
  if (spec.keyboard_mode !== undefined) wire.keyboard_mode = KEYBOARD_WIRE[spec.keyboard_mode];
  if (spec.margins !== undefined) {
    wire.margins = [spec.margins.left, spec.margins.right, spec.margins.top, spec.margins.bottom];
  }
  return wire;
}

/** Converte o objeto da wire no `SurfaceSpec` gerado pelo ts-rs. */
export function surfaceFromWire(value: unknown): SurfaceSpec {
  if (typeof value !== "object" || value === null) throw new Error("superfície inválida");
  const raw = value as Record<string, unknown>;
  const margins = Array.isArray(raw.margins) ? raw.margins : null;
  const anchors = (raw.anchors ?? {}) as Record<string, unknown>;
  return {
    ...raw,
    kind: pascal(raw.kind) as SurfaceKind,
    layer: pascal(raw.layer) as SurfaceLayer,
    keyboard_mode: pascal(raw.keyboard_mode) as SurfaceKeyboardMode,
    margins: margins
      ? { left: Number(margins[0]), right: Number(margins[1]), top: Number(margins[2]), bottom: Number(margins[3]) }
      : raw.margins,
    anchors: {
      left: Boolean(anchors.left), right: Boolean(anchors.right),
      top: Boolean(anchors.top), bottom: Boolean(anchors.bottom),
    },
    input_region: (raw.input_region ?? null) as SurfaceInputRegion[] | null,
    blur_region: (raw.blur_region ?? null) as SurfaceInputRegion[] | null,
    width: raw.width === undefined ? null : raw.width,
    height: raw.height === undefined ? null : raw.height,
    html: raw.html === undefined ? null : raw.html,
    url: raw.url === undefined ? null : raw.url,
    actions: (raw.actions ?? null) as SurfaceAction[] | null,
  } as SurfaceSpec;
}

export function completeSurface(kind: SurfaceKind, options: SurfaceOptions): SurfaceSpec {
  const desktop = kind === "Desktop";
  const panel = kind === "Panel";
  return {
    id: options.id,
    kind,
    output: options.output ?? 0,
    anchors: options.anchors ?? (desktop || kind === "Overlay"
      ? { left: true, right: true, top: true, bottom: true }
      : { left: true, right: true, top: false, bottom: true }),
    margins: options.margins ?? { left: 0, right: 0, top: 0, bottom: 0 },
    exclusive_zone: options.exclusive_zone ?? (panel ? (options.height ?? 48) : 0),
    layer: options.layer ?? (desktop ? "Background" : panel ? "Top" : "Overlay"),
    keyboard_mode: options.keyboard_mode ?? (desktop ? "None" : "OnDemand"),
    input_region: options.input_region ?? null,
    blur_region: options.blur_region ?? null,
    width: options.width ?? null,
    height: options.height ?? (panel ? 48 : null),
    html: options.html ?? null,
    url: options.url ?? null,
    actions: options.actions ?? null,
  };
}

/** Cria o namespace de superfícies sobre o transporte privilegiado do runtime. */
export function createSurfacesApi(transport: Transport, store: WindowStore) {
  return {
    /**
     * Cria uma superfície web. Somente o runtime Bun possui esta capability.
     * @example await aero.surfaces.create("Desktop", { id: "fundo", html: "<body></body>" })
     */
    async create(kind: SurfaceKind, options: SurfaceOptions): Promise<SurfaceSpec> {
      const spec = completeSurface(kind, options);
      expectOk(await transport(`surface-create ${JSON.stringify(surfaceToWire(spec))}`));
      return spec;
    },

    /** Lista o estado canônico do compositor e reconcilia o store reativo. */
    async list(): Promise<SurfaceSpec[]> {
      const lines = await transport("surface-list");
      const surfaces = lines.filter((line) => line.startsWith("S ")).map(
        (line) => surfaceFromWire(JSON.parse(line.slice(2))),
      );
      store.applySurfaces(surfaces);
      return surfaces;
    },

    /** Atualiza campos declarativos de uma superfície existente. */
    async update(id: string, patch: SurfacePatch): Promise<void> {
      expectOk(await transport(`surface-update ${JSON.stringify(surfaceToWire({ id, ...patch }))}`));
    },

    /** Destrói a superfície e libera sua WebView no host. */
    async destroy(id: string): Promise<void> {
      expectOk(await transport(`surface-destroy ${id}`));
    },

    /** Define os retângulos interativos; `null` deixa a região vazia/pass-through. */
    async setInputRegion(id: string, regions: SurfaceInputRegion[] | null): Promise<void> {
      return this.update(id, { input_region: regions });
    },

    /** Declara a região de blur; o mecanismo visual é ext-background-effect-v1. */
    async setBlurRegion(id: string, regions: SurfaceInputRegion[] | null): Promise<void> {
      return this.update(id, { blur_region: regions });
    },

    /** Altera o modo de teclado da layer sem recarregar seu documento. */
    async setKeyboard(id: string, mode: SurfaceKeyboardMode): Promise<void> {
      return this.update(id, { keyboard_mode: mode });
    },
  };
}
