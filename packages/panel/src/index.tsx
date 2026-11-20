//! Painel React da fatia vertical: lista janelas e foca a clicada.
//! O transporte é a ponte privada `window.webkit.messageHandlers.aero`: o host
//! encaminha cada comando ao socket IPC e devolve as linhas por `__aeroReply`.

import { useEffect } from "react";
import { createRoot } from "react-dom/client";

import { createAero } from "../../sdk/src/index";
import { useWindows } from "../../react/src/useWindows";

type Reply = (lines: string[]) => void;

declare global {
  interface Window {
    __aeroReply?: (seq: number, lines: string[]) => void;
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

/** Retângulos reais dos botões, lidos do DOM para o hit-test (S3). */
function publishInputRegions(): void {
  const rects = Array.from(document.querySelectorAll<HTMLElement>(".aero-row"))
    .map((el) => {
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, w: r.width, h: r.height };
    })
    .filter((r) => r.w > 0 && r.h > 0);
  post({ type: "interactive", rects, cursor: "pointer" });
}

function Panel() {
  const windows = useWindows(aero.store);

  useEffect(() => {
    let alive = true;
    const poll = async () => {
      try {
        await aero.windows.list();
      } finally {
        if (alive) {
          const current = aero.store.getSnapshot().windows;
          console.info(
            "painel React:",
            current.map((w) => `${w.id} ${w.title}`).join(" | "),
          );
        }
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  // Roda após o commit: os rects refletem o layout real dos botões.
  useEffect(() => {
    publishInputRegions();
  }, [windows]);

  return (
    <div className="aero-panel">
      {windows.map((w) => (
        <button
          key={w.id}
          data-window-id={w.id}
          className="aero-row"
          onClick={() => void aero.windows.focus(w.id)}
        >
          <span className="aero-id">{w.id}</span>
          <span className="aero-title">{w.title}</span>
        </button>
      ))}
    </div>
  );
}

const host = document.getElementById("root");
if (!host) throw new Error("aero://panel/index.html sem #root");
createRoot(host).render(<Panel />);
