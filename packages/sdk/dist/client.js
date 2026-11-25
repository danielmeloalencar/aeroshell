// packages/sdk/src/api/input.ts
//! API privilegiada `aero.input`: grabs globais e callbacks do runtime.
//! O guest browser não recebe este módulo nem esta capability.
var prefix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

// packages/sdk/src/api/outputs.ts
//! API privilegiada `aero.outputs`; os tipos vêm exclusivamente do ts-rs.

// packages/sdk/src/api/surfaces.ts
//! API privilegiada `aero.surfaces` do runtime Bun.
//! Os tipos públicos vêm exclusivamente dos bindings ts-rs; a conversão da wire
//! textual (minúsculas/margins em array) fica isolada neste módulo.
function pascal(value) {
  const text = String(value ?? "");
  return text.charAt(0).toUpperCase() + text.slice(1);
}
function surfaceFromWire(value) {
  if (typeof value !== "object" || value === null)
    throw new Error("superfície inválida");
  const raw = value;
  const margins = Array.isArray(raw.margins) ? raw.margins : null;
  const anchors = raw.anchors ?? {};
  return {
    ...raw,
    kind: pascal(raw.kind),
    layer: pascal(raw.layer),
    keyboard_mode: pascal(raw.keyboard_mode),
    margins: margins ? { left: Number(margins[0]), right: Number(margins[1]), top: Number(margins[2]), bottom: Number(margins[3]) } : raw.margins,
    anchors: {
      left: Boolean(anchors.left),
      right: Boolean(anchors.right),
      top: Boolean(anchors.top),
      bottom: Boolean(anchors.bottom)
    },
    input_region: raw.input_region ?? null,
    blur_region: raw.blur_region ?? null,
    width: raw.width === undefined ? null : raw.width,
    height: raw.height === undefined ? null : raw.height,
    html: raw.html === undefined ? null : raw.html,
    url: raw.url === undefined ? null : raw.url
  };
}

// packages/sdk/src/index.ts
//! SDK mínimo: política TS sobre o transporte IPC do compositor.
//! Os tipos de estado vêm de `./generated` (gerados por ts-rs a partir do Rust).
var EVENT_KEYS = [
  "Snapshot",
  "Title",
  "Icon",
  "Focus",
  "Geometry",
  "Csd",
  "FramePainted",
  "WorkspaceCreated",
  "WorkspaceActivated",
  "WorkspaceRenamed",
  "WorkspaceDestroyed",
  "WindowMovedToWorkspace",
  "AppListUpdated",
  "OutputAdded",
  "OutputChanged",
  "OutputRemoved",
  "KeyBind",
  "SurfaceCreated",
  "SurfaceUpdated",
  "SurfaceDestroyed",
  "NotificationShown",
  "NotificationDismissed",
  "DecorationRequest",
  "DecorationApplied"
];
function parseEvent(raw) {
  try {
    const value = typeof raw === "string" ? JSON.parse(raw) : raw;
    if (typeof value !== "object" || value === null)
      return null;
    if (!EVENT_KEYS.some((key) => (key in value)))
      return null;
    if ("SurfaceCreated" in value) {
      return { SurfaceCreated: { surface: surfaceFromWire(value.SurfaceCreated.surface) } };
    }
    if ("SurfaceUpdated" in value) {
      return { SurfaceUpdated: { surface: surfaceFromWire(value.SurfaceUpdated.surface) } };
    }
    return value;
  } catch {
    return null;
  }
}
var notificationPrefix = `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;

// packages/sdk/src/client.ts
//! Client browser sem framework sobre a capability `window.aero` da própria surface.
function createClient(host = window) {
  const surfaceId = host.aeroSurfaceId;
  if (!surfaceId)
    throw new Error("window.aeroSurfaceId ausente: superfície sem identidade");
  let nextSeq = 1;
  const pending = new Map;
  host.__aeroReply = (seq, lines) => {
    pending.get(seq)?.resolve(lines);
    pending.delete(seq);
  };
  function command(command) {
    return new Promise((resolve) => {
      const seq = nextSeq++;
      pending.set(seq, { resolve });
      host.aero.postMessage({ type: "ipc", command, seq });
    });
  }
  async function update(patch) {
    const candidate = patch;
    if ("id" in candidate)
      throw new Error("guest só atualiza a própria superfície");
    const invalid = Object.keys(candidate).filter((key) => key !== "input_region" && key !== "blur_region");
    if (invalid.length) {
      throw new Error(`guest só atualiza input_region/blur_region; campo(s): ${invalid.join(",")}`);
    }
    const lines = await command(`surface-update ${JSON.stringify({ id: surfaceId, ...patch })}`);
    if (lines[0] !== "OK")
      throw new Error(lines.join(`
`) || "IPC sem resposta");
  }
  return {
    subscribe(callback) {
      const listener = (raw) => {
        const event = parseEvent(raw.detail);
        if (event)
          callback(event);
      };
      host.addEventListener("aeroEvent", listener);
      return () => host.removeEventListener("aeroEvent", listener);
    },
    update,
    enableHmr() {
      const location = host.location;
      const Socket = host.WebSocket;
      if (!location || location.protocol !== "http:" || !Socket) {
        throw new Error("HMR requer uma superfície servida pelo dev-server HTTP");
      }
      const socket = new Socket(`ws://${location.host}/ws`);
      socket.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data);
          if (message.type === "reload" && message.surfaceIds?.includes(surfaceId)) {
            location.reload();
          }
        } catch {}
      };
      return () => socket.close();
    },
    setInputRegion(regions) {
      return update({ input_region: regions });
    },
    setBlurRegion(regions) {
      return update({ blur_region: regions });
    }
  };
}
export {
  createClient
};
