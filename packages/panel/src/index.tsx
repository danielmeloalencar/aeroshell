//! Painel React da fatia vertical: lista janelas e foca a clicada.
//! O transporte é a ponte privada `window.webkit.messageHandlers.aero`: o host
//! encaminha cada comando ao socket IPC e devolve as linhas por `__aeroReply`.

import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

import type { App } from "../../sdk/src/generated/App";
import type { Notification } from "../../sdk/src/generated/Notification";
import type { WindowInfo } from "../../sdk/src/generated/WindowInfo";
import type { PanelIndicators, WorkspacesState } from "../../sdk/src/store";
import type { Workspace } from "../../sdk/src/generated/Workspace";
import { createAero, parseEvent } from "../../sdk/src/index";
import {
  useApps, useIndicators, useNotifications, useWindows, useWorkspaces,
} from "../../react/src/useWindows";

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
  launcherOpen: boolean,
  query: string,
  apps: App[],
  notifications: Notification[],
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
  const launcherRects = Array.from(document.querySelectorAll<HTMLElement>(
    ".aero-launcher-toggle, .aero-launcher-search, .aero-app-row",
  )).map(rect).filter((r) => r.w > 0 && r.h > 0);
  const notificationRects = Array.from(document.querySelectorAll<HTMLElement>(
    ".aero-notification-close",
  )).map(rect).filter((r) => r.w > 0 && r.h > 0);
  // Janelas/workspaces continuam nos primeiros índices (contrato S3/F2); os
  // controles novos são anexados DEPOIS para não deslocar os rects antigos.
  post({
    type: "interactive",
    rects: [...rowRects, ...workspaceRects, ...launcherRects, ...notificationRects],
    cursor: "pointer",
  });

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
  const appRects: Record<string, { x: number; y: number; w: number; h: number }> = {};
  document.querySelectorAll<HTMLElement>(".aero-app-row").forEach((element) => {
    const id = element.dataset.appId;
    if (id) appRects[id] = rect(element);
  });
  const notificationCloseRects: Record<string, { x: number; y: number; w: number; h: number }> = {};
  document.querySelectorAll<HTMLElement>(".aero-notification-close").forEach((element) => {
    const id = element.dataset.notificationId;
    if (id) notificationCloseRects[id] = rect(element);
  });
  const toggle = document.querySelector<HTMLElement>(".aero-launcher-toggle");
  const search = document.querySelector<HTMLElement>(".aero-launcher-search");
  // O bridge loga mensagens não-ipc como `mensagem da WebView`; o harness lê
  // este estado renderizado (linhas + indicadores + retângulos) do log.
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
    launcher: {
      open: launcherOpen,
      query,
      apps: apps.map((app) => ({ id: app.id, name: app.name, icon: app.icon })),
      appRects,
      toggleRect: toggle ? rect(toggle) : null,
      searchRect: search ? rect(search) : null,
    },
    notifications: notifications.map((notification) => ({
      id: notification.id,
      title: notification.title,
      body: notification.body,
      icon: notification.icon,
      iconSource: notification.icon || "dialog-information",
      timeout_ms: notification.timeout_ms,
    })),
    notificationRects: notificationCloseRects,
  });
}

function NotificationToast({ notification }: { notification: Notification }) {
  useEffect(() => {
    const timer = window.setTimeout(() => {
      void aero.dismissNotification(notification.id);
    }, notification.timeout_ms);
    return () => window.clearTimeout(timer);
  }, [notification.id, notification.timeout_ms]);

  return (
    <div className="aero-notification" role="alert" data-notification-id={notification.id}>
      <img
        className="aero-notification-icon"
        alt=""
        src={`aero://icon/${encodeURIComponent(notification.icon || "dialog-information")}`}
      />
      <div className="aero-notification-content">
        <strong className="aero-notification-title">{notification.title}</strong>
        <span className="aero-notification-body">{notification.body}</span>
      </div>
      <button
        type="button"
        className="aero-notification-close"
        data-notification-id={notification.id}
        aria-label={`fechar notificação ${notification.title}`}
        onClick={() => void aero.dismissNotification(notification.id)}
      >
        ✕
      </button>
    </div>
  );
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
  const allApps = useApps(aero.store);
  const notifications = useNotifications(aero.store);
  const [launcherOpen, setLauncherOpen] = useState(false);
  const [query, setQuery] = useState("");
  const filteredApps = launcherOpen ? aero.searchApps(query) : [];
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
    publishPanelState(
      launcherOpen ? [] : windows,
      indicators,
      workspaceState,
      launcherOpen,
      query,
      filteredApps,
      notifications,
    );
  }, [
    windows, indicators, workspaceState, launcherOpen, query, filteredApps, allApps, notifications,
  ]);

  // A layer do painel tem 48 px; com toasts ela cresce para baixo/para cima sem
  // mudar a exclusive zone (o host apenas altera o height_request).
  useEffect(() => {
    const height = notifications.length === 0
      ? 48
      : Math.min(480, 64 + notifications.length * 88);
    post({ type: "surface-height", height });
  }, [notifications.length]);

  // O configure da layer chega depois do commit do React: republicar na mudança
  // de tamanho mantém os retângulos de clique alinhados com o que está na tela.
  useEffect(() => {
    const republish = () => publishPanelState(
      launcherOpen ? [] : windows,
      indicators,
      workspaceState,
      launcherOpen,
      query,
      filteredApps,
      notifications,
    );
    window.addEventListener("resize", republish);
    return () => window.removeEventListener("resize", republish);
  }, [
    windows, indicators, workspaceState, launcherOpen, query, filteredApps, notifications,
  ]);

  return (
    <>
      {notifications.length > 0 && (
        <div className="aero-notifications" role="list" aria-label="notificações">
          {notifications.map((notification) => (
            <NotificationToast key={notification.id} notification={notification} />
          ))}
        </div>
      )}
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
      <button
        type="button"
        className="aero-launcher-toggle"
        aria-label={launcherOpen ? "fechar launcher" : "abrir launcher"}
        onClick={() => {
          setLauncherOpen((open) => !open);
          setQuery("");
        }}
      >
        {launcherOpen ? "Fechar" : "Apps"}
      </button>
      {launcherOpen && (
        <div className="aero-launcher">
          <input
            className="aero-launcher-search"
            aria-label="buscar aplicativos"
            placeholder="Buscar aplicativos"
            value={query}
            autoFocus
            onChange={(event) => setQuery(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setLauncherOpen(false);
                setQuery("");
              }
            }}
          />
          <div className="aero-apps" role="list" aria-label="aplicativos filtrados">
            {filteredApps.map((app) => (
              <button
                type="button"
                role="listitem"
                className="aero-app-row"
                data-app-id={app.id}
                key={app.id}
                title={app.exec}
                onClick={() => {
                  aero.launchApp(app.id, workspaceState.activeId ?? undefined);
                  setLauncherOpen(false);
                  setQuery("");
                }}
              >
                <img
                  className="aero-app-icon"
                  alt=""
                  src={`aero://icon/${encodeURIComponent(app.icon || "application-x-executable")}`}
                />
                <span className="aero-app-name">{app.name}</span>
              </button>
            ))}
            {filteredApps.length === 0 && (
              <span className="aero-apps-empty">Nenhum aplicativo</span>
            )}
          </div>
        </div>
      )}
      {!launcherOpen && windows.map((w) => (
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
    </>
  );
}

const host = document.getElementById("root");
if (!host) throw new Error("aero://panel/index.html sem #root");
createRoot(host).render(<Panel />);
