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
  blur_region: null,
  width: null,
  height: null,
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

test("aero.surfaces create/list/update/regiões/teclado/destroy usam a wire tipada", async () => {
  const commands: string[] = [];
  const aero = createAero(async (command) => {
    commands.push(command);
    if (command === "surface-list") {
      return ["S " + JSON.stringify({
        ...surface,
        kind: "desktop",
        layer: "background",
        keyboard_mode: "none",
        margins: [0, 0, 0, 0],
        blur_region: null,
        width: null,
        height: null,
      })];
    }
    return ["OK"];
  });

  const created = await aero.surfaces.create("Desktop", {
    id: "fundo",
    html: "<body>fundo</body>",
  });
  expect(created.kind).toBe("Desktop");
  expect(created.anchors).toEqual({ left: true, right: true, top: true, bottom: true });
  expect(JSON.parse(commands[0].slice("surface-create ".length)).kind).toBe("desktop");

  const listed = await aero.surfaces.list();
  expect(listed).toEqual([surface]);

  await aero.surfaces.setInputRegion("fundo", [{ x: 1, y: 2, width: 3, height: 4 }]);
  await aero.surfaces.setBlurRegion("fundo", null);
  await aero.surfaces.setKeyboard("fundo", "Exclusive");
  await aero.surfaces.update("fundo", { exclusive_zone: 48 });
  await aero.surfaces.destroy("fundo");

  expect(commands[2]).toBe("surface-update " + JSON.stringify({
    id: "fundo", input_region: [{ x: 1, y: 2, width: 3, height: 4 }],
  }));
  expect(commands[3]).toBe('surface-update {"id":"fundo","blur_region":null}');
  expect(commands[4]).toBe('surface-update {"id":"fundo","keyboard_mode":"exclusive"}');
  expect(commands[5]).toBe('surface-update {"id":"fundo","exclusive_zone":48}');
  expect(commands[6]).toBe("surface-destroy fundo");
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

  expect(commands[0]).toBe("surface-create " + JSON.stringify({
    ...surface,
    kind: "desktop",
    layer: "background",
    keyboard_mode: "none",
    margins: [0, 0, 0, 0],
  }));
  expect(commands[1]).toContain("surface-update ");
  expect(commands[1]).toContain('"id":"fundo"');
  expect(commands[2]).toBe("surface-destroy fundo");
});
