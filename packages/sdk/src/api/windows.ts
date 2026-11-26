//! API privilegiada `aero.windows`: mecanismo no compositor, política no runtime.
//! O guest browser não recebe este módulo nem comandos arbitrários de janelas.

import type { WindowResizeEdge } from "../generated/WindowResizeEdge";
import type { Transport } from "../index";

const EDGE_WIRE: Record<WindowResizeEdge, string> = {
  Top: "top", Bottom: "bottom", Left: "left", Right: "right",
  TopLeft: "top-left", BottomLeft: "bottom-left",
  TopRight: "top-right", BottomRight: "bottom-right",
};

function expectOk(lines: string[]): void {
  if (lines[0] !== "OK") throw new Error(lines.join("\n") || "IPC sem resposta");
}

/** Cria as operações de estado/geometria sobre o IPC privilegiado do runtime. */
export function createWindowsApi(transport: Transport) {
  return {
    /** Esconde a janela sem removê-la do workspace; `focus(id)` a restaura. */
    async minimize(id: number): Promise<void> {
      expectOk(await transport(`minimize ${id}`));
    },

    /** Alterna maximize na work area do output sob a janela. */
    async maximize(id: number): Promise<void> {
      expectOk(await transport(`maximize ${id}`));
    },

    /** Alterna fullscreen no output sob a janela, sem exclusive zones nem frame. */
    async fullscreen(id: number): Promise<void> {
      expectOk(await transport(`fullscreen ${id}`));
    },

    /**
     * Move a origem lógica da janela no Space global.
     * @example await aero.windows.move(windowId, 700, 100); // output 1 em 2×640 px
     */
    async move(id: number, x: number, y: number): Promise<void> {
      expectOk(await transport(`move ${id} ${x} ${y}`));
    },

    /** Pede um novo tamanho lógico ao cliente; Geometry confirma o tamanho aplicado. */
    async resize(id: number, width: number, height: number): Promise<void> {
      expectOk(await transport(`resize ${id} ${width} ${height}`));
    },

    /** Entrega o move ao grab Rust usando o último press válido do seat. */
    async beginMove(id: number): Promise<void> {
      expectOk(await transport(`begin-move ${id}`));
    },

    /** Entrega o resize ao grab Rust pela borda/canto tipado pelo ts-rs. */
    async beginResize(id: number, edge: WindowResizeEdge): Promise<void> {
      expectOk(await transport(`begin-resize ${id} ${EDGE_WIRE[edge]}`));
    },

    /**
     * Captura de thumbnail ainda não existe: screencopy pertence à F7.
     * @throws sempre com o erro explícito do compositor nesta fase.
     */
    async thumbnail(id: number): Promise<never> {
      expectOk(await transport(`thumbnail ${id}`));
      throw new Error("thumbnail deferido para screencopy/F7");
    },
  };
}
