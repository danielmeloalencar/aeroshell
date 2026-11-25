//! API privilegiada `aero.input`: grabs globais e callbacks do runtime.
//! O guest browser não recebe este módulo nem esta capability.

import type { AeroEvent } from "../generated/AeroEvent";
import type { KeyBind } from "../generated/KeyBind";
import type { Transport } from "../index";

export type KeyBindCallback = (event: KeyBind) => void | Promise<void>;

let sequence = 1;
const prefix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

function expectOk(lines: string[]): void {
  if (lines[0] !== "OK") throw new Error(lines.join("\n") || "IPC sem resposta");
}

/** Cria o mecanismo de binds; a escolha das ações permanece no chamador TS. */
export function createInputApi(transport: Transport) {
  const callbacks = new Map<string, KeyBindCallback>();
  return {
    /**
     * Registra um grab global e despacha seu `KeyBind` ao callback.
     * @returns o bind tipado, cujo `id` pode ser passado a `unbind`.
     */
    async bind(keys: string, callback: KeyBindCallback, id = `sdk-${prefix}-${sequence++}`): Promise<KeyBind> {
      if (typeof callback !== "function") throw new Error("callback de keybind deve ser função");
      const binding = { id, keys };
      expectOk(await transport(`input-bind ${JSON.stringify(binding)}`));
      callbacks.set(id, callback);
      return binding;
    },

    /** Remove o grab global e o callback local de forma idempotente no runtime. */
    async unbind(id: string): Promise<void> {
      expectOk(await transport(`input-unbind ${JSON.stringify(id)}`));
      callbacks.delete(id);
    },

    /** Lista o estado canônico de grabs registrado no compositor. */
    async list(): Promise<KeyBind[]> {
      return (await transport("input-list")).filter((line) => line.startsWith("B ")).map(
        (line) => JSON.parse(line.slice(2)) as KeyBind,
      );
    },

    /** Despacha somente o evento ao dono do ID; outros runtimes não executam nada. */
    dispatch(event: AeroEvent): boolean {
      if (!("KeyBind" in event)) return false;
      const callback = callbacks.get(event.KeyBind.id);
      if (!callback) return false;
      void Promise.resolve(callback(event.KeyBind)).catch((error: unknown) => {
        console.error(`KEYBIND_CALLBACK_ERROR id=${event.KeyBind.id}: ${String(error)}`);
      });
      return true;
    },
  };
}
