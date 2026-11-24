import { expect, test } from "bun:test";

import type { Workspace } from "./generated/Workspace";
import { createAero, parseEvent, parseWorkspaces } from "./index";

const initial = [
  'WS 1 active=1 windows=0 "Workspace 1"',
  ".",
];

test("parseWorkspaces decodifica nome com espaços, ativa e janelas", () => {
  expect(parseWorkspaces([
    'WS 1 active=0 windows= "Workspace 1"',
    'WS 2 active=1 windows=0,4,9 "Projetos pessoais"',
    ".",
  ])).toEqual([
    { id: 1, name: "Workspace 1", window_ids: [] },
    { id: 2, name: "Projetos pessoais", window_ids: [0, 4, 9] },
  ]);
});

test("API pública cria, renomeia, ativa, move e destrói workspaces", async () => {
  const commands: string[] = [];
  let listed = initial;
  let created = false;
  const aero = createAero(async (command) => {
    commands.push(command);
    if (command === 'workspace-create "Projeto Aero"') {
      created = true;
      listed = [...initial.slice(0, -1), 'WS 2 active=0 windows= "Projeto Aero"', "."];
      return ["WORKSPACE 2"];
    }
    if (command === 'workspace-rename 2 "Aero Shell"') {
      listed = [initial[0], 'WS 2 active=0 windows= "Aero Shell"', "."];
      return ["OK"];
    }
    if (command === "workspace-activate 2") {
      listed = ['WS 1 active=0 windows=0 "Workspace 1"', 'WS 2 active=1 windows= "Aero Shell"', "."];
      return ["OK"];
    }
    if (command === "window-workspace 0 2") {
      listed = ['WS 1 active=0 windows= "Workspace 1"', 'WS 2 active=1 windows=0 "Aero Shell"', "."];
      return ["OK"];
    }
    if (command === "workspace-destroy 2") {
      listed = ['WS 1 active=1 windows=0 "Workspace 1"', "."];
      return ["OK"];
    }
    if (command === "workspace-list") return listed;
    throw new Error(`comando inesperado: ${command}`);
  });

  expect((await aero.listWorkspaces()).map((workspace) => workspace.id)).toEqual([1]);
  const workspace: Workspace = await aero.createWorkspace("Projeto Aero");
  expect(workspace).toEqual({ id: 2, name: "Projeto Aero", window_ids: [] });
  expect(created).toBe(true);
  await aero.renameWorkspace(2, "Aero Shell");
  await aero.activateWorkspace(2);
  expect(aero.store.getWorkspaces().activeId).toBe(2);
  await aero.moveWindowToWorkspace(0, 2);
  expect(aero.store.getWorkspaces().workspaces[1]?.window_ids).toEqual([0]);
  await aero.destroyWorkspace(2);
  expect(aero.store.getWorkspaces().workspaces.map((item) => item.id)).toEqual([1]);
  expect(commands).toContain("workspace-list");
});

test("API propaga recusa do compositor em vez de fingir sucesso", async () => {
  const aero = createAero(async () => ["ERR último workspace"]);
  await expect(aero.destroyWorkspace(1)).rejects.toThrow("último workspace");
});

test("eventos de workspace atualizam store sem chamar list", () => {
  const aero = createAero(async () => {
    throw new Error("IPC não devia ser chamado");
  });
  aero.store.applyEvent({
    WorkspaceCreated: { workspace: { id: 1, name: "Um", window_ids: [] } },
  });
  const created = parseEvent({
    WorkspaceCreated: { workspace: { id: 2, name: "Dois", window_ids: [] } },
  });
  aero.store.applyEvent(created!);
  aero.store.applyEvent({ WorkspaceActivated: { id: 2 } });
  aero.store.applyEvent({ WorkspaceRenamed: { id: 2, name: "Editado" } });
  aero.store.applyEvent({ WindowMovedToWorkspace: { window_id: 7, workspace_id: 2 } });

  expect(aero.store.getWorkspaces()).toEqual({
    workspaces: [
      { id: 1, name: "Um", window_ids: [] },
      { id: 2, name: "Editado", window_ids: [7] },
    ],
    activeId: 2,
  });
  aero.store.applyEvent({ WindowMovedToWorkspace: { window_id: 7, workspace_id: 1 } });
  expect(aero.store.getWorkspaces().workspaces[0]?.window_ids).toEqual([7]);
  expect(aero.store.getWorkspaces().workspaces[1]?.window_ids).toEqual([]);
  aero.store.applyEvent({ WorkspaceDestroyed: { id: 2 } });
  expect(aero.store.getWorkspaces().workspaces.map((workspace) => workspace.id)).toEqual([1]);
  expect(aero.store.getWorkspaces().activeId).toBeNull();
});
