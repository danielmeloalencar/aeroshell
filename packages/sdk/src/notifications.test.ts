import { expect, test } from "bun:test";

import { createAero, parseEvent } from "./index";

test("NotificationShown alimenta o estado e onNotification; dismiss remove por id", () => {
  const aero = createAero(async () => {
    throw new Error("IPC não devia ser chamado ao aplicar eventos");
  });
  const received: string[] = [];
  let changes = 0;
  aero.onNotification((notification) => received.push(notification.id));
  aero.store.subscribeNotifications(() => changes++);
  const notification = {
    id: "n-1",
    title: "Teste",
    body: "Corpo da mensagem",
    icon: null,
    timeout_ms: 5000,
  };

  const shown = parseEvent({ NotificationShown: { notification } });
  expect(shown).not.toBeNull();
  aero.store.applyEvent(shown!);
  aero.store.applyEvent(shown!); // replay/duplicata não emite nem duplica

  expect(aero.store.getNotifications()).toEqual([notification]);
  expect(received).toEqual(["n-1"]);
  expect(changes).toBe(1);

  aero.store.applyEvent(parseEvent({ NotificationDismissed: { id: "n-1" } })!);
  expect(aero.store.getNotifications()).toEqual([]);
  expect(changes).toBe(2);
});

test("showNotification retorna id e envia título, corpo, ícone e timeout pelo IPC", async () => {
  const commands: string[] = [];
  const aero = createAero(async (command) => {
    commands.push(command);
    return ["OK"];
  });

  const first = aero.showNotification("Teste", "Corpo da mensagem");
  const second = aero.showNotification("Longa", "Persiste", "utilities-terminal", 10000);
  aero.dismissNotification(first);
  await Promise.resolve();

  expect(first).toBeString();
  expect(second).toBeString();
  expect(second).not.toBe(first);
  expect(commands[0]).toBe(`notify "Teste" "Corpo da mensagem" null 5000 ${JSON.stringify(first)}`);
  expect(commands[1]).toBe(`notify "Longa" "Persiste" "utilities-terminal" 10000 ${JSON.stringify(second)}`);
  expect(commands[2]).toBe(`dismiss-notification ${JSON.stringify(first)}`);
});
