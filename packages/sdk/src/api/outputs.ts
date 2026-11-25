//! API privilegiada `aero.outputs`; os tipos vêm exclusivamente do ts-rs.

import type { OutputInfo } from "../generated/OutputInfo";
import type { Transport } from "../index";
import type { WindowStore } from "../store";

function parse(lines: string[]): OutputInfo[] {
  return lines.filter((line) => line.startsWith("O ")).map(
    (line) => JSON.parse(line.slice(2)) as OutputInfo,
  );
}

/** Cria o contrato de outputs sobre o IPC privilegiado do runtime. */
export function createOutputsApi(transport: Transport, store: WindowStore) {
  return {
    /** Lista nome, geometria lógica, escala e modo físico de todos os outputs. */
    async list(): Promise<OutputInfo[]> {
      const outputs = parse(await transport("output-list"));
      store.applyOutputs(outputs);
      return outputs;
    },

    /** Obtém um output pelo mesmo índice usado em `SurfaceSpec.output`. */
    async get(id: number): Promise<OutputInfo> {
      if (!Number.isInteger(id) || id < 0) throw new Error("output id deve ser inteiro não negativo");
      const output = parse(await transport(`output-get ${id}`))[0];
      if (!output) throw new Error(`output ${id} não encontrado`);
      return output;
    },

    /** Assina mudanças reativas; o retorno cancela a inscrição. */
    subscribe(listener: () => void): () => void {
      return store.subscribeOutputs(listener);
    },
  };
}
