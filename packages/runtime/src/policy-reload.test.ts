//! F2-decoration-policy-hotreload: teste-first do loader/watcher de regras.
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "bun:test";

import { loadRules, RuleSource, validateConfig } from "./policy-config";

const valid = {
  default: "Server",
  rules: [{ app_id: "aero-hotreload", class: null, policy: "Client" }],
};

function request(appId: string) {
  return {
    id: 0,
    app_id: appId,
    title: `policy-${appId}`,
    mode: "normal",
    client_frame: false,
  };
}

test("validateConfig aceita a config tipada do ts-rs", () => {
  expect(validateConfig(valid)).toEqual(valid);
  void request; // shape do DecorationRequest permanece o mesmo do cartão anterior
});

test("validateConfig rejeita valores inválidos em runtime", () => {
  expect(() => validateConfig(null)).toThrow();
  expect(() => validateConfig({ default: "Bogus", rules: [] })).toThrow(/default/);
  expect(() => validateConfig({ default: "Server", rules: {} })).toThrow(/rules/);
  expect(() =>
    validateConfig({ default: "Server", rules: [{ app_id: null, class: null, policy: "Bogus" }] }),
  ).toThrow(/policy/);
  expect(() =>
    validateConfig({ default: "Server", rules: [{ app_id: 7, class: null, policy: "None" }] }),
  ).toThrow(/app_id/);
  expect(() =>
    validateConfig({ default: "Server", rules: [{ app_id: null, class: 3, policy: "None" }] }),
  ).toThrow(/class/);
  expect(() =>
    validateConfig({ default: "Server", rules: ["not-an-object"] }),
  ).toThrow();
});

test("loadRules recarrega arquivo editado (sem cache de módulo)", async () => {
  const dir = mkdtempSync(join(tmpdir(), "aero-hotreload-"));
  const file = join(dir, "rules.ts");
  writeFileSync(file, "export default { default: 'Server', rules: [{ app_id: 'x', class: null, policy: 'Client' }] };");
  const first = await loadRules(file);
  expect(first.ok).toBe(true);
  if (first.ok) expect(first.config.rules[0]?.policy).toBe("Client");

  writeFileSync(file, "export default { default: 'Server', rules: [{ app_id: 'x', class: null, policy: 'None' }] };");
  const second = await loadRules(file);
  expect(second.ok).toBe(true);
  if (second.ok) expect(second.config.rules[0]?.policy).toBe("None");
});

test("erro de sintaxe mantém as últimas regras boas no RuleSource", async () => {
  const dir = mkdtempSync(join(tmpdir(), "aero-hotreload-"));
  const file = join(dir, "rules.ts");
  writeFileSync(file, "export default { default: 'Server', rules: [{ app_id: 'x', class: null, policy: 'Client' }] };");
  const source = new RuleSource(valid);

  const good = await source.refresh(file);
  expect(good.ok).toBe(true);
  expect(source.config.rules[0]?.policy).toBe("Client");

  writeFileSync(file, "export default { default: ; syntax error }");
  const bad = await source.refresh(file);
  expect(bad.ok).toBe(false);
  expect(source.config.rules[0]?.policy).toBe("Client");

  writeFileSync(file, "export default { default: 'Server', rules: [{ app_id: 'x', class: null, policy: 'None' }] };");
  const repaired = await source.refresh(file);
  expect(repaired.ok).toBe(true);
  expect(source.config.rules[0]?.policy).toBe("None");
});
