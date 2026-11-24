import { expect, test } from "bun:test";

import type { App } from "./generated/App";
import { createAero, parseEvent } from "./index";

const apps: App[] = [
  {
    id: "org.gnome.gedit",
    name: "gedit",
    icon: "org.gnome.gedit",
    exec: "gedit %U",
    categories: ["GNOME", "GTK", "Utility", "TextEditor"],
  },
  {
    id: "org.gnome.Calculator",
    name: "Calculadora",
    icon: "accessories-calculator",
    exec: "gnome-calculator",
    categories: ["GNOME", "Utility"],
  },
];

test("AppListUpdated alimenta listApps e busca fuzzy por nome e categorias", () => {
  const aero = createAero(async () => {
    throw new Error("IPC não devia ser chamado para listar/buscar o cache");
  });
  const event = parseEvent({ AppListUpdated: { apps } });
  expect(event).not.toBeNull();
  aero.store.applyEvent(event!);

  expect(aero.listApps()).toEqual(apps);
  expect(aero.searchApps("ged").map((app) => app.id)).toEqual(["org.gnome.gedit"]);
  expect(aero.searchApps("gdt").map((app) => app.id)).toEqual(["org.gnome.gedit"]);
  expect(aero.searchApps("text").map((app) => app.id)).toEqual(["org.gnome.gedit"]);
  expect(aero.searchApps("calc").map((app) => app.id)).toEqual(["org.gnome.Calculator"]);
  expect(aero.searchApps("app que não existe")).toEqual([]);
  expect(aero.searchApps("")).toEqual(apps);
});

test("launchApp envia app_id e workspace opcional pelo IPC", async () => {
  const commands: string[] = [];
  const aero = createAero(async (command) => {
    commands.push(command);
    return ["OK"];
  });

  expect(aero.launchApp("org.gnome.gedit")).toBeUndefined();
  aero.launchApp("org.gnome.gedit", 2);
  await Promise.resolve();

  expect(commands).toEqual(["launch org.gnome.gedit", "launch org.gnome.gedit 2"]);
});
