//! Entrada declarativa do desenvolvedor; o tipo completo é gerado pelo ts-rs.

import type { DesktopConfig } from "./generated/DesktopConfig";

/**
 * Declara o desktop sem framework nem código Rust.
 * Na F2, `surfaces`, `frame` e `rules` são aplicados. Usar `layouts`,
 * `keybindings`, `idle`, `lock` ou `greeter` produz erro explícito da fase futura.
 *
 * @example
 * export default defineDesktop({
 *   surfaces: [{ kind: "Desktop", id: "fundo", html: "<body></body>" }],
 *   frame: { enabled: true },
 *   rules: { default: "Server", rules: [] },
 * });
 */
export function defineDesktop(config: DesktopConfig): DesktopConfig {
  return config;
}
