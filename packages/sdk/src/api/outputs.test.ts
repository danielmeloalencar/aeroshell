import { expect, test } from "bun:test";

import { createAero, parseEvent } from "../index";

const zero = {
  id: 0,
  name: "aero-0",
  geometry: { x: 0, y: 0, width: 640, height: 480 },
  scale: 1,
  mode: { width: 640, height: 480, refresh_millihz: 60_000 },
};
const one = {
  id: 1,
  name: "aero-1",
  geometry: { x: 640, y: 0, width: 640, height: 480 },
  scale: 1.5,
  mode: { width: 960, height: 720, refresh_millihz: 60_000 },
};

test("aero.outputs.list/get leem o estado tipado do compositor", async () => {
  const aero = createAero(async (command) => {
    if (command === "output-list") return [
      `O ${JSON.stringify(zero)}`, `O ${JSON.stringify(one)}`, ".",
    ];
    if (command === "output-get 1") return [`O ${JSON.stringify(one)}`];
    return ["ERR comando inesperado"];
  });
  expect(await aero.outputs.list()).toEqual([zero, one]);
  expect(await aero.outputs.get(1)).toEqual(one);
  expect(aero.store.getOutputs()).toEqual([zero, one]);
});

test("OutputAdded/Changed/Removed atualizam o store reativo sem polling", () => {
  const aero = createAero(async () => ["ERR sem IPC"]);
  let notifications = 0;
  const unsubscribe = aero.store.subscribeOutputs(() => notifications++);
  for (const raw of [
    { OutputAdded: { output: zero } },
    { OutputAdded: { output: one } },
    { OutputChanged: { output: { ...one, scale: 2 } } },
  ]) {
    const event = parseEvent(raw);
    expect(event).not.toBeNull();
    aero.store.applyEvent(event!);
  }
  expect(aero.store.getOutputs()).toEqual([zero, { ...one, scale: 2 }]);
  aero.store.applyEvent(parseEvent({ OutputRemoved: { id: 0 } })!);
  expect(aero.store.getOutputs()).toEqual([{ ...one, scale: 2 }]);
  expect(notifications).toBe(4);
  unsubscribe();
});
