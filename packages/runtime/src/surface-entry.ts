//! Bundle browser compartilhado pelo HMR e pelo HTML inline de produção.

import { resolve } from "node:path";

import type { SurfaceDeclaration } from "../../sdk/src/generated/SurfaceDeclaration";

export type SurfaceBundle = { javascript: string; inputs: string[] };

/** Empacota o entry e todo o grafo local/framework em um único módulo browser. */
export async function bundleSurfaceEntry(entry: string): Promise<SurfaceBundle> {
  const result = await Bun.build({
    entrypoints: [resolve(entry)],
    target: "browser",
    format: "esm",
    metafile: true,
  });
  if (!result.success) throw new Error(`Bun.build falhou para ${entry}: ${result.logs.join("\n")}`);
  const output = result.outputs.find((artifact) => artifact.path.endsWith(".js"));
  if (!output) throw new Error(`Bun.build não gerou JavaScript para ${entry}`);
  return {
    javascript: await output.text(),
    inputs: Object.keys(result.metafile?.inputs ?? {}).map((path) => resolve(path)),
  };
}

/** Substitui scripts inline pelo bundle, preservando o documento e o reset. */
export function surfaceDocumentHtml(
  surface: SurfaceDeclaration,
  script: string,
  reset = "",
): string {
  let html = surface.html ?? "<!doctype html><html><head></head><body></body></html>";
  html = html.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "");
  if (html.includes("</head>")) {
    return html
      .replace(/<head>/i, `<head>${reset}`)
      .replace("</head>", `${script}</head>`);
  }
  return `${reset}${script}${html}`;
}

/**
 * Materializa `dev_entry` também fora de `--dev`: o JavaScript fica inline no
 * HTML `aero://`, sem dev server, asset externo ou navegação adicional.
 */
export async function materializeProductionSurface(
  declaration: SurfaceDeclaration,
): Promise<SurfaceDeclaration> {
  if (!declaration.dev_entry) return { ...declaration, dev_entry: undefined };
  const { javascript } = await bundleSurfaceEntry(declaration.dev_entry);
  // Uma string JS não pode fechar o script do documento que a contém.
  const safeJavascript = javascript.replace(/<\/script/gi, "<\\/script");
  return {
    ...declaration,
    dev_entry: undefined,
    html: surfaceDocumentHtml(
      declaration,
      `<script type="module">${safeJavascript}</script>`,
    ),
  };
}
