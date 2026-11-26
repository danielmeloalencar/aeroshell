import { expect, test } from "bun:test";

import { completeSurface, surfaceFromWire } from "./surfaces";

test("actions da superfície sobrevivem ao contrato completo e à wire", () => {
  const spec = completeSurface("Overlay", {
    id: "menu", actions: ["LaunchApp"],
  });
  expect(spec.actions).toEqual(["LaunchApp"]);
  expect(surfaceFromWire(JSON.parse(JSON.stringify(spec))).actions).toEqual(["LaunchApp"]);
  expect(completeSurface("Panel", { id: "sem-acao" }).actions).toBeNull();
});
