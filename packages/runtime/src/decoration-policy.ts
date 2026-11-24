//! F2-decoration-policy-ts: avaliador puro das regras declarativas.
//! Tipos vêm dos bindings gerados por ts-rs em `packages/sdk/src/generated`.

import type { AeroEvent } from "../../sdk/src/generated/AeroEvent";
import type { DecorationPolicy } from "../../sdk/src/generated/DecorationPolicy";
import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";
import type { DecorationRule } from "../../sdk/src/generated/DecorationRule";

export type DecorationRequest = Extract<
  AeroEvent,
  { DecorationRequest: unknown }
>["DecorationRequest"];
export type DecisionSource = "rule" | "default";
export type DecorationDecision = { policy: DecorationPolicy; source: DecisionSource };

/** ts-rs serializa o enum com variantes em PascalCase; o IPC textual usa minúsculas. */
export const DECORATION_WIRE: Record<DecorationPolicy, "server" | "client" | "none"> = {
  Server: "server",
  Client: "client",
  None: "none",
};

/**
 * Wayland não tem WM_CLASS. "Classe" aqui é o app_id completo ou seu último
 * segmento (ex.: `org.gnome.gedit` → `gedit`), case-insensitive.
 */
export function matchesClass(className: string, appId: string): boolean {
  const wanted = className.toLowerCase();
  const last = appId.split(".").pop() ?? appId;
  return wanted === appId.toLowerCase() || wanted === last.toLowerCase();
}

function matchesRule(rule: DecorationRule, request: DecorationRequest): boolean {
  if (rule.app_id !== null && rule.app_id !== request.app_id) return false;
  if (rule.class !== null && !matchesClass(rule.class, request.app_id)) return false;
  return true;
}

/** Política explícita > default. A decisão CSD/default do compositor só vale no fallback. */
export function decideDecoration(
  config: DecorationPolicyConfig,
  request: DecorationRequest,
): DecorationDecision {
  for (const rule of config.rules) {
    if (matchesRule(rule, request)) return { policy: rule.policy, source: "rule" };
  }
  return { policy: config.default, source: "default" };
}
