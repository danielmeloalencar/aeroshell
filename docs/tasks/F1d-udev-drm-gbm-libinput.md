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
| **F1d2** (sessão seguinte, só após ok) | sonda GBM em `renderD128` (EGL → GbmAllocator) sem trocar VT, harness novo | saída da sonda registrada em STATE.md §7 |
| **F1d3** (sessão seguinte, só após ok) | backend udev completo + extração da cena de `backend/headless.rs:126-255` para `render/scene.rs` | juiz visual em tty3 (humano troca o VT e fecha) |

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

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Implementar `init_udev`, `DrmDevice`/`DrmSurface`/`DrmCompositor`, `LibinputInputBackend`,
  `LibseatSession`, notificador de sessão (`DrmDeviceNotifier`) — é **F1d3**.
- Sonda GBM/EGL em `renderD128` e harness `tests/integration/udev_drm_probe.py` — é **F1d2**.
- Extração da cena de `backend/headless.rs:126-255` para `render/scene.rs` — é **F1d3**.
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
commit local (sem push) · **PARAR**: não iniciar F1d2 sem o ok do humano.