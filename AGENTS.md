# AGENTS.md — AeroShell

## Leia isto primeiro
- `plan.md` é o plano sincronizado; `docs/progress.md` é o histórico executivo com números e hashes.
- **Cuidado**: `docs/progress.md` ainda contém seções históricas com `AERO_SHELL_PID` e um "BLOQUEIO" de fd. Isso está **superado**: o código atual usa `AERO_PRIVILEGED_FD` + socketpair. Para comportamento, confie no código (`crates/aeroshell-compositor/src/main.rs`, `crates/aeroshell-compositor/src/protocols/aero_frame.rs`), não nessas seções.
- F0 avança por aprovação do usuário. Próximo passo registrado: protótipo mínimo do frame. **Não iniciar F1 sem autorização.**

## Comandos que realmente funcionam
- `bash scripts/doctor.sh` — verifica deps e **nomeia o pacote apt faltante**; `just check` roda `doctor` + `cargo check --workspace --all-targets` + `bun test`.
- `cargo clippy --workspace --all-targets -- -D warnings` (recipe `just clippy`).
- Teste focado do protocolo: `cargo test -p aeroshell-compositor protocols::aero_frame::tests`.
- Testes de protocolo completos: `bash tests/integration/aero_frame_protocol.sh` (usa Python + socketpair; precisa de `zenity` e `python3`).
- Outros: `bash tests/integration/s1_nested.sh`, `bash tests/integration/s2_panel.sh`, `bash tests/integration/s3_input.sh`, `bash tests/bench/run_s5_bench.sh`.
- `bun test` cobre `tests/integration/s1_nested.test.ts`.
- **`just dev` / `bun run dev` hoje quebram**: apontam para `packages/cli/src/index.ts`, que ainda não existe (S4 não iniciado). Não use como verificação.
- **`just types` / `npm run generate:types` são stubs**: `aeroshell-ipc` tem `export-bindings = []`, sem `ts-rs`.

## Ambiente e dependências
- Validado sem env customizado: Ubuntu 26.04, `pkg-config --modversion gtk4-layer-shell-0` → 1.3.0, crate Rust `gtk4-layer-shell 0.8.0` (mínimo declarado 1.0). Debian trixie 1.0.x **não foi validado**.
- `LD_PRELOAD=libgtk4-layer-shell.so.0` é defesa documentada; a falha silenciosa não reproduziu na lib C 1.3.0. Ver `docs/nested-dev.md`.
- VM VirtualBox sem 3D: medir só funcionamento; FPS/RAM/latência finais exigem GPU real. `WebKitGPUProcess = 0` nos números de VM.
- `sudo` pede senha neste ambiente; aptroot local em `/tmp/opencode/aptroot` é histórico e **não é mais necessário** para compilar.

## Fronteiras de arquitetura
- `aeroshell-compositor` não conhece GTK/WebKit; `shell`/`lock`/`greeter` não conhecem Smithay.
- Somente `backend/*.rs` conhece `winit` (nested), `udev` (real) e `headless`.
- Layer surfaces devem passar pelo `LayerMap` do Smithay (`map_layer`/`arrange`), **nunca** ser desenhadas manualmente: isso já causou painel invisível com `Transform::Flipped180`.
- `aero://shell-reset.css` (`assets/shell-reset.css`) é injetado em todas as superfícies; componentes podem sobrescrever explicitamente. Ver `docs/sdk-reference/shell-reset.md`.
- Z-order: `desktop < [frame + cliente] < panels/overlay < lock`.

## Wayland / segurança (armadilhas caras)
- **Nunca coloque `WAYLAND_SOCKET` no ambiente do compositor**: `winit` → `wayland-client::connect_to_env()` consome o fd e disputa posse (causou `IO Safety violation: owned file descriptor already closed`). Use `AERO_PRIVILEGED_FD`, convertido para `OwnedFd` no início de `main`, removido do env; a ponta vai para `UnixStream::from(fd)` → `display.insert_client` com `ClientState{is_shell:true}`. `WAYLAND_SOCKET` existe só no `Command` do filho.
- Cliente do socket público **nunca** vê `aero_frame_manager_v1`; `can_view` só olha a marca `is_shell` (padrão: negar). `AERO_SHELL_PID` não existe mais.
- `--command` do compositor usa `sh -c`, seta `WAYLAND_DISPLAY`/`GDK_BACKEND=wayland` e remove `DISPLAY` **e** `WAYLAND_SOCKET`. Sem remover `DISPLAY`, GTK3 abre no X11 do host e nada aparece na janela.
- `aero_frame_v1`: `role` via `give_role`; `already_constructed` e serial inválido são erros/ignorados protegendo a fronteira; `unknown_window` gera `closed` (corrida legítima, não erro fatal).
- Não altere `WAYLAND_DISPLAY` do host nem `/etc/pam.d` sem aviso.

## Código gerado / protocolos
- XML: `protocols/aero_frame_v1.xml`. Código gerado em tempo de compilação por `wayland_scanner::generate_interfaces!` + `generate_server_code!`/`generate_client_code!`, no lado servidor em `crates/aeroshell-compositor/src/aero_frame_protocol.rs` e no cliente em `crates/aeroshell-frame-client/src/main.rs`. Não editar "código gerado" à mão.
- O XML usa objetos Wayland reais (`wl_surface`, `wl_seat`); o módulo gerado precisa reexportar as interfaces centrais de `wayland-server`/`wayland-client` e gerar interfaces de `/usr/share/wayland/wayland.xml` num submódulo separado.
- `aero://panel/index.html` tem **host** `panel` e **path** `/index.html`: o handler do esquema casa pela URI completa, não só pelo path.

## Variáveis de teste / depuração
- Compositor: `AERO_PRIVILEGED_FD`, `AERO_TEST_INPUT_SOCK` (`motion x y`, `button press|release`), `AERO_DEBUG_CLEAR_LIGHT`.
- Shell: `AERO_BENCH`, `AERO_BENCH_RELATED`, `AERO_BENCH_ATLAS`, `AERO_BENCH_ANCHOR`, `AERO_TEST_BACKDROP`, `AERO_PANEL_FORCE_OVERFLOW`, `AERO_PANEL_LEGACY`, `AERO_SHELL_EXIT_AFTER` (saída graciosa para logs completos).
- Cliente de teste: `AERO_WAIT_SOCKET`, `AERO_EXPECT_NO_GLOBAL`, `AERO_TEST_OK`, `AERO_TEST_DOUBLE`, `AERO_TEST_ROLE`, `AERO_TEST_FALSE_SERIAL`, `AERO_FRAME_WINDOW_ID`.
- O shell escreve logs no **stderr** (unbuffered) porque processos mortos por `timeout` perdiam o buffer de stdout.
- `AERO_TEST_BACKDROP` troca o `application_id` para `dev.aeroshell.backdrop`; sem isso o GApplication faz handoff para a instância já registrada.

## Decisões e métricas
- ADR 0001: `related-view` com âncora oculta viva; pool 2–4 frames serve **latência**, não memória; atlas é Plano B condicionado a medição em GPU real.
- ADR 0003: blur apenas via `ext-background-effect-v1`; Smithay git já tem handler, `wayland-protocols` tem bindings; custo real é o render. Não reintroduzir `set_blur_region` privado.
- Métrica congelada do frame: ms entre `frame.configure(serial,w,h)` e o `wl_surface.commit` do frame com o novo tamanho, por passo de resize; média, p95 e frames perdidos (timeout de ack = 100 ms); release, 3 repetições, N=5 e N=20; VM = relativo.
- Commits pequenos e descritivos. Documentação em `docs/adr/`, `docs/progress.md`, `docs/nested-dev.md`.
