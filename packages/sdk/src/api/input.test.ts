import { expect, test } from "bun:test";

import { createAero, parseEvent } from "../index";

test("aero.input.bind/list/dispatch/unbind preserva callback e IDs", async () => {
  const commands: string[] = [];
  let registered: { id: string; keys: string } | undefined;
  const aero = createAero(async (command) => {
    commands.push(command);
    if (command.startsWith("input-bind ")) {
      registered = JSON.parse(command.slice("input-bind ".length));
      return ["OK"];
    }
    if (command === "input-list") {
      return registered ? [`B ${JSON.stringify(registered)}`, "."] : ["."];
    }
    if (command.startsWith("input-unbind ")) {
      registered = undefined;
      return ["OK"];
    }
    return ["ERR comando inesperado"];
  });
  const calls: string[] = [];
  const binding = await aero.input.bind("Super+1", (event) => {
    calls.push(event.id);
  });
  expect(binding.keys).toBe("Super+1");
  expect(await aero.input.list()).toEqual([binding]);
  const event = parseEvent({ KeyBind: { id: binding.id, keys: binding.keys } });
  expect(event).not.toBeNull();
  expect(aero.input.dispatch(event!)).toBe(true);
  expect(aero.input.dispatch(parseEvent({ KeyBind: { id: "outro", keys: "Super+2" } })!)).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(calls).toEqual([binding.id]);
  expect(event).toEqual({ KeyBind: { id: binding.id, keys: "Super+1" } });
  await aero.input.unbind(binding.id);
  expect(await aero.input.list()).toEqual([]);
  expect(aero.input.dispatch(event!)).toBe(false);
  expect(commands[0]).toBe(`input-bind ${JSON.stringify(binding)}`);
});
