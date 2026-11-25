import { expect, test } from "bun:test";

import { defineDesktop } from "./define";


test("defineDesktop preserva o contrato completo tipado", () => {
  const config = defineDesktop({
    surfaces: [{ kind: "Desktop", id: "fundo", html: "<body></body>" }],
    frame: { enabled: true },
    rules: { default: "Server", rules: [] },
  });
  expect(config.surfaces?.[0]?.kind).toBe("Desktop");
  expect(config.frame?.enabled).toBe(true);
  expect(config.rules?.default).toBe("Server");
});
