import { expect, test } from "bun:test";

import { createAero, parseEvent } from "./index";

const surface = {
  id: "fundo",
  kind: "Desktop" as const,
  output: 0,
  anchors: { left: true, right: true, top: true, bottom: true },
  margins: { left: 0, right: 0, top: 0, bottom: 0 },
  exclusive_zone: 0,
  layer: "Background" as const,
  keyboard_mode: "None" as const,
  input_region: null,
  html: "<body></body>",
  url: null,
};

test("eventos Surface* alimentam o store reativo de superfícies", () => {
  const aero = createAero(async () => {
    throw new Error("IPC não devia ser chamado ao aplicar eventos");
  });
  let changes = 0;
  aero.store.subscribeSurfaces(() => changes++);

  aero.store.applyEvent(parseEvent({ SurfaceCreated: { surface } })!);
  expect(aero.store.getSurfaces()).toEqual([surface]);
  expect(changes).toBe(1);

  const moved = { ...surface, exclusive_zone: 48 };
  aero.store.applyEvent(parseEvent({ SurfaceUpdated: { surface: moved } })!);
  expect(aero.store.getSurfaces()).toEqual([moved]);
  expect(changes).toBe(2);

  aero.store.applyEvent(parseEvent({ SurfaceDestroyed: { id: "fundo" } })!);
  expect(aero.store.getSurfaces()).toEqual([]);
  expect(changes).toBe(3);
});

test("createSurface/updateSurface/destroySurface usam comandos IPC tipados", async () => {
  const commands: string[] = [];
  const aero = createAero(async (command) => {
    commands.push(command);
    return ["OK"];
  });

  aero.createSurface(surface);
  aero.updateSurface("fundo", { exclusive_zone: 48 });
  aero.destroySurface("fundo");
  await Promise.resolve();

  expect(commands[0]).toBe("surface-create " + JSON.stringify(surface));
  expect(commands[1]).toContain("surface-update ");
  expect(commands[1]).toContain('"id":"fundo"');
  expect(commands[2]).toBe("surface-destroy fundo");
});
