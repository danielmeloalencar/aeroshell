import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { desktopRules, loadDesktopConfig, validateDesktopConfig } from "./config-loader";


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

test("campos F3/F5/F6 são tipados mas falham explicitamente, nunca silenciosos", () => {
  expect(() => validateDesktopConfig({ layouts: {} })).toThrow(/layouts.*F3/);
  expect(() => validateDesktopConfig({ keybindings: [] })).toThrow(/keybindings.*F3/);
  expect(() => validateDesktopConfig({ idle: [] })).toThrow(/idle.*F5/);
  expect(() => validateDesktopConfig({ lock: {} })).toThrow(/lock.*F5/);
  expect(() => validateDesktopConfig({ greeter: {} })).toThrow(/greeter.*F6/);
});

test("frame.enabled controla o fallback SSD e rules.default tem precedência", () => {
  expect(desktopRules({ frame: { enabled: false } }).default).toBe("None");
  expect(desktopRules({ frame: { enabled: true } }).default).toBe("Server");
  expect(desktopRules({
    frame: { enabled: false }, rules: { default: "Client", rules: [] },
  }).default).toBe("Client");
});
