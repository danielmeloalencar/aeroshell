//! F2-shell-real: teste-first da resolução do arquivo de regras.
//! Baseline falha por módulo ausente; a implementação vem depois.

import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { resolveRulesPath } from "./policy-path";

function sandbox(): { dir: string; configHome: string; bundled: string } {
  const dir = mkdtempSync(join(tmpdir(), "aero-policy-path-"));
  const configHome = join(dir, "config");
  const bundled = join(dir, "bundled", "rules.ts");
  mkdirSync(join(configHome, "aeroshell"), { recursive: true });
  mkdirSync(join(dir, "bundled"), { recursive: true });
  writeFileSync(bundled, "export default {};\n");
  return { dir, configHome, bundled };
}

test("AERO_DECORATION_RULES vence o config dir (edição ao vivo)", () => {
  const s = sandbox();
  writeFileSync(join(s.configHome, "aeroshell", "rules.ts"), "export default {};\n");
  const edited = join(s.dir, "edited.ts");
  writeFileSync(edited, "export default {};\n");

  const resolved = resolveRulesPath(
    { AERO_DECORATION_RULES: edited, XDG_CONFIG_HOME: s.configHome },
    s.bundled,
  );

  expect(resolved).toEqual({ path: edited, source: "edited-ts" });
});

test("config dir com arquivo é a fonte padrão (persistência)", () => {
  const s = sandbox();
  const rules = join(s.configHome, "aeroshell", "rules.ts");
  writeFileSync(rules, "export default {};\n");

  const resolved = resolveRulesPath({ XDG_CONFIG_HOME: s.configHome }, s.bundled);

  expect(resolved).toEqual({ path: rules, source: "configdir" });
});

test("config dir sem arquivo cai para o bundle", () => {
  const s = sandbox();

  const resolved = resolveRulesPath({ XDG_CONFIG_HOME: s.configHome }, s.bundled);

  expect(resolved).toEqual({ path: s.bundled, source: "default" });
});

test("sem XDG_CONFIG_HOME usa HOME/.config/aeroshell/rules.ts", () => {
  const s = sandbox();
  const home = join(s.dir, "home");
  const rules = join(home, ".config", "aeroshell", "rules.ts");
  mkdirSync(join(home, ".config", "aeroshell"), { recursive: true });
  writeFileSync(rules, "export default {};\n");

  const resolved = resolveRulesPath({ HOME: home }, s.bundled);

  expect(resolved).toEqual({ path: rules, source: "configdir" });
});

test("diretório temporário vazio é ignorado (sem XDG/HOME)", () => {
  const s = sandbox();

  const resolved = resolveRulesPath({}, s.bundled);

  expect(resolved).toEqual({ path: s.bundled, source: "default" });
});
