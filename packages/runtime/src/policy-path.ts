//! F2-shell-real: onde o runtime procura as regras de decoração.
//! Ordem: override explícito (> `AERO_DECORATION_RULES`) > config dir do usuário
//! (`$XDG_CONFIG_HOME/aeroshell/rules.ts` ou `~/.config/aeroshell/rules.ts`) >
//! bundle enviado com o runtime. Assim as regras sobrevivem a restart sem
//! exigir variável de ambiente, e o hot-reload continua no arquivo resolvido.

import { existsSync } from "node:fs";
import { join } from "node:path";

export type RulesSource = "edited-ts" | "configdir" | "default";

export interface ResolvedRules {
  path: string;
  source: RulesSource;
}

/**
 * Função pura sobre o ambiente (não lê `process.env` direto) para ser testável
 * com diretórios temporários. `bundledPath` é injetado pelo chamador.
 */
export function resolveRulesPath(
  env: Record<string, string | undefined>,
  bundledPath: string,
): ResolvedRules {
  const edited = env.AERO_DECORATION_RULES;
  if (edited) return { path: edited, source: "edited-ts" };

  const configHome = env.XDG_CONFIG_HOME
    ? env.XDG_CONFIG_HOME
    : env.HOME
      ? join(env.HOME, ".config")
      : undefined;
  if (configHome) {
    const candidate = join(configHome, "aeroshell", "rules.ts");
    if (existsSync(candidate)) return { path: candidate, source: "configdir" };
  }
  return { path: bundledPath, source: "default" };
}
