//! Runtime Bun da política de decoração (F2-decoration-policy-ts).
//! Uso: AERO_WINDOW_IPC_SOCK=<path> [AERO_DECORATION_RULES=<arquivo.ts>] \
//!        [AERO_POLICY_NO_REPLY=1] bun packages/runtime/src/policy.ts
//! Carrega as regras UMA vez no startup (sem hot-reload), anuncia `policy-ready`
//! no mesmo IPC e responde cada `DecorationRequest` do stream PUSH — sem polling.

import { pathToFileURL } from "node:url";

import { parseEvent } from "../../sdk/src/index";
import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";
import { DECORATION_WIRE, decideDecoration } from "./decoration-policy";
import { eventStream, ipcTransport } from "./ipc-client";

const socketPath = process.env.AERO_WINDOW_IPC_SOCK;
if (!socketPath) {
  console.error("FALTA: AERO_WINDOW_IPC_SOCK não definido");
  process.exit(2);
}

const rulesPath = process.env.AERO_DECORATION_RULES;
const config: DecorationPolicyConfig = rulesPath
  ? ((await import(pathToFileURL(rulesPath).href)).default as DecorationPolicyConfig)
  : (await import("./rules")).default;
const noReply = process.env.AERO_POLICY_NO_REPLY !== undefined;

const transport = ipcTransport(socketPath);
const ready = await transport("policy-ready");
console.log(
  `POLICY_TS READY rules=${rulesPath ?? "default"} no_reply=${noReply} ready_reply=${ready.join(",")}`,
);
console.log(
  `POLICY_TS RULES source=${rulesPath ? "edited-ts" : "default"} default=${config.default} rules=${config.rules.length}`,
);
if (noReply) console.log("POLICY_TS NO_REPLY=1");

eventStream(socketPath, (json) => {
  const event = parseEvent(json);
  if (event === null || !("DecorationRequest" in event)) return;
  const request = event.DecorationRequest;
  console.log(`POLICY_TS REQUEST id=${request.id} app_id=${request.app_id}`);
  if (noReply) return;

  const decision = decideDecoration(config, request);
  void (async () => {
    try {
      const reply = await transport(
        `policy ${request.id} ${DECORATION_WIRE[decision.policy]}`,
      );
      console.log(
        `POLICY_TS DECISION id=${request.id} app_id=${request.app_id} ` +
          `policy=${decision.policy} source=${decision.source} reply=${reply.join(",")}`,
      );
    } catch (error) {
      console.log(`POLICY_TS ERROR id=${request.id} ${String(error)}`);
    }
  })();
});
