//! Runtime de política: ponto de entrada CLI do SDK mínimo (tarefa 03).
//! Uso: AERO_WINDOW_IPC_SOCK=<path> bun packages/runtime/src/main.ts --list
//!                                                                --focus <id>
//!                                                                --close <id>

import { createAero } from "../../sdk/src/index";
import { ipcTransport } from "./ipc-client";

const socketPath = process.env.AERO_WINDOW_IPC_SOCK;
if (!socketPath) {
  console.error("FALTA: AERO_WINDOW_IPC_SOCK não definido");
  process.exit(2);
}

const aero = createAero(ipcTransport(socketPath));
const [command, rawId] = process.argv.slice(2);

if (command === "--list") {
  const windows = await aero.windows.list();
  for (const window of windows) console.log(`${window.id}\t${window.title}`);
} else if (command === "--focus" || command === "--close") {
  const id = Number(rawId);
  if (!Number.isInteger(id)) {
    console.error(`Uso: --${command.slice(2)} <id>`);
    process.exit(2);
  }
  if (command === "--focus") await aero.windows.focus(id);
  else await aero.windows.close(id);
  console.log(`OK: ${command.slice(2)} ${id}`);
} else {
  console.error("Uso: --list | --focus <id> | --close <id>");
  process.exit(2);
}
