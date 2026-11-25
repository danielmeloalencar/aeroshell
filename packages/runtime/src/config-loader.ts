//! Startup declarativo: importa desktop.config.tsx e aplica surfaces/frame/rules.

import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import type { DesktopConfig } from "../../sdk/src/generated/DesktopConfig";
import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";
import { createAero, parseEvent } from "../../sdk/src/index";
import { validateConfig } from "./policy-config";
import { eventStream, ipcTransport } from "./ipc-client";

const DEFERRED: Record<string, string> = {
  layouts: "F3", keybindings: "F3", idle: "F5", lock: "F5", greeter: "F6",
};

/** Valida a parte executável na F2 e rejeita todo campo deferido presente. */
export function validateDesktopConfig(value: unknown): DesktopConfig {
  if (typeof value !== "object" || value === null) throw new Error("desktop config deve exportar um objeto");
  const config = value as Record<string, unknown>;
  for (const [field, phase] of Object.entries(DEFERRED)) {
    if (config[field] !== undefined) throw new Error(`${field} é deferido para ${phase}; remova o campo na F2`);
  }
  if (config.surfaces !== undefined && !Array.isArray(config.surfaces)) {
    throw new Error("surfaces deve ser um array");
  }
  if (config.frame !== undefined) {
    const frame = config.frame as Record<string, unknown>;
    if (typeof frame !== "object" || frame === null || typeof frame.enabled !== "boolean") {
      throw new Error("frame.enabled deve ser boolean");
    }
  }
  if (config.rules !== undefined) validateConfig(config.rules);
  return value as DesktopConfig;
}

/** Importa TSX nativamente pelo Bun, preservando imports relativos do dev. */
export async function loadDesktopConfig(path: string): Promise<DesktopConfig> {
  const module = await import(pathToFileURL(path).href) as { default?: unknown };
  return validateDesktopConfig(module.default);
}

/** Converte frame.enabled no fallback SSD; rules.default explícito vence. */
export function desktopRules(config: DesktopConfig): DecorationPolicyConfig {
  const fallback = config.frame?.enabled === false ? "None" : "Server";
  return config.rules ?? { default: fallback, rules: [] };
}

/** Carrega e aplica o config, depois mantém a política viva no mesmo processo. */
export async function startDesktop(path: string): Promise<void> {
  const socketPath = process.env.AERO_WINDOW_IPC_SOCK;
  if (!socketPath) throw new Error("AERO_WINDOW_IPC_SOCK não definido");
  const config = await loadDesktopConfig(path);
  const transport = ipcTransport(socketPath);
  const aero = createAero(transport);
  eventStream(socketPath, (json) => {
    const event = parseEvent(json);
    if (event) aero.store.applyEvent(event);
  });

  for (const declaration of config.surfaces ?? []) {
    const { kind, ...options } = declaration;
    await aero.surfaces.create(kind, options);
    console.log(`CONFIG_SURFACE id=${declaration.id} kind=${kind}`);
  }
  const listed = await aero.surfaces.list();
  console.log(`CONFIG_SURFACES count=${listed.length}`);
  console.log(`CONFIG_LIST ids=${listed.map((surface) => surface.id).sort().join(",")}`);

  // Reusa o runtime de política/hot-reload existente: o config é materializado
  // num módulo TS temporário, sem dev server nem um segundo caminho de regras.
  const dir = await mkdtemp(join("/tmp/opencode", "aero-desktop-rules-"));
  const rulesPath = join(dir, "rules.ts");
  await writeFile(rulesPath, `export default ${JSON.stringify(desktopRules(config))};\n`);
  process.env.AERO_DECORATION_RULES = rulesPath;
  console.log(`CONFIG_RULES default=${desktopRules(config).default} frame=${config.frame?.enabled ?? true}`);
  await import("./policy.ts");
}
