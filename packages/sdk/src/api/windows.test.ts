import { expect, test } from "bun:test";

import { createAero, parseEvent } from "../index";

const initialWindow = { id: 4, title: "A", x: 60, y: 80, width: 500, height: 300 };

test("aero.windows expõe operações IPC sem transformar thumbnail em captura falsa", async () => {
  const commands: string[] = [];
  const aero = createAero(async (command) => {
    commands.push(command);
    if (command === "thumbnail 4") return ["ERR thumbnail deferido para screencopy/F7"];
    if (command === "list") return [
      'W 4 60 80 500 300 "A" "app" csd=0 transient=0 mode=normal', ".",
    ];
    return ["OK"];
  });

  await aero.windows.minimize(4);
  await aero.windows.maximize(4);
  await aero.windows.fullscreen(4);
  await aero.windows.move(4, 700, 90);
  await aero.windows.resize(4, 500, 300);
  await aero.windows.beginMove(4);
  await aero.windows.beginResize(4, "BottomRight");
  await expect(aero.windows.thumbnail(4)).rejects.toThrow("F7");

  expect(commands).toEqual([
    "minimize 4", "maximize 4", "fullscreen 4", "move 4 700 90",
    "resize 4 500 300", "begin-move 4", "begin-resize 4 bottom-right", "thumbnail 4",
  ]);
});

test("Minimized/Maximized/Fullscreen/Geometry atualizam stores reativos sem list", () => {
  const aero = createAero(async () => {
    throw new Error("list não devia ser chamado");
  });
  aero.store.applySnapshot({ revision: 1, windows: [initialWindow] });
  let notifications = 0;
  const unsubscribe = aero.store.subscribeWindowStates(() => notifications++);

  for (const raw of [
    { Minimized: { id: 4, minimized: true } },
    { Maximized: { id: 4, maximized: true } },
    { Fullscreen: { id: 4, fullscreen: true } },
  ]) {
    const event = parseEvent(raw);
    expect(event).not.toBeNull();
    aero.store.applyEvent(event!);
  }
  expect(aero.store.getWindowState(4)).toEqual({
    minimized: true, maximized: true, fullscreen: true,
  });
  expect(notifications).toBe(3);

  aero.store.applyEvent(parseEvent({
    Geometry: { id: 4, x: 648, y: 36, width: 628, height: 352 },
  })!);
  expect(aero.store.getSnapshot().windows[0]).toEqual({
    ...initialWindow, x: 648, y: 36, width: 628, height: 352,
  });

  aero.store.applyEvent(parseEvent({ Minimized: { id: 4, minimized: false } })!);
  expect(aero.store.getWindowState(4).minimized).toBe(false);
  expect(notifications).toBe(4);
  unsubscribe();
});

test("erros IPC das novas operações são propagados", async () => {
  const aero = createAero(async () => ["ERR janela inexistente"]);
  await expect(aero.windows.beginMove(99)).rejects.toThrow("janela inexistente");
});
