import type { AeroEvent } from "./generated/AeroEvent";
import type { DecorationPolicy } from "./generated/DecorationPolicy";
import type { Patch } from "./generated/Patch";
import type { Snapshot } from "./generated/Snapshot";
import type { WindowInfo } from "./generated/WindowInfo";

export type Listener = () => void;

/**
 * Indicadores do painel: estado por janela que NÃO altera `WindowInfo` e por
 * isso vive fora do `Snapshot` (o store das janelas continua imutável e os
 * testes antigos intactos). `useSyncExternalStore` exige um objeto estável
 * até haver mudança real — daí o cache em `indicatorsState`.
 */
export type PanelIndicators = {
  focusedId: number | null;
  decorations: Record<number, DecorationPolicy>;
};

/**
 * Estado reativo das janelas. Os tipos vêm dos bindings gerados pelo Rust
 * (`packages/sdk/src/generated`), nunca declarados à mão aqui.
 */
export class WindowStore {
  private state: Snapshot;
  private readonly listeners = new Set<Listener>();
  private indicatorsState: PanelIndicators = { focusedId: null, decorations: {} };
  private readonly indicatorListeners = new Set<Listener>();

  constructor(initial: Snapshot = { revision: 0, windows: [] }) {
    this.state = initial;
  }

  getSnapshot(): Snapshot {
    return this.state;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  getIndicators(): PanelIndicators {
    return this.indicatorsState;
  }

  subscribeIndicators(listener: Listener): () => void {
    this.indicatorListeners.add(listener);
    return () => {
      this.indicatorListeners.delete(listener);
    };
  }

  private setIndicators(next: PanelIndicators): void {
    this.indicatorsState = next;
    for (const listener of this.indicatorListeners) listener();
  }

  applySnapshot(snapshot: Snapshot): void {
    this.state = snapshot;
    this.emit();
  }

  applyPatch(patch: Patch): void {
    // Revisão é monotônica: um patch atrasado não pode desfazer um estado mais novo.
    if (patch.revision <= this.state.revision) return;

    let windows: WindowInfo[];
    if ("Upsert" in patch.change) {
      const window = patch.change.Upsert;
      const index = this.state.windows.findIndex((w) => w.id === window.id);
      windows =
        index === -1
          ? [...this.state.windows, window]
          : this.state.windows.map((w, i) => (i === index ? window : w));
    } else {
      const id = patch.change.Remove;
      windows = this.state.windows.filter((w) => w.id !== id);
    }

    this.state = { revision: patch.revision, windows };
    this.emit();
  }

  /** Evento PUSH do compositor: atualiza o store sem nenhum pedido `list`. */
  applyEvent(event: AeroEvent): void {
    if ("Snapshot" in event) {
      this.applySnapshot(event.Snapshot.snapshot);
      return;
    }
    if ("Focus" in event) {
      const id = event.Focus.id;
      if (this.indicatorsState.focusedId !== id) {
        this.setIndicators({ ...this.indicatorsState, focusedId: id });
      }
      return;
    }
    if ("DecorationApplied" in event) {
      const applied = event.DecorationApplied;
      // event_json() serializa a decisão em minúsculas ("server"); os bindings
      // ts-rs e o tipo DecorationPolicy usam "Server". Normalizar aqui mantém o
      // contrato do SDK sem mudar a wire nem o runtime Bun.
      const raw = applied.decision as string;
      const decision = (raw.charAt(0).toUpperCase() + raw.slice(1)) as DecorationPolicy;
      if (this.indicatorsState.decorations[applied.id] === decision) return;
      this.setIndicators({
        ...this.indicatorsState,
        decorations: { ...this.indicatorsState.decorations, [applied.id]: decision },
      });
      return;
    }
    let id: number | undefined;
    let changes: Partial<WindowInfo> | undefined;
    if ("Title" in event) {
      id = event.Title.id;
      changes = { title: event.Title.title };
    } else if ("Geometry" in event) {
      id = event.Geometry.id;
      changes = {
        x: event.Geometry.x,
        y: event.Geometry.y,
        width: event.Geometry.width,
        height: event.Geometry.height,
      };
    }
    // Icon/Csd/FramePainted e DecorationRequest não alteram o snapshot: só os
    // indicadores (Focus/DecorationApplied) e título/geometria fazem re-render.
    if (id === undefined || changes === undefined) return;
    const existing = this.state.windows.find((window) => window.id === id);
    if (!existing) return;
    this.applyPatch({
      revision: this.state.revision + 1,
      change: { Upsert: { ...existing, ...changes } },
    });
  }

  private emit(): void {
    for (const listener of this.listeners) listener();
  }
}
