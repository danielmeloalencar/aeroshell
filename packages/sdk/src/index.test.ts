import { expect, test } from "bun:test";

import { createAero, parseWindows } from "./index";

test("parseWindows decodifica id, geometria e título JSON com espaços", () => {
  const windows = parseWindows([
    'W 0 10 20 800 600 "foot A"',
    'W 1 0 0 640 480 "user@host: ~"',
    ".",
  ]);

  expect(windows).toEqual([
    { id: 0, title: "foot A", x: 10, y: 20, width: 800, height: 600 },
    { id: 1, title: "user@host: ~", x: 0, y: 0, width: 640, height: 480 },
  ]);
});

test("list publica snapshot; close publica patch e remove só aquela janela", async () => {
  const commands: string[] = [];
  let listed = [
    'W 0 0 0 800 600 "A"',
    'W 1 100 0 640 480 "B"',
    ".",
  ];
  const aero = createAero(async (command) => {
    commands.push(command);
    if (command === "close 1") listed = ['W 0 0 0 800 600 "A"', "."];
    return listed;
  });

  expect((await aero.windows.list()).map((w) => w.id)).toEqual([0, 1]);
  expect(aero.store.getSnapshot().revision).toBe(1);

  await aero.windows.close(1);

  expect(commands).toEqual(["list", "close 1", "list"]);
  expect(aero.store.getSnapshot().windows.map((w) => w.id)).toEqual([0]);
  expect(aero.store.getSnapshot().revision).toBe(2);
});
