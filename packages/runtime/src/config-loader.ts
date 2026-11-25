//! Startup declarativo: importa desktop.config.tsx e aplica surfaces/frame/rules.

import { watch } from "node:fs";
import { mkdtemp, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";

import type { DesktopConfig } from "../../sdk/src/generated/DesktopConfig";
import type { SurfaceDeclaration } from "../../sdk/src/generated/SurfaceDeclaration";
import type { DecorationPolicyConfig } from "../../sdk/src/generated/DecorationPolicyConfig";
import { createAero, parseEvent } from "../../sdk/src/index";
import { completeSurface } from "../../sdk/src/api/surfaces";
import { validateConfig } from "./policy-config";
import { eventStream, ipcTransport } from "./ipc-client";
import { normalizeDevOrigin, startDevServer, type DevServer } from "./dev-server";

const DEFERRED: Record<string, string> = {
  layouts: "F3", keybindings: "F3", idle: "F5", lock: "F5", greeter: "F6",
};

/** Valida a parte executável na F2 e rejeita todo campo deferido presente. */
export function validateDesktopConfig(value: unknown): DesktopConfig {
  if (typeof value !== "object" || value === null) throw new Error("desktop config deve exportar um objeto");
  const config = value as Record<string, unknown>;
  for (const [field, phase] of Object.entries(DEFERRED)) {
    if (config[field] !== undefined) throw new Error(`${field} é deferido para ${phase}; remova o campo na F2`);
  }
  if (config.surfaces !== undefined && !Array.isArray(config.surfaces)) {
    throw new Error("surfaces deve ser um array");
  }
  if (config.frame !== undefined) {
    const frame = config.frame as Record<string, unknown>;
    if (typeof frame !== "object" || frame === null || typeof frame.enabled !== "boolean") {
      throw new Error("frame.enabled deve ser boolean");
    }
  }
  if (config.rules !== undefined) validateConfig(config.rules);
  return value as DesktopConfig;
}

export type SurfaceDiff = {
  create: SurfaceDeclaration[];
  update: SurfaceDeclaration[];
  destroy: string[];
};

/** Diff por ID: conteúdo alterado é update, nunca uma nova WebView. */
export function diffSurfaceDeclarations(
  previous: SurfaceDeclaration[],
  next: SurfaceDeclaration[],
): SurfaceDiff {
  const before = new Map(previous.map((surface) => [surface.id, surface]));
  const after = new Map(next.map((surface) => [surface.id, surface]));
  if (before.size !== previous.length || after.size !== next.length) {
    throw new Error("ids de superfícies devem ser únicos");
  }
  return {
    create: next.filter((surface) => !before.has(surface.id)),
    update: next.filter((surface) => {
      const old = before.get(surface.id);
      return old !== undefined && JSON.stringify(old) !== JSON.stringify(surface);
    }),
    destroy: previous.filter((surface) => !after.has(surface.id)).map((surface) => surface.id),
  };
}

/** Empacota o grafo e importa um artifact novo: query em file:// não basta no Bun 1.4.4. */
export async function loadDesktopConfig(path: string, _cacheKey?: number): Promise<DesktopConfig> {
  const result = await Bun.build({
    entrypoints: [resolve(path)],
    target: "bun",
    format: "esm",
  });
  if (!result.success) throw new Error(`build do desktop config falhou: ${result.logs.join("\n")}`);
  const artifact = result.outputs[0];
  if (!artifact) throw new Error("build do desktop config não gerou artifact");
  const javascript = await artifact.text();
  const url = `data:text/javascript;base64,${Buffer.from(javascript).toString("base64")}`;
  const module = await import(url) as { default?: unknown };
  return validateDesktopConfig(module.default);
}

/** Converte frame.enabled no fallback SSD; rules.default explícito vence. */
export function desktopRules(config: DesktopConfig): DecorationPolicyConfig {
  const fallback = config.frame?.enabled === false ? "None" : "Server";
  return config.rules ?? { default: fallback, rules: [] };
}

export type StartDesktopOptions = { dev?: boolean };

function ids(surfaces: SurfaceDeclaration[]): string {
  return surfaces.map((surface) => surface.id).join(",") || "-";
}

/** Carrega e aplica o config, depois mantém a política viva no mesmo processo. */
export async function startDesktop(path: string, options: StartDesktopOptions = {}): Promise<void> {
  const socketPath = process.env.AERO_WINDOW_IPC_SOCK;
  if (!socketPath) throw new Error("AERO_WINDOW_IPC_SOCK não definido");
  const configPath = resolve(path);
  const transport = ipcTransport(socketPath);
  const aero = createAero(transport);
  eventStream(socketPath, (json) => {
    const event = parseEvent(json);
    if (event) aero.store.applyEvent(event);
  });

  let devServer: DevServer | undefined;
  if (options.dev) {
    const requested = normalizeDevOrigin(process.env.AERO_DEV_ORIGIN ?? "http://127.0.0.1:4173");
    if (new URL(requested).port === "0") throw new Error("--dev exige uma porta AERO_DEV_ORIGIN fixa");
    devServer = await startDevServer({
      origin: requested,
      root: dirname(configPath),
      configPath,
      shellResetPath: resolve("assets/shell-reset.css"),
      sdkClientPath: resolve("packages/sdk/src/client.ts"),
    });
    console.log(`DEV_SERVER origin=${devServer.origin} config=${configPath}`);
  }

  async function materialize(config: DesktopConfig, revision: number): Promise<SurfaceDeclaration[]> {
    const declarations = (config.surfaces ?? []).map((declaration) => ({
      ...declaration,
      dev_entry: declaration.dev_entry
        ? resolve(dirname(configPath), declaration.dev_entry)
        : undefined,
    }));
    await devServer?.update(declarations, revision);
    return declarations.map((declaration) => {
      const { dev_entry: entry, ...base } = declaration;
      if (!options.dev || !entry || !devServer) return { ...base };
      return {
        ...base,
        html: undefined,
        url: devServer.surfaceUrl(declaration.id, revision),
      };
    });
  }

  async function applyDiff(
    previous: SurfaceDeclaration[],
    config: DesktopConfig,
    revision: number,
  ): Promise<SurfaceDeclaration[]> {
    const next = await materialize(config, revision);
    const diff = diffSurfaceDeclarations(previous, next);
    for (const id of diff.destroy) await aero.surfaces.destroy(id);
    for (const declaration of diff.create) {
      const { kind, ...surfaceOptions } = declaration;
      await aero.surfaces.create(kind, surfaceOptions);
      console.log(`CONFIG_SURFACE id=${declaration.id} kind=${kind}`);
    }
    for (const declaration of diff.update) {
      const { kind, ...surfaceOptions } = declaration;
      const { id: _id, kind: _kind, ...patch } = completeSurface(kind, surfaceOptions);
      await aero.surfaces.update(declaration.id, patch);
    }
    console.log(`HMR_DIFF create=${ids(diff.create)} update=${ids(diff.update)} destroy=${diff.destroy.join(",") || "-"}`);
    return next;
  }

  let revision = 0;
  let config = await loadDesktopConfig(configPath);
  let current = await applyDiff([], config, revision);
  const listed = await aero.surfaces.list();
  console.log(`CONFIG_SURFACES count=${listed.length}`);
  console.log(`CONFIG_LIST ids=${listed.map((surface) => surface.id).sort().join(",")}`);

  // Reusa o runtime de política/hot-reload existente. Recarregar o config
  // regrava este arquivo e deixa o watcher de regras já testado fazer seu diff.
  const dir = await mkdtemp(join("/tmp/opencode", "aero-desktop-rules-"));
  const rulesPath = join(dir, "rules.ts");
  async function applyRules(nextConfig: DesktopConfig): Promise<void> {
    await writeFile(rulesPath, `export default ${JSON.stringify(desktopRules(nextConfig))};\n`);
  }
  await applyRules(config);
  process.env.AERO_DECORATION_RULES = rulesPath;
  console.log(`CONFIG_RULES default=${desktopRules(config).default} frame=${config.frame?.enabled ?? true}`);

  if (options.dev) {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let running = false;
    let queued = false;
    const reload = async () => {
      if (running) {
        queued = true;
        return;
      }
      running = true;
      do {
        queued = false;
        const nextRevision = ++revision;
        try {
          const nextConfig = await loadDesktopConfig(configPath, nextRevision);
          current = await applyDiff(current, nextConfig, nextRevision);
          config = nextConfig;
          await applyRules(nextConfig);
          console.log(`HMR_CONFIG revision=${nextRevision} status=OK`);
        } catch (error) {
          // Último config/bundle válido permanece ativo; nada é destruído por erro.
          console.error(`HMR_CONFIG_ERROR revision=${nextRevision} ${String(error)}`);
        }
      } while (queued);
      running = false;
    };
    const watcher = watch(configPath, () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void reload(), 40);
    });
    watcher.on("error", (error) => console.error(`HMR_CONFIG_WATCH_ERROR ${String(error)}`));
  }
  await import("./policy.ts");
}
