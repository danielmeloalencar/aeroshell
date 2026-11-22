# Tarefa F1d — backend udev (DRM + GBM + libinput + libseat)

> Cartão de tarefa: um agente deve conseguir executá-lo lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> Pré-requisitos: F1a, F1b, F1c FECHADAS (juiz visual OK). Autorização do humano para F1d obtida em 2026-11-21.

## Objetivo (uma frase, verificável)
O binário do compositor passa a **compilar e linkar** o backend de sessão real do Smithay
(udev + DRM/GBM + libinput + libseat) por padrão, o `doctor` e o CI passam a exigir as libs que
esse backend precisa, e o nested continua verde — **sem nenhuma mudança de comportamento visual**.

F1d é executada em **três sub-cartões independentes**, cada um com gate verde e commit próprio:

| Sub-cartão | O que entrega | Validação final |
|---|---|---|
| **F1d1** (este arquivo, §critério) | features do Smithay, `doctor`, `ci.yml`, RUNBOOK, deps no `Cargo.lock` | gate + nested + opt-out |
| **F1d2** (executada 2026-11-22, ok do humano) | sonda GBM em `renderD128` (GbmDevice → GbmAllocator) + enumerate de modos DRM, sem trocar VT/master | saída da sonda registrada em STATE.md §7 |
| **F1d3** (implementada 2026-11-22, **juiz visual NEGOU**) | backend udev (libseat/udev/DRM/GBM/EGL/libinput) + cena compartilhada em `render/scene.rs` | humano troca o VT no tty3, olha e fecha; sem o ok não é FECHADA |
| **F1d3-diagnóstico** (cartão `F1d3-diagnostico-visual.md`) | instrumentação de sessão/CRTC/queue/vblank + `Ctrl+Alt+F1..F4` | log instrumentado do tty3 + `udev_frame_probe.py`; a imagem ainda é julgada pelo humano |

## Contexto mínimo (arquivos que importam)
- `Cargo.toml:21-26` — `smithay = { version = "=0.7.0", default-features = false, features = [...] }` do workspace.
- `crates/aeroshell-compositor/Cargo.toml:7-13` — `[features] bench = []` e `smithay.workspace = true`
  (único crate do workspace que usa Smithay; verificado por `grep -rn "^smithay" crates/*/Cargo.toml`).
- `crates/aeroshell-compositor/src/backend/udev.rs:20-26` — `init_udev` retorna `Err(...)`: stub declarado "implementação na F1".
- `crates/aeroshell-compositor/src/backend/mod.rs` — trait `Backend` e escolha de backend (`--backend winit|udev|headless`).
- Features do Smithay **na versão fixada 0.7.0** (`~/.cargo/registry/src/*/smithay-0.7.0/Cargo.toml`):
  `backend_gbm` = `["gbm","cc","pkg-config","backend_drm"]` (linhas 59-64 → **já implica `backend_drm`**);
  `backend_udev` = `["udev","input/udev"]` (linhas 73-76); `backend_libinput` = `["input"]` (67);
  `backend_session_libseat` = `["backend_session","libseat"]` (69-72). Não inventar nomes fora desta lista.
- `scripts/doctor.sh:65-68` — `libdrm`, `gbm`, `libseat` hoje são `check_optional_pc`.
- `.github/workflows/ci.yml:13-19` — lista de apt do CI, **sem** as libs de DRM/GBM/libseat.
- `scripts/setup-deps.sh:13-19` — já instala `libdrm-dev libgbm-dev libinput-dev libudev-dev libsystemd-dev`
  e `libseat-dev` (fallback `libelogind-dev`): é a lista de referência para o CI.
- ADRs relevantes: nenhum novo. Decisão congelada aplicável: AGENTS.md §2 "Backends: `winit` (nested),
  `udev` (real), `headless` (testes); só `backend/*.rs` conhece a diferença".

## Critério de aceite (comandos exatos + resultado esperado)
1. `cargo check --workspace --all-targets` → `EXIT=0`.
2. `cargo clippy --workspace --all-targets -- -D warnings` → `EXIT=0`.
3. `cargo build --workspace` → `EXIT=0`.
4. **Prova de que o backend de sessão real está no build** (comandos que medem o que o plano supunha medir):
   - `cargo tree -p aeroshell-compositor | grep -cE "(gbm|drm|drm-ffi|input|udev|libseat) v"` → **≥ 6**
     (crates no grafo: `drm 0.14.1`, `drm-ffi 0.9.1`, `gbm 0.18.0`, `input 0.9.1`, `libseat 0.2.4`, `udev 0.9.3`).
   - `nm -a --defined-only target/debug/aeroshell-compositor | grep -E " a (gbm|libseat|udev|input|drm)\."`
     → **linhas de CGU dos 5 crates** (prova de que entraram no binário, não só na feature).
   - **Registrado e NÃO descartado:** o plano dizia `ldd … | grep -E "gbm|libdrm|libinput|libseat|libudev"`
     → 5 linhas. **Medido: 0 linhas** (`readelf -d` mostra `DT_NEEDED` só de libxkbcommon, libgcc_s, libc, ld-linux).
     O binário da F1d1 não chama nenhuma função C dessas libs (`backend/udev.rs:20-26` é stub), e o padrão
     `--as-needed` do linker do Ubuntu descarta as `.so` não referenciadas. A `DT_NEEDED` delas só aparece
     quando o código as usa → **a provar em F1d2** (sonda GBM) e F1d3 (backend real).
     Fonte da linkagem em tempo de build: `libseat-sys-0.2.0/build.rs:5,30` (`pkg_config::probe_library("libseat").unwrap()`)
     — é por isso que `libseat-dev` passa a ser obrigatório no `doctor`.
5. `cargo tree -p aeroshell-compositor --no-default-features` → **sem** linhas de `libseat`, `gbm`, `input`, `udev`
   (prova de opt-out: o build leve do nested continua possível).
6. `bash scripts/check.sh` → verde numa **única** execução (doctor + fmt + clippy + cargo test + bun test).
7. `bash tests/integration/s1_nested.sh` → verde (nested não regride).
8. `bash scripts/doctor.sh` → `libdrm`, `gbm`, `libseat` aparecem como obrigatórios (`OK ... pc=`), sem a
   seção "Opcionais ausentes" listando-os.
9. CI: `.github/workflows/ci.yml` lista `libdrm-dev libgbm-dev libinput-dev libudev-dev libsystemd-dev`
   (mesma lista de `scripts/setup-deps.sh:13-19`) e instala `libseat-dev` num passo separado com o
   fallback `libelogind-dev` (espelhando `scripts/setup-deps.sh:17-19`), para o `apt-get` principal
   não falhar caso o runner não tenha o nome `libseat-dev`.
10. `docs/RUNBOOK.md` tem a seção "Rodar em DRM (udev)" com o comando exato do tty3 e a ressalva de que a
    **validação visual é do humano** (ele troca o VT e fecha).
11. **Nenhum critério visual nesta sessão.** O backend udev continua sendo stub (`udev.rs:20-26` devolve `Err`):
    nada aparece na tela em F1d1. O juiz visual humano só é exigido em F1d3.

## F1d2 — critério de aceite (executada em 2026-11-22)
1. **Teste primeiro:** `python3 tests/integration/udev_drm_probe.py` escrito e executado **antes** da sonda →
   `exit 2`, `argumento desconhecido: --probe-gbm=/dev/dri/renderD128` (amostra em `/tmp/opencode/f1d2/probe-run.log`).
2. `cargo build -p aeroshell-compositor` → `EXIT=0`, sem warnings.
3. `python3 tests/integration/udev_drm_probe.py` → `EXIT=0`, `OK: sonda GBM em renderD128 passou`.
   Medições registradas em STATE.md §7: `backend_name=drm`, 4 formats RENDERING|SCANOUT, alocação direta e via
   `GbmAllocator` com dmabuf, 8 conectores em card0 (`Virtual-1` conectado, 35 modos).
4. `readelf -d target/debug/aeroshell-compositor | grep NEEDED` → **`libgbm.so.1` presente**, fechando a lacuna
   de F1d1 (medida: 0 linhas antes da sonda). `ldd` mostra `libgbm.so.1` e `libdrm.so.2` carregados.
5. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` (e `--features bench`),
   `cargo check --workspace --no-default-features`, `bash tests/integration/s1_nested.sh`,
   `bash scripts/check.sh` → todos `EXIT=0` (amostra falhada de `fmt` preservada em `/tmp/opencode/f1d2/validate.log`,
   corrigida por `cargo fmt` e reprovada em `/tmp/opencode/f1d2/validate2.log`).
6. Repetibilidade: 3 execuções da sonda → 3× `EXIT=0`, mesmo SHA-256 (`3095005fe352…`), 8935 bytes cada.
7. Casos de borda: `--probe-gbm` sem valor usa `/dev/dri/renderD128`; `--help` documenta a flag;
   `--no-default-features` recusa a sonda com mensagem explícita (`EXIT=1`) e o build padrão é restaurado em seguida.
8. **Nenhum critério visual:** sonda roda dentro do compositor host, sem VT/master/sessão. Nada foi aberto/fechado.

## F1d3 — critério de aceite (código pronto, validação visual PENDENTE)
1. `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   (e `--features bench`), `cargo check --workspace --no-default-features` → `EXIT=0`.
2. `bash tests/integration/s1_nested.sh`, `python3 tests/integration/headless_render_damage.py`,
   `dbus-run-session -- python3 tests/integration/multi_output_damage.py` → verdes (sem regressão do
   refactor `render/scene.rs`); `python3 tests/integration/udev_drm_probe.py` → verde.
3. `bash scripts/check.sh` → verde numa única execução.
4. `env -u WAYLAND_DISPLAY -u DISPLAY target/debug/aeroshell-compositor --backend udev …` nesta VM →
   abre libseat (`seat0`), descobre `/dev/dri/card0` e falha **graciosamente** em `session.open()` com `EAGAIN`
   (sem VT ativo). **Não é prova de funcionamento em DRM** — apenas de que a cadeia até a abertura do device está correta.
5. **Critério visual NÃO executado pelo agente** (AGENTS.md §0.9): no tty3, humano troca o VT, confirma
   painel/`foot`/arrasto/sem piscar/retomada após VT, dá o ok e fecha. **Sem esse ok a tarefa não é FECHADA.**
6. Cursor no backend udev **não renderizado** (dívida conhecida); ponteiro relativo do libinput passou a ser
   tratado em `input.rs` (antes era no-op) — sem teste automatizado próprio, coberto só pelo juiz visual.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Hotplug de DRM (adicionar/remover GPU em runtime) — F1d3 só **loga** o evento e pede reinício.
- Cursor no backend udev (já registrado em §6; exige `cursor-icon` 1.2 + `MemoryRenderBufferRenderElement`).
- Multi-GPU, `GpuManager`/dmabuf feedback, DRM lease, presentation-time, XWayland — o anvil faz, mas exigiria
  `smithay_drm_extras` (dependência nova, não aprovada) e está fora do cartão.
- `EGLDevice`/`EGLDisplay` são usados em F1d3 (`init_udev` cria o renderer EGL sobre GBM); a sonda F1d2 não
  precisava deles. `ElesRenderer` não foi usado — o caminho é `DrmOutputManager` + `GlesRenderer`.
- Cursor no backend udev (`PointerRenderElement`/`SoftwareCursor` não existem no Smithay 0.7.0; exige
  `cursor-icon` 1.2 + `MemoryRenderBufferRenderElement`) — registrar em STATE.md §6, não corrigir.
- Adicionar `renderer_pixman`, `renderer_multi`, `renderer_test`, `xwayland`, `use_system_lib`,
  `backend_gbm_has_create_with_modifiers2`, `backend_gbm_has_fd_for_plane`.
- Plano B (dumb + legacy + `renderer_pixman`): exige aprovação explícita do humano.
- F2/F3 (painel em sessão real, lock/greeter em TTY), presentation-time, dmabuf.
- Trocar `default-features` do workspace dependency por features de sessão real (ver "Decisão de features").

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Decisão de features (e por quê)
As quatro features de sessão real entram **no crate** `aeroshell-compositor`, como aliases
(`backend_udev = ["smithay/backend_udev"]`) com `default = [os quatro aliases]`, e **não** na tabela
`[workspace.dependencies]`. Motivo, medido: se as features estiverem na tabela do workspace, elas são
**incondicionais** para todo membro que usa `smithay.workspace = true`, e o critério 5 (`--no-default-features`)
deixaria de ser prova de opt-out — o build leve continuaria compilando gbm/libseat. Como só um crate usa
Smithay, o efeito no build padrão é idêntico e o opt-out passa a ser mensurável.
O esboço em `docs/plano-mestre.md:374-378` mostra as features na tabela do workspace; a forma adotada é
equivalente em build padrão e mantém o nested leve. Reverta se discordar.

## Limites
- Tempo: ~2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: **nenhuma crate/apt nova** além das que o Smithay 0.7.0 já traz e que
  `scripts/setup-deps.sh:13-19` instala. Entram no `Cargo.lock` (por aprovação do humano em 2026-11-21),
  todas MIT, **versões resolvidas medidas** por `cargo check --workspace --all-targets`:
  `drm 0.14.1`, `drm-ffi 0.9.1`, `drm-sys 0.8.1`, `gbm 0.18.0`, `gbm-sys 0.4.0`, `input 0.9.1`,
  `input-sys 1.19.0`, `udev 0.9.3`, `libudev-sys`, `libseat 0.2.4` (a exigência do Smithay é `"0.2.3"`,
  que por caret aceita 0.2.4), `libseat-sys 0.2.0`, + `bytemuck_derive`, `io-lifetimes`, `hermit-abi`,
  `linux-raw-sys` e os alvos `windows-*` (condicionados a Windows; não afetam o build Linux).
  24 pacotes novos no `Cargo.lock` (`git diff Cargo.lock | grep -cE '^\+name = '` → 24).
- Arquivos protegidos: `Cargo.lock` muda **só** por esta mudança de dependência aprovada (AGENTS.md §6).
  `protocols/*.xml`, `docs/adr/*`, `docs/security-model.md`, `packaging/*`, `rust-toolchain.toml`, `AGENTS.md`: intocados.
- Sem `sudo`, sem tocar em `/etc`, sem pacotes do sistema (AGENTS.md §3.8). Rodar em DRM exige troca de VT
  feita pelo humano.
- Regra das duas tentativas: se `cargo check` com as features falhar 2× pela mesma causa, PARE e peça decisão.

## Entrega (relatório curto, AGENTS.md §8)
O que foi feito · comando → saída crua → interpretação · o que **não** foi testado (validação em DRM real:
pendente do humano em F1d3) · descobertas laterais (STATE.md §6) · STATE.md §3/§4/§5/§6/§7/§9 atualizado ·
commit local (sem push) · **PARAR**: F1d1/F1d2 FECHADAS e F1d3 com código pronto; **não encerrar F1d3 sem o
ok visual do humano no tty3** (AGENTS.md §0.9).