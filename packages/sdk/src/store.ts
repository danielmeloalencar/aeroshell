import type { Patch } from "./generated/Patch";
import type { Snapshot } from "./generated/Snapshot";
import type { WindowInfo } from "./generated/WindowInfo";

export type Listener = () => void;

/**
 * Estado reativo das janelas. Os tipos vêm dos bindings gerados pelo Rust
 * (`packages/sdk/src/generated`), nunca declarados à mão aqui.
 */
export class WindowStore {
  private state: Snapshot;
  private readonly listeners = new Set<Listener>();

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

  private emit(): void {
    for (const listener of this.listeners) listener();
  }
}
