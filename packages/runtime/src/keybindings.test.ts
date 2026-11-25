import { expect, test } from "bun:test";

import { diffKeybindings, runKeybindingAction, validateKeybindings } from "./keybindings";

test("diff de keybindings não duplica grabs e remove entradas apagadas", () => {
  const previous = [
    { keys: "Super+1", command: "workspace-activate 1" },
    { keys: "Super+2", command: "workspace-activate 2" },
  ];
  expect(diffKeybindings(previous, previous)).toEqual({ bind: [], unbind: [] });
  expect(diffKeybindings(previous, [previous[0]])).toEqual({
    bind: [], unbind: ["desktop-config-1"],
  });
  const changed = [previous[0], { keys: "Super+3", command: "workspace-activate 2" }];
  expect(diffKeybindings(previous, changed)).toEqual({
    bind: [{ id: "desktop-config-1", ...changed[1] }],
    unbind: ["desktop-config-1"],
  });
});

test("ação da config vive no runtime e chama o SDK tipado", async () => {
  const activated: number[] = [];
  await runKeybindingAction("workspace-activate 2", {
    activateWorkspace: async (id: number) => { activated.push(id); },
  });
  expect(activated).toEqual([2]);
  await expect(runKeybindingAction("shell rm -rf /", {
    activateWorkspace: async () => {},
  })).rejects.toThrow("ação");
});

test("keybindings valida shape e ação; demais campos continuam deferidos", () => {
  expect(() => validateKeybindings([
    { keys: "Super+1", command: "workspace-activate 1" },
    { keys: "Super+2", command: "workspace-activate 2" },
  ])).not.toThrow();
  expect(() => validateKeybindings([{ keys: "", command: "workspace-activate 1" }])).toThrow("keys");
  expect(() => validateKeybindings([{ keys: "Super+1", command: "desconhecida" }])).toThrow("ação");
});
