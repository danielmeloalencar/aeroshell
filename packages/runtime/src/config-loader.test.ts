import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type { SurfaceSpec } from "../../sdk/src/generated/SurfaceSpec";
import { createSurfaceHydrationTracker, desktopRules, diffSurfaceDeclarations, loadDesktopConfig, validateDesktopConfig } from "./config-loader";


test("loader importa desktop.config.tsx do desenvolvedor", async () => {
  const dir = await mkdtemp(join("/tmp/opencode", "aero-config-test-"));
  const define = join(process.cwd(), "packages/sdk/src/define.ts");
  const path = join(dir, "desktop.config.tsx");
  await writeFile(path, `
    import { defineDesktop } from ${JSON.stringify(define)};
    export default defineDesktop({
      surfaces: [{ kind: "Desktop", id: "fundo", html: "<body>ok</body>" }],
      frame: { enabled: false },
      rules: { default: "Client", rules: [] },
    });
  `);
  try {
    const config = await loadDesktopConfig(path);
    expect(config.surfaces?.[0]?.id).toBe("fundo");
    expect(desktopRules(config).default).toBe("Client");
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("reload do config e de seu helper não usa cache de módulo", async () => {
  const dir = await mkdtemp(join("/tmp/opencode", "aero-config-reload-test-"));
  const define = join(process.cwd(), "packages/sdk/src/define.ts");
  const helper = join(dir, "helper.ts");
  const path = join(dir, "desktop.config.tsx");
  await writeFile(helper, "export const text = 'ANTES';\n");
  await writeFile(path, `
    import { defineDesktop } from ${JSON.stringify(define)};
    import { text } from './helper.ts';
    export default defineDesktop({ surfaces: [{ kind: 'Desktop', id: 'fundo', html: text }] });
  `);
  try {
    expect((await loadDesktopConfig(path)).surfaces?.[0]?.html).toBe("ANTES");
    await writeFile(helper, "export const text = 'DEPOIS';\n");
    expect((await loadDesktopConfig(path, 2)).surfaces?.[0]?.html).toBe("DEPOIS");
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("campos F3/F5/F6 são tipados mas falham explicitamente, nunca silenciosos", () => {
  expect(() => validateDesktopConfig({ layouts: {} })).toThrow(/layouts.*F3/);
  expect(() => validateDesktopConfig({ idle: [] })).toThrow(/idle.*F5/);
  expect(() => validateDesktopConfig({ lock: {} })).toThrow(/lock.*F5/);
  expect(() => validateDesktopConfig({ greeter: {} })).toThrow(/greeter.*F6/);
});

test("defineDesktop aceita dois keybindings executáveis na F2", () => {
  const config = validateDesktopConfig({
    keybindings: [
      { keys: "Super+1", command: "workspace-activate 1" },
      { keys: "Super+2", command: "workspace-activate 2" },
    ],
  });
  expect(config.keybindings).toHaveLength(2);
  expect(() => validateDesktopConfig({ keybindings: {} })).toThrow("keybindings deve ser um array");
});

test("diff do config cria, atualiza e destrói por ID sem reiniciar o runtime", () => {
  const previous = [
    { kind: "Desktop" as const, id: "fundo", html: "vermelho" },
    { kind: "Panel" as const, id: "painel", html: "antigo" },
    { kind: "Overlay" as const, id: "antigo", html: "sair" },
  ];
  const next = [
    { kind: "Desktop" as const, id: "fundo", html: "azul" },
    { kind: "Panel" as const, id: "painel", html: "antigo" },
    { kind: "Overlay" as const, id: "novo", html: "entrar" },
  ];
  expect(diffSurfaceDeclarations(previous, next)).toEqual({
    create: [next[2]],
    update: [next[0]],
    destroy: ["antigo"],
  });
  expect(diffSurfaceDeclarations(next, next)).toEqual({ create: [], update: [], destroy: [] });
});

test("update vazio da própria surface pede replay; mudança real não pede", () => {
  const tracker = createSurfaceHydrationTracker();
  const surface = { id: "panel", html: "mesmo" } as SurfaceSpec;
  expect(tracker({ SurfaceCreated: { surface } })).toBe(false);
  expect(tracker({ SurfaceUpdated: { surface } })).toBe(true);
  expect(tracker({ SurfaceUpdated: { surface: { ...surface, html: "mudou" } } })).toBe(false);
  expect(tracker({ Snapshot: { snapshot: { revision: 1, windows: [] } } })).toBe(false);
});

test("frame.enabled controla o fallback SSD e rules.default tem precedência", () => {
  expect(desktopRules({ frame: { enabled: false } }).default).toBe("None");
  expect(desktopRules({ frame: { enabled: true } }).default).toBe("Server");
  expect(desktopRules({
    frame: { enabled: false }, rules: { default: "Client", rules: [] },
  }).default).toBe("Client");
});
