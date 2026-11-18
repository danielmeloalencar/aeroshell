# AeroShell

**Pré-alfa.** Framework para compositores Wayland completos cuja interface (taskbar, widgets,
workspaces, decoração de janela, lock screen e greeter) é escrita em TSX/JSX e renderizada em
WebViews nativas do sistema (WebKitGTK 6.0). O mecanismo em Rust/Smithay é dono do estado e do
input; a política em TypeScript/Bun decide layouts, regras e idle.

Estado atual: F0 (spikes go/no-go) em andamento. S1/S2/S3 concluídos; protocolo `aero_frame_v1`
gerado e testado; desenho do frame e S4/S6/S7 pendentes. Fonte única de status:
[`docs/progress.md`](docs/progress.md). Plano resumido: [`plan.md`](plan.md).

## Rodando em modo nested (sem afetar o host)

Pré-requisitos: Ubuntu 26.04 com `libgtk-4-dev`, `libwebkitgtk-6.0-dev`,
`libgtk4-layer-shell-dev`, `libwayland-dev`, `wayland-protocols`, `libxkbcommon-dev`; Rust e Bun.
`bash scripts/doctor.sh` diz exatamente o que falta. O WebKitGTK 6.0 funciona; WPE WebKit não
existe no Ubuntu 26.04 e não é necessário.

Hoje o comando de desenvolvimento `aero dev` (`bun packages/cli/src/index.ts dev`) **ainda não
existe** — o CLI é F8. Para ver o compositor e o painel nested manualmente:

```bash
cargo build --workspace
target/debug/aeroshell-compositor --backend winit --socket wayland-aero-0 &
WAYLAND_DISPLAY=wayland-aero-0 GDK_BACKEND=wayland \
  LD_PRELOAD=libgtk4-layer-shell.so.0 target/debug/aeroshell-shell
```

O compositor nunca sobrescreve `WAYLAND_DISPLAY` do host: o nome do socket é passado apenas aos
processos filhos. Em VM sem 3D valide funcionamento; números de desempenho só em GPU real.

Documentação: [`docs/architecture.md`](docs/architecture.md),
[`docs/nested-dev.md`](docs/nested-dev.md), ADRs em [`docs/adr/`](docs/adr/).

Licença: [MIT](LICENSE).
