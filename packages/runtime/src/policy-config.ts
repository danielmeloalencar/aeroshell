//! F2-decoration-policy-hotreload: loader validado das regras TS.
//! Os tipos vêm dos bindings gerados por ts-rs; em runtime os valores precisam
//! ser conferidos (tipos TS desaparecem depois da transpilação).

import { readFile } from "node:fs/promises";

import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";

export type LoadResult =
  | { ok: true; config: DecorationPolicyConfig }
  | { ok: false; error: string };

const POLICIES = new Set(["Server", "Client", "None"]);

/** Lança `Error` descrevendo o primeiro campo inválido. */
export function validateConfig(value: unknown): DecorationPolicyConfig {
  if (typeof value !== "object" || value === null) {
    throw new Error("config de política não é um objeto");
  }
  const config = value as Record<string, unknown>;
  if (!POLICIES.has(config.default as string)) {
    throw new Error(`default inválido: ${String(config.default)}`);
  }
  if (!Array.isArray(config.rules)) {
    throw new Error("rules não é um array");
  }
  config.rules.forEach((rule, index) => {
    if (typeof rule !== "object" || rule === null) {
      throw new Error(`rules[${index}] não é um objeto`);
    }
    const entry = rule as Record<string, unknown>;
    if (!POLICIES.has(entry.policy as string)) {
      throw new Error(`rules[${index}].policy inválida: ${String(entry.policy)}`);
    }
    for (const key of ["app_id", "class"] as const) {
      const selector = entry[key];
      if (selector !== null && typeof selector !== "string") {
        throw new Error(`rules[${index}].${key} inválido: ${String(selector)}`);
      }
    }
  });
  return value as DecorationPolicyConfig;
}

/**
 * Lê o `.ts`, transpila e importa por `data:` URL: o cache de módulos do Bun
 * 1.4.4 ignora query strings em arquivos locais (medido nesta sessão), então a
 * única forma de ver o conteúdo editado é transpilar o texto lido do disco.
 * `import type` é removido pelo transpilador; se as regras passarem a ter
 * imports de runtime, eles precisam ser resolvidos antes (fora deste cartão).
 * Qualquer falha de leitura/sintaxe/validação vira `{ ok: false }` — nunca uma
 * exceção que derrube o runtime.
 */
export async function loadRules(path: string): Promise<LoadResult> {
  try {
    const source = await readFile(path, "utf8");
    // Bun 1.4.4: um módulo `data:` cujo código começa com `//!` devolve a URL
    // como `default` (medido nesta sessão). Os `//!` são só documentação neste
    // repositório; removê-los não altera as regras.
    const transpiler = new Bun.Transpiler({ loader: "ts" });
    const javascript = transpiler.transformSync(source.replace(/^\/\/!.*$/gm, ""));
    const module = (await import(
      `data:text/javascript;charset=utf-8,${encodeURIComponent(javascript)}`
    )) as { default?: unknown };
    return { ok: true, config: validateConfig(module.default) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

/** Guarda as últimas regras boas: um refresh com erro não altera `config`. */
export class RuleSource {
  private current: DecorationPolicyConfig;

  constructor(initial: DecorationPolicyConfig) {
    this.current = initial;
  }

  get config(): DecorationPolicyConfig {
    return this.current;
  }

  async refresh(path: string): Promise<LoadResult> {
    const result = await loadRules(path);
    if (result.ok) this.current = result.config;
    return result;
  }
}
