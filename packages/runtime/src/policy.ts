//! Runtime Bun da política de decoração (F2-decoration-policy-ts + hot-reload).
//! Uso: AERO_WINDOW_IPC_SOCK=<path> [AERO_DECORATION_RULES=<arquivo.ts>] \
//!        [AERO_POLICY_NO_REPLY=1] bun packages/runtime/src/policy.ts
//! Anuncia `policy-ready` no mesmo IPC e responde cada `DecorationRequest` do
//! stream PUSH — sem polling. `fs.watch` no arquivo de regras reparseia e pede
//! ao compositor que reemita os pedidos; regras inválidas são logadas e as
//! últimas boas permanecem em uso.

import { watch } from "node:fs";
import { basename, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import { parseEvent } from "../../sdk/src/index";
import { DECORATION_WIRE, decideDecoration } from "./decoration-policy";
import { eventStream, ipcTransport } from "./ipc-client";
import { RuleSource, loadRules } from "./policy-config";

const socketPath = process.env.AERO_WINDOW_IPC_SOCK;
if (!socketPath) {
  console.error("FALTA: AERO_WINDOW_IPC_SOCK não definido");
  process.exit(2);
}

const rulesEnv = process.env.AERO_DECORATION_RULES;
const rulesPath = rulesEnv ?? fileURLToPath(new URL("./rules.ts", import.meta.url));
const initial = await loadRules(rulesPath);
if (!initial.ok) {
  console.error(`POLICY_TS RULES ERROR source=${rulesEnv ? "edited-ts" : "default"} ${initial.error}`);
  process.exit(2);
}
const source = new RuleSource(initial.config);
const noReply = process.env.AERO_POLICY_NO_REPLY !== undefined;

const transport = ipcTransport(socketPath);
const ready = await transport("policy-ready");
console.log(
  `POLICY_TS READY rules=${rulesEnv ?? "default"} no_reply=${noReply} ready_reply=${ready.join(",")}`,
);
console.log(
  `POLICY_TS RULES source=${rulesEnv ? "edited-ts" : "default"} default=${source.config.default} rules=${source.config.rules.length}`,
);
if (noReply) console.log("POLICY_TS NO_REPLY=1");

let generation = 0;
let reloadTimer: ReturnType<typeof setTimeout> | undefined;

async function reload(): Promise<void> {
  const current = ++generation;
  const result = await source.refresh(rulesPath);
  if (current !== generation) return; // edição mais recente vence
  const label = `source=${rulesEnv ? "edited-ts" : "default"}`;
  if (!result.ok) {
    console.log(`POLICY_TS RELOAD ERROR ${label} ${result.error}`);
    return;
  }
  console.log(
    `POLICY_TS RELOAD ${label} default=${source.config.default} rules=${source.config.rules.length}`,
  );
  try {
    const reply = await transport("policy-reload");
    console.log(`POLICY_TS RELOAD_DISPATCH reply=${reply.join(",")}`);
  } catch (error) {
    console.log(`POLICY_TS RELOAD ERROR dispatch: ${String(error)}`);
  }
}

const targetName = basename(rulesPath);
const watcher = watch(dirname(rulesPath), (_event, filename) => {
  if (filename === null || basename(String(filename)) !== targetName) return;
  if (reloadTimer) clearTimeout(reloadTimer);
  reloadTimer = setTimeout(() => {
    void reload();
  }, 80);
});
watcher.on("error", (error) => {
  console.log(`POLICY_TS RELOAD ERROR watch: ${String(error)}`);
});
console.log(`POLICY_TS WATCH file=${targetName}`);

eventStream(socketPath, (json) => {
  const event = parseEvent(json);
  if (event === null || !("DecorationRequest" in event)) return;
  const request = event.DecorationRequest;
  console.log(`POLICY_TS REQUEST id=${request.id} app_id=${request.app_id}`);
  if (noReply) return;

  const decision = decideDecoration(source.config, request);
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
