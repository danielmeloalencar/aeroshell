//! Política declarativa de atalhos: diff, callbacks e ações tipadas do SDK.

import type { KeybindingConfig } from "../../sdk/src/generated/KeybindingConfig";

export type KeyActionAero = {
  activateWorkspace(id: number): Promise<void>;
};

type InputForConfig = {
  bind(
    keys: string,
    callback: (event: { id: string; keys: string }) => void | Promise<void>,
    id?: string,
  ): Promise<unknown>;
  unbind(id: string): Promise<void>;
};

export type KeybindingDiff = {
  bind: Array<KeybindingConfig & { id: string }>;
  unbind: string[];
};

function action(command: string): { workspaceId: number } {
  const parts = command.trim().split(/\s+/);
  if (parts.length === 2 && parts[0] === "workspace-activate") {
    const workspaceId = Number(parts[1]);
    if (Number.isInteger(workspaceId) && workspaceId > 0 && String(workspaceId) === parts[1]) {
      return { workspaceId };
    }
  }
  throw new Error(`ação de keybind desconhecida: ${command}`);
}

/** Executa uma ação permitida; nunca interpreta o comando como shell. */
export async function runKeybindingAction(command: string, aero: KeyActionAero): Promise<void> {
  const parsed = action(command);
  await aero.activateWorkspace(parsed.workspaceId);
}

/** Valida o array completo antes de alterar qualquer grab existente. */
export function validateKeybindings(value: unknown): KeybindingConfig[] {
  if (!Array.isArray(value)) throw new Error("keybindings deve ser um array");
  const keys = new Set<string>();
  return value.map((item, index) => {
    if (typeof item !== "object" || item === null) {
      throw new Error(`keybindings[${index}] deve ser objeto`);
    }
    const candidate = item as Record<string, unknown>;
    if (Object.keys(candidate).some((key) => key !== "keys" && key !== "command")) {
      throw new Error(`keybindings[${index}] contém campo desconhecido`);
    }
    if (typeof candidate.keys !== "string" || !candidate.keys.trim()) {
      throw new Error(`keybindings[${index}].keys deve ser string não vazia`);
    }
    if (typeof candidate.command !== "string" || !candidate.command.trim()) {
      throw new Error(`keybindings[${index}].command deve ser string não vazia`);
    }
    const normalized = candidate.keys.toLocaleLowerCase();
    if (keys.has(normalized)) throw new Error(`keybinding duplicado: ${candidate.keys}`);
    keys.add(normalized);
    action(candidate.command);
    return { keys: candidate.keys, command: candidate.command };
  });
}

export function configBindId(index: number): string {
  return `desktop-config-${index}`;
}

/** Diff posicional estável: uma edição substitui um grab, nunca o duplica. */
export function diffKeybindings(
  previous: KeybindingConfig[],
  next: KeybindingConfig[],
): KeybindingDiff {
  const bind: KeybindingDiff["bind"] = [];
  const unbind: string[] = [];
  for (const [index, item] of next.entries()) {
    const old = previous[index];
    if (JSON.stringify(old) === JSON.stringify(item)) continue;
    const id = configBindId(index);
    if (old) unbind.push(id);
    bind.push({ id, ...item });
  }
  for (let index = next.length; index < previous.length; index++) {
    unbind.push(configBindId(index));
  }
  return { bind, unbind };
}

/** Aplica o diff e instala callbacks que executam a política no mesmo runtime. */
export async function syncKeybindings(
  previous: KeybindingConfig[],
  next: KeybindingConfig[],
  aero: KeyActionAero & { input: InputForConfig },
): Promise<KeybindingDiff> {
  const diff = diffKeybindings(previous, next);
  for (const id of diff.unbind) await aero.input.unbind(id);
  for (const item of diff.bind) {
    await aero.input.bind(item.keys, async () => {
      try {
        await runKeybindingAction(item.command, aero);
        console.log(`KEYBIND_ACTION id=${item.id} command=${item.command} status=OK`);
      } catch (error) {
        console.error(`KEYBIND_ACTION id=${item.id} command=${item.command} status=ERROR ${String(error)}`);
      }
    }, item.id);
  }
  return diff;
}
