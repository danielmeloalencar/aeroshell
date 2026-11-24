//! Painel React da fatia vertical: lista janelas e foca a clicada.
//! O transporte é a ponte privada `window.webkit.messageHandlers.aero`: o host
//! encaminha cada comando ao socket IPC e devolve as linhas por `__aeroReply`.

import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import type { PanelIndicators, WorkspacesState } from "../../sdk/src/store";
import type { Workspace } from "../../sdk/src/generated/Workspace";
import { createAero, parseEvent } from "../../sdk/src/index";
import { useIndicators, useWindows, useWorkspaces } from "../../react/src/useWindows";

type Reply = (lines: string[]) => void;

declare global {
  interface Window {
    __aeroReply?: (seq: number, lines: string[]) => void;
    __aeroEvent?: (json: string) => void;
    webkit: {
      messageHandlers: {
        aero: { postMessage(message: unknown): void };
      };
    };
  }
}

const pending = new Map<number, Reply>();
window.__aeroReply = (seq, lines) => {
  pending.get(seq)?.(lines);
  pending.delete(seq);
};

let nextSeq = 1;
function post(message: unknown, reply?: Reply): void {
  const seq = nextSeq++;
  if (reply) pending.set(seq, reply);
  window.webkit.messageHandlers.aero.postMessage({ ...message, seq });
}

const transport = (command: string): Promise<string[]> =>
  new Promise((resolve) => {
    post({ type: "ipc", command }, resolve);
  });

const aero = createAero(transport);

// O host empurra eventos; o React re-renderiza pelo store, sem setInterval.
window.__aeroEvent = (json: string) => {
  const event = parseEvent(json);
  if (event) aero.store.applyEvent(event);
};
// O bridge guarda os eventos publicados antes do bundle carregar e libera a
// fila aqui: sem isto os indicadores iniciais ficavam "default"/sem foco.
post({ type: "ready" });

/** Retângulos reais das linhas e dos ✕, lidos do DOM para o hit-test (S3). */
function publishPanelState(
  windows: WindowInfo[],
  indicators: PanelIndicators,
  workspaceState: WorkspacesState,
): void {
  const rect = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  };
  const rowRects = Array.from(document.querySelectorAll<HTMLElement>(".aero-row"))
    .map(rect)
    .filter((r) => r.w > 0 && r.h > 0);
  const workspaceRects = Array.from(
    document.querySelectorAll<HTMLElement>(".aero-workspace, .aero-workspace-add"),
  ).map(rect).filter((r) => r.w > 0 && r.h > 0);
  const tabs: Record<number, { x: number; y: number; w: number; h: number }> = {};
  document.querySelectorAll<HTMLElement>(".aero-workspace").forEach((element) => {
    const id = Number(element.dataset.workspaceId);
    const r = rect(element);
    if (Number.isInteger(id) && r.w > 0) tabs[id] = r;
  });
  const addControl = document.querySelector<HTMLElement>(".aero-workspace-add");
  const addRect = addControl ? rect(addControl) : null;
  // Janelas continuam nos primeiros índices (contrato dos harnesses S3/F2).
  post({ type: "interactive", rects: [...rowRects, ...workspaceRects], cursor: "pointer" });

  // O ✕ pode ficar fora do box da linha quando o conteúdo transborda (o CSS
  // de reset usa overflow:hidden). A linha mesma trata os 18 px finais como
  // botão de fechar; publicamos esse retângulo para o harness clicar nele.
  const closes: Record<number, { x: number; y: number; w: number; h: number }> = {};
  windows.forEach((window, index) => {
    const row = rowRects[index];
    if (row) {
      closes[window.id] = { x: row.x + row.w - 18, y: row.y + 1, w: 18, h: row.h - 2 };
    }
  });
  // O bridge loga mensagens não-ipc como `mensagem da WebView`; o harness lê
  // este estado renderizado (linhas + indicadores + retângulo do ✕) do log.
  post({
    type: "panel-state",
    rows: windows.map((w) => ({
      id: w.id,
      title: w.title,
      focused: indicators.focusedId === w.id,
      decoration: indicators.decorations[w.id] ?? "default",
    })),
    closes,
    workspaces: workspaceState.workspaces.map((workspace) => ({
      id: workspace.id,
      name: workspace.name,
      active: workspace.id === workspaceState.activeId,
      windowIds: workspace.window_ids,
    })),
    activeWorkspace: workspaceState.activeId,
    workspaceRects: tabs,
    addRect,
  });
}

function WorkspaceTab({ workspace, active }: { workspace: Workspace; active: boolean }) {
  const [name, setName] = useState(workspace.name);
  useEffect(() => setName(workspace.name), [workspace.id, workspace.name]);

  function commitName(): void {
    const next = name.trim();
    if (!next || next === workspace.name) {
      setName(workspace.name);
      return;
    }
    void aero.renameWorkspace(workspace.id, next);
  }

  return (
    <div
      className="aero-workspace"
      data-workspace-id={workspace.id}
      data-active={active ? "1" : "0"}
      role="button"
      tabIndex={0}
      onClick={() => void aero.activateWorkspace(workspace.id)}
    >
      <input
        aria-label={`nome do workspace ${workspace.id}`}
        value={name}
        onChange={(event) => setName(event.currentTarget.value)}
        onBlur={commitName}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") {
            setName(workspace.name);
            event.currentTarget.blur();
          }
        }}
      />
    </div>
  );
}

function Panel() {
  const allWindows = useWindows(aero.store);
  const indicators = useIndicators(aero.store);
  const workspaceState = useWorkspaces(aero.store);
  const activeWorkspace = workspaceState.workspaces.find(
    (workspace) => workspace.id === workspaceState.activeId,
  );
  const windows = activeWorkspace
    ? allWindows.filter((window) => activeWorkspace.window_ids.includes(window.id))
    : allWindows;

  useEffect(() => {
    // Snapshots iniciais únicos; mudanças posteriores chegam por __aeroEvent.
    void Promise.all([aero.windows.list(), aero.listWorkspaces()]).then(() => {
      const current = aero.store.getSnapshot().windows;
      console.info(
        "painel React:",
        current.map((w) => `${w.id} ${w.title}`).join(" | "),
      );
    });
  }, []);

  // Roda após o commit: os rects refletem o layout real dos botões.
  useEffect(() => {
    publishPanelState(windows, indicators, workspaceState);
  }, [windows, indicators, workspaceState]);

  return (
    <div className="aero-panel">
      <div className="aero-workspaces">
        {workspaceState.workspaces.map((workspace) => (
          <WorkspaceTab
            key={workspace.id}
            workspace={workspace}
            active={workspace.id === workspaceState.activeId}
          />
        ))}
        <button
          type="button"
          className="aero-workspace-add"
          aria-label="criar workspace"
          onClick={() => void aero.createWorkspace()}
        >
          +
        </button>
      </div>
      {windows.map((w) => (
        <div
          key={w.id}
          data-window-id={w.id}
          className="aero-row"
          role="button"
          tabIndex={0}
          data-focused={indicators.focusedId === w.id ? "1" : "0"}
          data-decoration={indicators.decorations[w.id] ?? "default"}
          onClick={(event) => {
            const bounds = event.currentTarget.getBoundingClientRect();
            if (event.clientX >= bounds.right - 18) void aero.windows.close(w.id);
            else void aero.windows.focus(w.id);
          }}
        >
          <span className="aero-focus">{indicators.focusedId === w.id ? "●" : "○"}</span>
          <span className="aero-id">{w.id}</span>
          <span className="aero-title">{w.title}</span>
          <span className="aero-decoration">
            {(indicators.decorations[w.id] ?? "default").toLowerCase()}
          </span>
          <span
            className="aero-close"
            data-close-id={w.id}
            role="button"
            aria-label={`fechar ${w.title}`}
            onClick={(event) => {
              event.stopPropagation();
              void aero.windows.close(w.id);
            }}
          >
            ✕
          </span>
        </div>
      ))}
    </div>
  );
}

const host = document.getElementById("root");
if (!host) throw new Error("aero://panel/index.html sem #root");
createRoot(host).render(<Panel />);
