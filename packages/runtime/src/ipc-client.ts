//! Transporte textual para o socket IPC do compositor (protocolo por linha).
//! O codec binário do F0/S4 ainda não existe; não inventar API aqui.

import { createConnection, type Socket } from "node:net";

import type { Transport } from "../../sdk/src/index";

export function ipcTransport(path: string): Transport {
  return (command) =>
    new Promise<string[]>((resolve, reject) => {
      const socket: Socket = createConnection(path);
      const lines: string[] = [];
      let buffer = "";
      let settled = false;
      const waitsForDot = command.trim() === "list";
      const timer = setTimeout(() => finish(new Error(`IPC timeout: ${command}`)), 3000);

      function finish(error?: Error): void {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        socket.destroy();
        if (error) reject(error);
        else resolve(lines);
      }

      socket.setEncoding("utf8");
      socket.on("connect", () => socket.write(`${command}\n`));
      socket.on("error", (error) => finish(error));
      socket.on("close", () => finish(new Error("IPC fechado antes da resposta")));
      socket.on("data", (chunk: string) => {
        buffer += chunk;
        let index: number;
        while ((index = buffer.indexOf("\n")) !== -1) {
          const line = buffer.slice(0, index);
          buffer = buffer.slice(index + 1);
          if (line === "") continue;
          if (line === "." && waitsForDot) {
            finish();
            return;
          }
          if (line.startsWith("ERR")) {
            finish(new Error(line));
            return;
          }
          lines.push(line);
          if (!waitsForDot && line === "OK") {
            finish();
            return;
          }
        }
      });
    });
}
