//! Client browser sem framework sobre a capability `window.aero` da própria surface.

import type { AeroEvent } from "./generated/AeroEvent";
import type { SurfaceInputRegion } from "./generated/SurfaceInputRegion";
import { parseEvent } from "./index";

type GuestPatch = {
  input_region?: SurfaceInputRegion[] | null;
  blur_region?: SurfaceInputRegion[] | null;
};

type GuestWindow = {
  aeroSurfaceId: string;
  aero: { postMessage(message: Record<string, unknown>): void };
  __aeroReply?: (seq: number, lines: string[]) => void;
  addEventListener(type: string, listener: (event: Event) => void): void;
  removeEventListener(type: string, listener: (event: Event) => void): void;
};

/**
 * Cria o client da WebView atual. Não há create/destroy nem IPC arbitrário:
 * o host vincula a identidade e aceita somente input/blur da própria superfície.
 * Funciona com React, Vue, Svelte, Three.js ou DOM puro, sem adapter.
 *
 * @example
 * const client = createClient();
 * const off = client.subscribe(event => console.log(event));
 * await client.setInputRegion([{ x: 0, y: 0, width: 640, height: 48 }]);
 */
export function createClient(host: GuestWindow = window as unknown as GuestWindow) {
  const surfaceId = host.aeroSurfaceId;
  if (!surfaceId) throw new Error("window.aeroSurfaceId ausente: superfície sem identidade");
  let nextSeq = 1;
  const pending = new Map<number, { resolve(lines: string[]): void }>();
  host.__aeroReply = (seq, lines) => {
    pending.get(seq)?.resolve(lines);
    pending.delete(seq);
  };

  function command(command: string): Promise<string[]> {
    return new Promise((resolve) => {
      const seq = nextSeq++;
      pending.set(seq, { resolve });
      host.aero.postMessage({ type: "ipc", command, seq });
    });
  }

  async function update(patch: GuestPatch): Promise<void> {
    const candidate = patch as Record<string, unknown>;
    if ("id" in candidate) throw new Error("guest só atualiza a própria superfície");
    const invalid = Object.keys(candidate).filter(
      (key) => key !== "input_region" && key !== "blur_region",
    );
    if (invalid.length) {
      throw new Error(`guest só atualiza input_region/blur_region; campo(s): ${invalid.join(",")}`);
    }
    const lines = await command(`surface-update ${JSON.stringify({ id: surfaceId, ...patch })}`);
    if (lines[0] !== "OK") throw new Error(lines.join("\n") || "IPC sem resposta");
  }

  return {
    /** Assina eventos tipados do stream; o retorno cancela a inscrição. */
    subscribe(callback: (event: AeroEvent) => void): () => void {
      const listener = (raw: Event) => {
        const event = parseEvent((raw as CustomEvent).detail);
        if (event) callback(event);
      };
      host.addEventListener("aeroEvent", listener);
      return () => host.removeEventListener("aeroEvent", listener);
    },
    update,
    /** Atualiza os retângulos interativos da própria superfície. */
    setInputRegion(regions: SurfaceInputRegion[] | null): Promise<void> {
      return update({ input_region: regions });
    },
    /** Declara os retângulos de blur da própria superfície. */
    setBlurRegion(regions: SurfaceInputRegion[] | null): Promise<void> {
      return update({ blur_region: regions });
    },
  };
}
