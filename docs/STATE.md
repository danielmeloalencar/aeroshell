# STATE.md — estado atual do AeroShell (fonte única da verdade)
 
> Qualquer modelo/harness novo: leia `AGENTS.md`, este arquivo e `docs/RUNBOOK.md`, nesta ordem.
> ATUALIZE este arquivo ao fim de TODA sessão (seção "Handoff"). Mantenha-o curto (< 150 linhas):
> detalhes antigos vão para `docs/progress.md` (arquivo histórico).
 
Última atualização: 2026-11-22 · **Fase atual: F1** · **F1d1/F1d2 FECHADAS; F1d3 NÃO FECHADA (juiz negou 3×); cena/import provados no headless, `is_empty=true` é idle normal (fato do humano), falta medir frame-callbacks no udev** · F1c FECHADA pelo juiz visual · 02q FECHADA pelo juiz (harness de 300 ms segue como lacuna registrada)
 
## 1. Em uma frase
Framework para criar compositores Wayland (Rust + Smithay) cuja UI inteira (painéis, frames de janela,
lock, login) é TSX renderizado por WebKitGTK; runtime de política em Bun. Dev em nested (winit).
 
## 2. Objetivo atual (F1 autorizada)
**Fase atual: F1** (núcleo do compositor). F0/fatia vertical FECHADA pelo humano em 2026-11-21: `foot` com
decoração HTML funcional e painel React/TSX; fechamento formal em `docs/f0-relatorio-go-no-go.md` (quadro
S1–S5, ratificação registrada dos ADRs, S4→F2/F9 e S5→F5/F6 deferidos com justificativa).
F1 em andamento por cartões pequenos: F1a, F1b e F1c FECHADAS (juiz visual OK nas três; F1c em 2026-11-21);
F1d autorizada pelo humano em 2026-11-21: **F1d1 e F1d2 FECHADAS no critério técnico (§3); F1d3 implementada,
mas NÃO FECHADA — juiz visual negou 3× no tty3 em 2026-11-22 (§7). O diagnóstico novo provou que a cena e o
import do cliente funcionam no headless; o elo que falta é a apresentação GBM/DRM do udev (§3/§7).**
F1 anterior revertida em `4307cc4`.
 
## 3. Feito (com onde está a prova)
- [x] **F1d3-diagnóstico (sonda de composição do cliente), 2026-11-22** — humano corrigiu fato: `is_empty=true`
      é "sem damage, nada a submeter" (idle normal); input/VT/vblank provados bons no 4º log
      (`/home/daniel/aeroshell-udev-20261122-045035.log`, sumiu — recorte literal em
      `/tmp/opencode/f1d3-4exec/judge-log-045035-excerpt.txt`). Novo `tests/integration/udev_client_compose_probe.py`
      escrito ANTES e visto FALHando no recorte (`probe-baseline.log`: `foot_commits=0 frame_cb_samples=0`,
      `VEREDITO: FALHA`, EXIT=1). Instrumentação SOMENTE sob `bench`, sem drenar callbacks nem mudar protocolo:
      `protocols/compositor.rs::commit` → `FOOT_COMMIT t=… surface=client pending=<n>`; `udev.rs` timer →
      `UDEV_FRAME_CB t=… surface=client pending=<n>` a cada 60 ticks. Fonte da API: Smithay 0.7.0
      `wayland/compositor/mod.rs:248` (`frame_callbacks: Vec<WlCallback>`) e `desktop/wayland/utils.rs:217`
      (`send_frames_surface_tree`, que o repo NÃO chama). Gate `check.sh` EXIT=0 (`f1d3-4exec/gate-final.log`),
      clippy `--features bench` 0, `--no-default-features` 0, fmt 0, `strings` do binário sem bench = 0 para
      `FOOT_COMMIT`/`UDEV_FRAME_CB`. Sonda de commit provada em execução REAL: harness headless
      (`headless_render_damage.py` VEREDITO OK, log `/tmp/opencode/f1c/headless-14037.log`) gerou 7
      `FOOT_COMMIT` em 0,76 s com `pending_seq=0,1,1,0,1,1,0` — ou seja, no caminho que funciona os callbacks
      circulam. Classificador validado com commits reais + CB sintético (só parser, não produto):
      `SEM_DAMAGE_FORA_DA_CENA` e `CALLBACK_NAO_CHEGA` distintos. **Falta o log do tty3 com as sondas.**
- [x] **F1d3-diagnóstico (cena/import provados), 2026-11-22** — juiz negou 3× no tty3; o 3º log
      (`/home/daniel/aeroshell-udev-20261122-040108.log`, transcrito na mensagem; arquivo sumiu depois)
      mostrou `UDEV_SESSION/DRM/OUTPUT active=true`, `UDEV_FRAME … queued="ok"` e `UDEV_VBLANK … submitted="ok"`
      com `foot` mapeado, mas o juiz viu só a decoração vazia. Dois harnesses novos, escritos ANTES e vistos
      FALHANDO sem instrumentação (`/tmp/opencode/f1d3-pixels/{headless-client-baseline,element-baseline}.log`):
      `tests/integration/f1d3_headless_client_pixels.py` → `baseline_non_clear_max=0`,
      `after_non_clear_median=483/528`, `VEREDITO: OK` (cores de `foot` como `(36,36,36)/(38,38,38)`);
      `tests/integration/f1d3_client_element_probe.py` → `CLIENT_ELEMENT_PROBE … from_surface=Some err=none
      delta=1`, 68/68 linhas, `VEREDITO: OK`. A sonda `CLIENT_ELEMENT_PROBE` foi adicionada SÓ sob `bench`
      em `render/scene.rs`, sem mudar ordem/geometria/cores/cadência. Conclusão: **cena compartilhada e
      import do cliente NÃO são a causa**; o defeito está no caminho específico do udev depois da cena.
      Gate `check.sh` EXIT=0 (`gate-final2.log`), clippy bench 0, `--no-default-features` 0, s1_nested 0,
      multi_output 0, headless_render_damage 0 em D-Bus privado (`reg-headless-private.log`). Nenhum pixel do
      tty3 medido pelo agente (sem VT); nenhum juiz visual novo.
- [x] **F1d3-diagnóstico (2ª iteração, evidência de apresentação), 2026-11-22** — no log do tty3 do humano
      (`/tmp/opencode/f1d3-rediag/judge-log-032739-excerpt.txt`, recorte do texto colado): `UDEV_SESSION
      active=true`, `UDEV_OUTPUT active=true`, `UDEV_FRAME seq=1/2 queued="ok"`, `UDEV_VBLANK n=1/2/60
      submitted="ok"`. **O caminho render→queue→vblank submitted FUNCIONOU**; a tela segue negada pelo juiz.
      Dois defeitos do próprio diagnóstico corrigidos: (1) regex do analisador exigia `queued=ok` sem aspas,
      mas o tracing imprime `queued="ok"` — baseline FALHA (`frame=0 vblank=0`), depois `VEREDITO: OK`
      (`frame=6 queued=ok=2 vblank=3`); (2) atalho de VT usava keycodes evdev crus, mas o libinput do Smithay
      devolve XKB = evdev+8 (`backend/libinput/mod.rs:113-117`) — agora Ctrl=37/105, Alt=64/108, F1..F4=67..70,
      com `UDEV_KEY` para auditar. Filtro do `UDEV_FRAME` passou a registrar TODO frame não-vazio (antes só
      os 10 primeiros + 1/250, o que escondia exatamente os frames após o `foot` mapear).
      Gate/regressões: `/tmp/opencode/f1d3-rediag/gate-vtfix.log`. **Nenhum pixel medido; juiz visual pendente.**
- [x] **F1d3-diagnóstico (instrumentação + atalho VT), 2026-11-22** — cartão `docs/tasks/F1d3-diagnostico-visual.md`.
      Novo `tests/integration/udev_frame_probe.py`: baseline no recorte do log do juiz
      `/tmp/opencode/f1d3-rediag/judge-log-excerpt.txt` → `VEREDITO: FALHA` (`EXIT=1`, 0 linhas `UDEV_*`);
      `--self-test` → `EXIT=0` (fixture sintética, **não** evidência de produto). `udev.rs` passa a logar
      `UDEV_SESSION` (session.is_active), `UDEV_DRM`/`UDEV_OUTPUT` (device.is_active), `UDEV_FRAME`
      (is_empty/needs_sync/queued,primeiros 10 + 1/250), `UDEV_VBLANK`/`frame_submitted`, `UDEV_SESSION_EVENT`
      e `UDEV_VT`. Ctrl+Alt+F1..F4 agora chama `session.change_vt(n)` (antes: `grep -rn change_vt crates/` → 0).
      Gate: `CHECK_EXIT=0`, clippy bench 0, `--no-default-features` 0, sonda F1d2 0
      (`/tmp/opencode/f1d3-rediag/gate-final.log`); `cargo check -p aeroshell-compositor` 0, fmt 0.
      Regressões (`/tmp/opencode/f1d3-rediag/regressions.log`): `s1_nested.sh` EXIT=0,
      `headless_render_damage.py` VEREDITO OK, `multi_output_damage.py` EXIT=0.
      Script versionado `scripts/run-udev-diag.sh` + atalho curto `~/u` (o humano não consegue copiar/colar no tty):
      fixa `RUST_LOG=debug`, grava log e faz build/pré-checagens; `~/run-aeroshell-udev.sh` agora só repassa.
      Teste do script **nesta VM sem VT** (`/tmp/opencode/f1d3-rediag/script-run.log`): `EXIT=1` em
      `session.open()` com `EAGAIN`, mas o log `~/aeroshell-udev-20261122-032319.log` contém
      `UDEV_SESSION active=false seat=seat0` e o analisador o lê (`session=1`, `VEREDITO: FALHA`) — prova de que
      o debug chega ao instrumento. **Nenhuma imagem foi julgada por mim.**
- [ ] **F1d3 — backend udev completo + cena compartilhada (código pronto, juiz visual NEGOU)**, 2026-11-22.
      `render/scene.rs` novo: `collect_output_elements` (antes em `winit.rs`) agora é usado por headless, winit
      N>1 e udev — mesma ordem, offsets por `output_geometry`, filtro `frame_has_mapped_client` e log. O log
      `RENDER_SHARED` do headless sob `bench` foi preservado por parâmetro explícito (winit N>1 não o emite);
      o caminho N=1 do winit ficou intacto. `backend/udev.rs::init_udev` (antes stub) agora: `LibSeatSession`
      → `UdevMonitor` → `AERO_DRM_DEVICE`/`primary_gpu`/`all_gpus` → `session.open` → `DrmDeviceFd`/
      `DrmDevice` → `GbmDevice` → `EGLDisplay`/`EGLContext`/`GlesRenderer` → `GbmAllocator` +
      `GbmFramebufferExporter` → `DrmOutputManager` → conectores/CRTCs/modos via `drm::control` (sem
      `smithay_drm_extras`) → `Output` globals → evento libinput → notificadores de sessão/DRM/udev →
      render por timer de 16 ms com `render_frame`/`queue_frame`/`frame_submitted`. `input.rs`: movimento
      RELATIVO do libinput deixou de ser no-op (antes só absoluto/injeção sintética).
      **Provas (VM sem 3D):** fmt/clippy/bench/`--no-default-features` EXIT=0; `s1_nested.sh`,
      `headless_render_damage.py` (VEREDITO OK, RENDER_SHARED=22) e `multi_output_damage.py` (VEREDITO OK,
      `{0:20,1:0}`) verdes após o refactor; sonda F1d2 verde; gate `check.sh` EXIT=0 (29 Rust + 9 Bun).
      `--backend udev` nesta VM: `sessão libseat aberta seat0` + `dispositivo DRM selecionado /dev/dri/card0`
      e falha graciosa em `session.open()` com `EAGAIN` (sem VT ativo) — **não prova DRM funcionando**.
      **NÃO FECHADA**: AGENTS.md §0.9 exige o "ok" do humano olhando o tty3; nada foi aberto/fechado por mim.
- [x] **F1d2 — sonda GBM em `renderD128` + enumeração de modos DRM**, 2026-11-22. Escrito primeiro
      `tests/integration/udev_drm_probe.py` e visto falhando (`EXIT=1`: `argumento desconhecido: --probe-gbm=…`).
      Implementado `--probe-gbm[=NO]` em `cli.rs` (default `/dev/dri/renderD128`), despacho em `main.rs` ANTES do
      tracing/display/socket (stdout = só JSON) e `backend/udev.rs::probe_gbm` (protegido por `backend_gbm`, com
      fallback explícito no build leve). Usa `smithay::reexports::{gbm,drm}` — **nenhuma dependência nova**.
      **Medições (VM VirtualBox sem 3D, amostra em `/tmp/opencode/f1d2/probe3.log`):** `backend_name=drm`;
      formats RENDERING|SCANOUT = `Xrgb8888`, `Argb8888`, `Rgb565`, `Abgr8888` (Xrgb2101010/Argb2101010/
      Xrgb1555/Xbgr8888/Nv12/Yuv420/Yuyv não suportados); alocação `gbm_bo_create` e `GbmAllocator` de
      64×64 Xrgb8888, stride 256, modifier `Linear`, **dmabuf=sim nas duas**; `card0` com 8 conectores,
      `Virtual-1=Connected` com 35 modos (nativo `1920x955@60`, máx. `4096x2160@60`), 7 desconectados.
      `readelf -d` → `libgbm.so.1` em `DT_NEEDED` e `ldd` → `libgbm.so.1` + `libdrm.so.2`: **lacuna de F1d1 fechada**.
      Teste-first, gate `check.sh` EXIT=0 (29 Rust + 9 Bun), clippy normal/bench EXIT=0,
      `cargo check --no-default-features` EXIT=0, `s1_nested.sh` EXIT=0, sonda 3× com SHA idêntico.
      Casos de borda: `--probe-gbm` sem valor, `--help`, fallback sem feature (EXIT=1 com mensagem).
      **Nenhum critério visual** (sem VT/master/sessão; nada aberto/fechado). F1d3 segue pendente de ok.
- [x] **F1d1 — features/apt/docs do backend de sessão real (udev/DRM/GBM/libinput/libseat)**, 2026-11-22.
      `crates/aeroshell-compositor/Cargo.toml:[features]` ganhou os aliases `backend_udev`, `backend_gbm`,
      `backend_libinput`, `backend_session_libseat` (apontando para o Smithay 0.7.0 fixado) e `default` com os
      quatro; `scripts/doctor.sh` passou `libdrm`/`gbm`/`libseat` de opcionais para obrigatórios;
      `.github/workflows/ci.yml` ganhou `libdrm-dev libgbm-dev libinput-dev libudev-dev libseat-dev libsystemd-dev`
      (mesma lista de `scripts/setup-deps.sh:13-19`); `docs/RUNBOOK.md` tem "Rodar em DRM (udev)".
      **Provas medidas nesta sessão (VM VirtualBox sem 3D):** `cargo check --workspace --all-targets` EXIT=0;
      `cargo clippy --workspace --all-targets -- -D warnings` EXIT=0; `cargo build --workspace` EXIT=0;
      `cargo check --workspace --no-default-features` EXIT=0 **e** `cargo tree -p aeroshell-compositor
      --no-default-features` com **0** ocorrências de libseat/gbm/input/udev (opt-out real do backend pesado);
      `cargo tree -p aeroshell-compositor` com **6** crates novos no grafo (`drm 0.14.1`, `drm-ffi 0.9.1`,
      `gbm 0.18.0`, `input 0.9.1`, `libseat 0.2.4`, `udev 0.9.3`); CGUs desses 5 crates presentes no binário
      (`nm -a --defined-only target/debug/aeroshell-compositor` → 9 linhas); gate `bash scripts/check.sh`
      EXIT=0 numa única execução (29 Rust + 9 Bun); `bash tests/integration/s1_nested.sh` EXIT=0.
      Cartão: `docs/tasks/F1d-udev-drm-gbm-libinput.md`. **Nenhum critério visual** — o backend udev segue stub
      (`backend/udev.rs:20-26` devolve `Err`); nada muda na tela. F1d2/F1d3 só após ok do humano.
- [x] Compositor nested (winit) com socket próprio, xdg-shell, layer-shell — `scripts/check.sh` (S1 nested)
- [x] Painel WebKitGTK transparente como layer-shell, input regions do DOM, passagem de clique (S2/S3, validado à mão)
- [x] `aero_frame_v1` v1 (opção B: `associate` só MARCA a superfície, não atribui role; ver ADR 0004) — `protocol_tests` (17 testes)
- [x] Supervisor dentro do compositor, socketpair + `WAYLAND_SOCKET`, PDEATHSIG via rustix (ADR 0005) — `tests/integration/supervisor_lifecycle.py`
- [x] Frame em WebView com mover e redimensionar (8 arestas/cantos) — `tests/integration/frame_resize_edges.py`
- [x] `xdg-decoration` server-side (o `foot` abre mão da própria barra)
- [x] Botões maximizar/restaurar/fechar — `tests/integration/frame_buttons.py`
- [x] Regra de validação visual com juiz humano (agente executa/deixa aberto, humano testa e fecha) — `AGENTS.md §0.9`,
      `§0.7` e checklist `§9`; prova: `git diff -- AGENTS.md`
- [x] `docs/progress.md` despromovido a histórico — `AGENTS.md:3,154,262,272`, `plan.md`, `README.md`, `justfile`
      e ponteiros em `docs/plano-mestre.md` (plano ATIVO, preservado); prova: `grep -rn "progress" AGENTS.md plan.md README.md justfile`
- [x] `AGENTS_SESSAO_ADENDO.md` removido a pedido do usuário (protocolo de sessão já vive no próprio `AGENTS.md`).
- [x] Tarefa 00 — gate confiável: 3× `bash scripts/check.sh` verdes seguidas (bun 4,39/4,41/4,42 s). Falha induzida
      no S1 (`XDG_RUNTIME_DIR` inexistente) passou a mostrar `Error: PermissionDenied` + log do compositor;
      `finally` e trap TERM/INT encerram só o PID iniciado (provado: filho 13317 morreu ao matar o pai 13302);
      `grep -rn "pkill -x" tests/` vazio. Causa do flake antigo NÃO identificada (não reproduziu).
- [x] 02b (código): 2 `foot` com frame HTML cada; ✕/maximizar roteados por `window_id`. Prova:
      `python3 tests/integration/frame_window_id.py` → OK (`associate: [0, 1]`, `close 1`, janela 0 permanece).
- [x] 02b (causa raiz): `window_id` é lógico/estável, mas 6 resolvers usavam `space.elements().nth(id)` (ordem z,
      reordenada por `raise_element` no clique). Mapa estável `window_surfaces` + `window_for_id()` em `state.rs`.
      Prova: `tests/integration/frame_follows_resize.py` ANTES: A inalterada / B mudou (`100→60`, `696→732`);
      DEPOIS: A mudou (`60→20`, `696→732`) / B idêntica — mesmo comando, 1 execução cada.
- [x] 02b (não regressão): `frame_buttons.py`, `frame_resize_edges.py`, `frame_follows_resize.py` → OK.
      `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0).
- [x] 02b (clique na decoração, relato do humano): `focus_on_press` agora cai no frame via `surface_under` +
      `associated_frames` quando `element_under` não vê janela (titlebar é layer surface). Prova:
      `frame_follows_resize.py` ANTES: `ordem ... BARRA` inalterada, `foco=[]`, `barra eleva A=False`;
      DEPOIS: ordem inverte, `foco=['wl_surface@3[1]']` (A), `barra eleva A=True`.
- [x] 02b (barra da frente sobre conteúdo da de trás, relato do humano): DUAS causas corrigidas:
      (1) `surface_under` verificava o CLIENTE antes da decoração dos frames — o conteúdo de A roubava o hit da barra
      de B; agora a decoração vem primeiro e percorre o Space do topo (z-order) para baixo;
      (2) `focus_on_press` usava `element_under` (ignora layers) — trocado por `surface_under` como fonte única,
      resolvendo frame→`window_id` ou cliente→`Window`.
      Prova: ANTES `superfície em (200,100)=wl_surface@3[1]` (conteúdo de A), `frente antes y=120, depois y=80`,
      `barra da frente não passa=False`; DEPOIS `superfície=wl_surface@45[0]` (frame de B), `frente 120→120`,
      `foco=[]` (B já estava focada, foco não saltou para A), `barra da frente não passa=True`.
- [x] Gate final desta rodada: `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0);
      `frame_follows_resize.py`, `frame_window_id.py`, `frame_buttons.py`, `frame_resize_edges.py` → 4× OK.
- [x] 02b — VALIDAÇÃO VISUAL DO HUMANO (2026-11-20): "ok validado e fechada a instancia", após testar na janela
      aberta com 2 `foot` o clique na barra da janela da frente sobre o conteúdo da de trás; o próprio humano fechou
      a aplicação (`AGENTS.md §0.9`). Tarefa 02b FECHADA.
- [~] 02c (PARCIAL): instrumentação `RESIZE_SYNC_{MOTION,CLIENT,FRAME}` sob `cfg(feature="bench")` em `grab.rs`,
      `compositor.rs`; teste `tests/integration/frame_resize_sync.py` escrito e visto FALHANDO antes da correção
      (14/14, depois 13/14 passos pendentes). Correção mantida: `send_frame_configure` no próprio `ResizeGrab::motion`
      (`grab.rs:245`), em vez de só no commit do cliente. Prova com a MESMA métrica, reprocessando os logs crus:
      baseline `lag p95=177,22 ms / 13-14 pendentes`; após a correção `127,19 ms / 13-14` (−28% no lag, −1 commit).
      Critério 1 do cartão NÃO passa (falta `0` pendentes); gate `bash scripts/check.sh` verde.
- [~] 02c (VEREDITO VISUAL DO HUMANO, 2026-11-20): com a correção parcial já aplicada, arrastou a borda em ritmos
      lento e rápido e disse: **"ainda está do mesmo jeito"**. Ou seja, o ganho de 28% no lag não foi percebido na
      tela; o sintoma "decoração correndo atrás" PERSISTE. Humano pediu manter para outra tarefa e seguir em frente —
      continuação no cartão `02d-frame-resize-diagnostico.md`. Instância fechada pelo próprio humano.
- [x] 03 (tipos): `ts-rs 12.0.1` (MIT, aprovado) gera 4 bindings em `packages/sdk/src/generated/` a partir de
  `crates/aeroshell-ipc/src/types.rs`. Prova: `bun run generate:types` → `4 passed` e arquivos `WindowInfo/Snapshot/
  WindowChange/Patch.ts` com o cabeçalho "generated by ts-rs".
- [x] 03 (critério 1): `bun test packages/sdk` → `5 pass / 0 fail / 16 expect()`; store aplica snapshot, aplica
  patch Upsert/Remove e ignora revisão antiga/igual (identidade do objeto preservada, zero notificações).
- [x] 03 (IPC): `window_ipc.rs` agora lista `W id x y w h <titulo-json>` com `id` do mapa lógico `window_surfaces`
  (não `enumerate()` da ordem z) e título lido de `XdgToplevelSurfaceData.title` via `with_states`.
- [x] 03 (critérios 2/3, pré-filtro): `python3 tests/integration/sdk_cli.py` → `--list ['0\tfoot', '1\tfoot']`;
  `--focus 0` inverteu a ordem de render de `[...,@3[1],...,@3[0]]` para `[...,@3[0],...,@3[1]]`;
  `--close 1` deixou só a janela 0. Amostra crua: `/tmp/opencode/sdk_cli_run5.log`.
- [x] 03 (gate): `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0), `bun test` 6 pass / 0 fail.
- [x] 03 — VALIDAÇÃO VISUAL DO HUMANO (2026-11-20): com o nested aberto (2 `foot` + shell de frame), o humano rodou
  `--list` (2 linhas com id/título), `--focus 0` (elevou a janela certa) e `--close 1` (fechou só aquela) e respondeu
  **"ok validado"**. O próprio humano fechou a aplicação (PID 39214 encerrado); nenhum processo meu ficou órfão.
  Tarefa 03 FECHADA.
- [x] 04a (critério 1): `bun test packages/react` → `3 pass / 0 fail / 8 expect()`. O teste foi escrito antes e visto
  falhando (`Cannot find module './useWindows'`), depois passou. `useWindows` liga `store.subscribe`/
  `store.getSnapshot` ao `useSyncExternalStore`; patch notifica e unsubscribe desliga o listener.
- [x] 04a (deps): React 19.2.0 + react-dom 19.2.0 (MIT, aprovados) instalados no workspace `packages/react`;
  `bun.lock` versionado. Prova: `bun -e 'import("react")'` dentro de `packages/react` → `react real ok function`.
- [x] 04a (critérios 2/3): `bun run generate:types` → `EXIT=0` e `git diff -- packages/sdk/src/generated` vazio;
  `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0), `bun test` 9 pass / 0 fail.
- [x] 04b (pré-filtro): `python3 tests/integration/react_panel.py` → `OK: painel React listou 2 e 3 janelas;
  clique no botão da janela de trás elevou só ela; bridge continuou respondendo`. Escrito antes e visto falhando
  (`bundle ausente`, depois `bridge parou de responder após o clique`). O teste prova: bundle servido por
  `aero://panel/app.js`, lista `(id,título)` entregue via bridge, clique injetado no botão escolhido pela ordem de
  render inverte a ordem sem alterar as demais.
- [x] 04b (travamento do bridge): causa raiz reproduzida e corrigida. `forward()` esperava o `.` que só o `list`
  envia; `focus` responde `OK` e o main loop congelava (botões na tela, porém mortos). Teste de regressão escrito
  antes e visto falhando: `FALHA: bridge parou de responder após o clique`. Com `shutdown(Shutdown::Write)`:
  `bridge continuou respondendo` com 2 e 3 janelas. Commit `df89000`.
- [x] 04b (gate final): `bash scripts/check.sh` → `OK: scripts/check.sh passou`, `EXIT=0`, `bun test 9 pass / 0 fail`.
- [x] 04b (juiz visual — 2026-11-20): humano: **"tudo foi ok agora, inclusive abrir outro foot via comando em um
  dos foot"**. Prova no log `/tmp/opencode/aero-04b-fix.log`: após o 4º `foot` (`toplevel mapeado ...
  wl_surface@3[4]`), o painel listou `0 "foot" | 1 "foot" | 2 "foot" | 3 "foot"` e publicou 4 rects. Humano fechou
  a aplicação. **04b FECHADA.**
- [x] 04b (revalidação visual a pedido do humano, 2026-11-20): nova instância (`wayland-aero-04b-recheck`, compositor
  PID 54736 + shell PID 54794) com 3 `foot` e 3 botões; humano: **"ok tudo isso está funcionando"**. Clicou nos botões
  em sequência, confirmou elevação e responsividade contínua e abriu um 4º `foot` a partir de um `foot`. Log cru
  `/tmp/opencode/aero-04b-recheck.log`: 4º `foot` após `04:53:54`, lista final `0..3 "foot"`, 4 rects, 40 respostas
  `list` (bridge vivo). Humano fechou a aplicação; nenhum PID meu ficou órfão.
- [x] 02e (código): `winit.rs` usa `AeroRenderElements` (`render_elements!` do Smithay 0.7.0, padrão anvil v0.7.0)
  e `RescaleRenderElement::from_element(el, loc, scale)` quando `buffer_size() != conteúdo_commitado + extras`.
  Caminho normal (`Surface`) inalterado; move não escala; damage inalterado (`age=0` + `request_redraw`).
  Fonte das APIs: `smithay-0.7.0/src/backend/renderer/element/utils/elements.rs:12-30`, `surface.rs:269`.
- [x] 02e (teste escrito ANTES, visto falhando): `tests/integration/frame_resize_glue.py` → baseline sem
  instrumentação `FALHA: nenhuma linha RESIZE_GLUE` (`/tmp/opencode/glue_baseline.log`); instrumentado sem escala
  `FALHA: rescaled=False mas buffer=(708,530) target=(714,530)`, `VEREDITO: FALHA`
  (`/tmp/opencode/glue_instrumented_sem_escala.log`).
- [x] 02e (prova espacial, 3 execuções seguidas, nenhuma amostra descartada): `glue_final{1,2,3}.log` e
  `02e_glue_gate{1,2,3}.log` → todas `VEREDITO: OK`; `durante_grab` 182/141/165 e 87/96/117; `apos_soltar`
  89/71/67 e 46/66/63; em todas existe `buffer==target` com `rescaled=false` após soltar. Mecanismo exercitado:
  `/tmp/glue-comp-58730.log`, `-58852.log`, `-58966.log` com 104–118 linhas `rescaled=true` por execução.
- [x] 02e (vizinhos): `frame_resize_edges.py`, `frame_follows_resize.py`, `frame_buttons.py`,
  `frame_window_id.py` → 4× OK. `frame_resize_sync.py` (métrica antiga) → `VEREDITO: FALHA`, inalterado.
- [x] 02e (gate): `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, bun 9 pass / 0 fail);
  `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0.
- [x] 02e (ADR aditivo): `docs/adr/0006-frame-resize-composicao-elastica.md` com causa raiz (pipeline WebKit),
  decisão, alternativas preteridas e gatilhos de reavaliação. Nenhum ADR aceito editado.
- [x] 02f (teste escrito ANTES, visto falhando): `tests/integration/frame_configure_hygiene.py` sem instrumentação →
  `FALHA: nenhuma linha FRAME_CONFIGURE; configure do cliente 0 != motions 14; final configs=0`
  (`/tmp/opencode/02f_hygiene_baseline_sem_instr.log`). Com instrumentação e SEM mudança de comportamento →
  `configures/motions=1,857` (26/14: 14 `source=grab` + 12 `source=commit`), `VEREDITO: FALHA`
  (`/tmp/opencode/02f_hygiene_baseline2.log`); outra amostra `2,000` (`..._baseline_instrumentado.log`).
- [x] 02f (código): `grab.rs` — `ResizeGrab.last_sent_size`/`last_sent_at` + `FRAME_CONFIGURE_DEBOUNCE` 200 ms;
  configure do cliente em TODOS os motions; `unset` reabilita o commit-hook e envia UM configure `source=final`
  com `client_content_size()`. `state.rs` — `resize_grab_active: Option<u32>`; `compositor.rs:195-215` —
  commit-hook calado durante o grab. `aero_frame.rs` — `FRAME_CONFIGURE ... source=` sob `bench`.
- [x] 02f (prova, 3 execuções seguidas com o código final, nenhuma amostra descartada):
  `/tmp/opencode/02f_hygiene_finalcode{1,2,3}.log` → todas `VEREDITO: OK`; `motions=14`,
  `frame_configures=4` (razão **0,286** ≤ 1/3, só `source=grab`), `client_configures=14`,
  `finals=1` com `w,h=(834,494)` = último commit real, `glue_colados=44/31/37`, `buffers_ok=2/2/3`.
- [x] 02f (vizinhos): `frame_resize_glue.py` → `VEREDITO: OK` em 3 execuções seguidas
  (`02f_glue_fixed{1,2,3}.log`); `frame_resize_edges`, `frame_follows_resize`, `frame_buttons`,
  `frame_window_id` → 4× OK; `frame_resize_sync.py` → `VEREDITO: FALHA` (métrica antiga, inalterada).
- [x] 02f (gate): `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, bun 9 pass / 0 fail,
  `/tmp/opencode/02f_check_final2.log`); `cargo clippy --workspace --all-targets --features bench -- -D warnings`
  → EXIT=0 (`02f_clippy_bench_final2.log`). Nenhum arquivo protegido alterado, nenhuma dependência nova,
  nenhum `unsafe`/`#[allow]`/`unwrap` novo.
- [x] 02j (teste primeiro, ANTES de instrumentar): `cargo test -p aeroshell-compositor frame_diagnostics` →
  4 falhas (`/tmp/opencode/02j_diag_baseline_unit2.log`); a 1ª versão do stub tinha 1 teste que passava
  trivialmente, então a asserção `pending_count()==1` foi adicionada e o baseline refeito
  (`02j_diag_baseline_unit.log`, preservado). Harness `tests/integration/frame_diagnostics.py` →
  `VEREDITO: FALHA`, 0 linhas (`/tmp/opencode/02j_diag_baseline_harness.log`). Reprodução no arrasto real negado
  da 02f: `FRAME_NEVER_ACKED=0`, `FRAME_NEVER_DREW_AT_SIZE=0`, `FRAME_STRETCH_ACTIVE=0`, só 1 `FRAME_TIMEOUT`
  sintético em `/tmp/opencode/aero-02f-visual.log`.
- [x] 02j (código): módulo puro `frame_diagnostics.rs` com 3 modos (`ACK_TIMEOUT` 100 ms, `DRAW_TIMEOUT`
  300 ms, `STRETCH_IDLE` 500 ms), dedupe por serial e episódio; 5 pontos de observação em `aero_frame.rs`
  (`note_configure`, `note_ack`), `compositor.rs` (`note_buffer`), `winit.rs` (`note_stretch`), `state.rs`
  (`tick`/`flush`). `#[cfg(any(test, feature="bench"))]` no `mod` e `#[cfg(feature="bench")]` em
  `now_ns`/`with_fault_injection`/`flush` — sem `#[allow]`, sem desligar checagem.
- [x] 02j (prova, 3 execuções seguidas de cada fase, nenhuma amostra descartada):
  `/tmp/opencode/02j_diag_final{1,2,3}.log` (normal) e `02j_diag_faults_final{1,2,3}.log` → 6× `VEREDITO: OK`,
  0 linhas malformadas. Fase `faults` gera 1 `FRAME_NEVER_ACKED`, 1 `FRAME_NEVER_DREW_AT_SIZE` e 1
  `FRAME_STRETCH_ACTIVE` por execução, todos com `serial`, `window_id`, `w/h`, `target_w/h`, `buffer_w/h`,
  `waited_ms`, `start_t`, `end_t`, `duration_ms`, `max_scale`. Unitário: 4 passed
  (`02j_unit_final.log`). Nenhuma conclusão de desempenho: VM sem 3D.
- [x] 02j (vizinhos): `frame_resize_glue.py` 3× OK; `frame_configure_hygiene.py` 3× OK com retry de press
  registrado (`02j_hygiene_retry{1,2,3}.log`, todas `1=grab`); `frame_resize_edges.py` 3× OK com retry e
  polling (`02j_edges_retry{1,2,3}.log`); `frame_follows_resize.py`, `frame_buttons.py`, `frame_window_id.py`
  → OK; `frame_resize_sync.py` → `VEREDITO: FALHA` (métrica antiga, inalterada).
- [x] 02j (gate): `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, bun 9 pass / 0 fail,
  `/tmp/opencode/02j_check_final3.log`); `cargo clippy --workspace --all-targets --features bench -- -D warnings`
  → EXIT=0 (`02j_clippy_bench_final.log`); `cargo fmt --all -- --check` → EXIT=0. Binário sem bench: build OK e
  `strings target/debug/aeroshell-compositor | grep -cE 'FRAME_NEVER_ACKED|FRAME_NEVER_DREW_AT_SIZE|FRAME_STRETCH_ACTIVE'`
  → **0**. Nenhuma dependência, protocolo, ADR ou arquivo protegido alterado.
- [x] 02k (código, critério técnico): 9-slice no render (`Relocate<Rescale<Crop<E>>>`, 9 fatias com
      `fgeo.insets` como linhas de corte) e congelamento do buffer no grab (UM `grab-start` grande, nenhum
      configure durante, UM `final`). Baseline FALHOU (`02k_baseline2.log`: `edge_l` 5,97–6,36 e `title_h`
      29,43–31,94 contra insets 6/30; snap 0,0727); depois 3× OK (`02k_glue_final{1,2,3}.log`: 6,00/30,00,
      snap 0,0163, `SLICE_COMPOSE n=9` em todas as amostras). Gate `02k_check_final.log` EXIT=0; clippy bench
      `02k_clippy_final.log` EXIT=0; vizinhos OK conforme §6. Critério VISUAL ainda pendente (§7).
- [x] 02l (código, critério técnico): protocolo `set_titlebar_slices(left,right)` (autorização explícita do
      humano em 2026-11-20), `FrameGeometry.titlebar_slices`, handler, shell declarando `(120,96)` e HTML com
      zona fixa do título; 9-slice com 3 fatias no topo (texto/botões com escala 1:1, só o intervalo estica) e
      deslocamento das bordas direita/inferior para cobrir o alvo. **Teste primeiro visto falhando**:
      `02l_baseline1.log` → `title_n={1} declaradas={(0,0)} left=[696.00,834.00] right=[0.00,0.00]` e
      VEREDITO FALHA (zona do texto crescia com `slice_sx`). Depois 3× seguidas OK (`02l_glue_final{1,2,3}.log`):
      `title_n={3} declaradas={(120,96)} left=[120,120] right=[96,96] gap=[480,618]`, `SLICE_COMPOSE n=9`,
      `SLICE_GEOM` com 9 fatias cobrindo x 54→762 e y 50→580 (`02l_after5.log`). Gate
      `02l_check_final3.log` EXIT=0, bun 9 pass/0 fail; clippy bench `02l_clippy_final3.log` EXIT=0;
      vizinhos OK, `frame_resize_sync.py` FALHA esperada. Critério VISUAL: ver §7.
- [x] 02m (baseline, TESTE PRIMEIRO VISTO FALHANDO): novo `tests/integration/frame_initial_buffer.py` abre 2
      `foot` e inspeciona TODAS as linhas `RESIZE_SLICE` da janela 1, sem descartar amostra. Baseline cru
      `/tmp/opencode/02m_baseline1.log` → EXIT=1, 8 amostras `buf=200x200 target=708x530 rescaled=true
      slice_scale_x=3.7021` (t=1795161396743200190 … 1795161396789066653, ~4,6 ms) antes do primeiro buffer
      real `708x530 rescaled=false`. `frame_requested` cru: `window_id=1 conteudo=0x0` seguido de
      `window_id=1 conteudo=1280x800` (ignorado: "já tem WebView"). Janela 0 sem anomaly (233 amostras, todas
      `708x530`) — defeito restrito ao caminho `frame_requested`. Spike de API (≤20 min): `gtk4 0.11.5`
      expõe `set_default_size` (`~/.cargo/.../gtk4-0.11.5/src/auto/window.rs:817`), documentado em
      `/usr/share/gir-1.0/Gtk-4.0.gir:185031` como o tamanho usado "if no other constraints apply".
- 02m (PRIMEIRA TENTATIVA FALHOU — revertida): adiar `open_frame_window` até `frame_requested` com tamanho
  >0 criou **deadlock lógico**. `/tmp/ibuf-comp-102909.log` → só `frame_requested window_id=1 conteudo=0x0`,
  nenhum `associate` da janela 1; teste → `FALHA: timeout esperando 2o associate`. Causa lida no código:
  `xdg_shell.rs:74-80` envia 0x0 no `map` (antes do associate) e `aero_frame.rs:286-294` envia 1280x800 no
  `associate`, que depende da WebView existir. Humano (2026-11-20) confirmou o sintoma na tela:
  **"abriu duas janelas do foot, uma com decoracao e outra sem"**. NÃO tentar essa via de novo.
- [x] 02m (CORREÇÃO + PROVA, técnica pronta): 2ª tentativa no shell — criar a WebView imediatamente, mas com
  `set_default_size` usando o `total` já configurado da janela âncora (`FrameEntry::total` = 708×530), antes do
  `present()`. Fonte da API: `gtk4 0.11.5` `window.rs:817` + GIR `:185031` ("if no other constraints apply").
  Mesmo comando do baseline, 3× seguidas: `/tmp/opencode/02m_final{1,2,3}.log` → todas `VEREDITO: OK`,
  nenhuma amostra `buf=200x200`; 1º frame da janela 1 já `buf=708x530 target=708x530 rescaled=false
  slice_scale_x=1.0000` (`02m_after2.log`). Vizinhos OK: `frame_configure_hygiene`, `frame_buttons`,
  `frame_follows_resize`, `frame_window_id`, `frame_resize_glue`; `frame_resize_sync.py` FALHA esperada.
  Gate `02m_check_final.log` EXIT=0 (`OK: scripts/check.sh passou`, bun 9 pass/0 fail); clippy bench
  `02m_clippy_final.log` EXIT=0. Critério VISUAL: ver §7. Nenhuma mudança em 02l/protocolo/ADR.
- [x] 02m (TERCEIRA ABORDAGEM — COMPOSITOR, AUTORIZADA PELO HUMANO EM 2026-11-20): humano reescreveu o
  cartão com especificação técnica: remover o `send_frame_configure(window_id, big, "grab-start")` de
  `start_geometry_grab` (`grab.rs:460-470`); nenhum configure da decoração durante o grab; manter só
  `source=final` no `unset` (`grab.rs:403-420`); buffer congelado no último tamanho assentado; 9-slice já
  compõe com `buffer < target` (`winit.rs:244-289`, confirmado no código). Sem protocolo/shell/deps.
  Teste PRIMEIRO novo `tests/integration/frame_grab_configure_hygiene.py` correlaciona `FRAME_CONFIGURE` com
  press/release: baseline cru `02m_grab_baseline1.log` → EXIT=1, `source=grab-start w=1280x800` em +26,93 ms,
  `source=final` em +508,49 ms. Depois: 3× limpas seguidas `02m_grab_clean{1,2,3}.log` → todas
  `configures_no_grab=0 source_final_apos_release=1 VEREDITO: OK`. Vizinhos: `frame_configure_hygiene.py`
  exigia 1 `grab-start` (critério antigo invertido) — asserção atualizada só para ZERO, com justificativa no
  código do teste; `frame_buttons`, `frame_window_id`, `frame_resize_glue`, `frame_initial_buffer` OK;
  `frame_follows_resize.py` teve `detected=None` (flake de captura de log) 1× e 2/2 OK isolados depois;
  `frame_resize_sync.py` FALHA esperada. Gate `02m_v2_check_final.log` EXIT=0 (bun 9 pass/0 fail); clippy
  bench `02m_v2_clippy_final.log` EXIT=0.
  JUIZ VISUAL (2026-11-20, instância v3): **"o problema no clique/arrasto resolveu totalmente"** — o critério
  desta correção (zero configures no press/drag) PASSOU no juiz. Porém o humano relatou um sintoma NOVO no
  release, registrado em §7/§9; por isso 02m/02l ainda NÃO fecham. Gate/clippy verdes após o commit final
  (`02m_v2_check_aftercommit.log`, `02m_v2_clippy_aftercommit.log`), commit `4208226`.
- [x] 02n AUTORIZADA (2026-11-20) — 4ª correção, NÃO é nova família: completa a 3ª (press/arrasto já aprovados
  no juiz) atacando a mesma causa-raiz (apresentar buffer mid-resize). Humano reenviou spec (mensagem anterior
  com Hipótese A/B foi descartada). Cartão `docs/tasks/02n-swap-atomico-release.md`. Evidência:
  `/tmp/gh-comp-110069.log` — após `source=final`, buffer 708x530 (9-slice correto) por ~80 ms, swap para
  768x582 com flicker (layout/slices inconsistentes por 1+ frame). Correção: continuar 9-slice no release,
  aposentar só quando `buffer_size==total_final` E a referência/versão das slices casar com o buffer, swap
  atômico, adiar `source=final` para idle (~150-250 ms). Sem protocolo/shell/deps/ADR.
  LEITURA DE CÓDIGO (Passo 0, antes de editar): `resize_grab_active` é lido SÓ no commit-hook
  (`compositor.rs:215`), não controla o 9-slice; o 9-slice é decidido por `rescaled = buffer != target`
  (`winit.rs:248-251,359-452`). `titlebar_slices` é declarada UMA vez no associate
  (`frame_webview.rs:368` → `aero_frame.rs:434-444`); NÃO existe versão de slices no código (confirmado por
  grep: só `acked`/serial do configure em `aero_frame.rs:505-528`). A "referência/versão" da spec será
  implementada como estado interno do compositor (geração de congelamento), sem tocar no protocolo.
- [x] 02n IMPLEMENTADA E PROVADA (mecânica), juiz visual PENDENTE (§7). Correção só no compositor, 5 arquivos:
  `state.rs` (`ReleaseSwap`, constantes 200/50/400 ms, `slice_generation`, `tick_release_swap`),
  `grab.rs` (`unset` cria o release pendente em vez de enviar `source=final`; novo grab cancela),
  `winit.rs` (render força 9-slice até `buffer==target` + geração casa + idle enviou final + 50 ms de
  assentamento; aposenta num único render), `aero_frame.rs` (incrementa geração em `SetTitlebarSlices`,
  limpa estado no `Destroy`), `main.rs` (chama `tick_release_swap` no idle).
  PROVA: baseline cru `/tmp/opencode/02n_release_baseline2.log` →
  `frames_buffer_inconsistente=1 swaps_atomicos=0 VEREDITO: FALHA` (buf 708x530 rescaled=true →
  768x582 rescaled=false no mesmo frame, ~60-80 ms após o final). Depois: 3× seguidas OK em
  `/tmp/opencode/02n_release_clean{1,2,3}.log` → `frames_buffer_inconsistente=0 swaps_atomicos=1`.
  Evidência do ciclo em `/tmp/rsw-comp-*.log` da execução 1:
  `RELEASE_SWAP_BEGIN` → `RELEASE_SWAP_FINAL delay_ms=205.0` → `RELEASE_SWAP_MATCH buf=768x582 gen=1` →
  `RELEASE_SWAP_ATOMIC waited_ms=57.1`. Teste: `python3 tests/integration/frame_release_swap_hygiene.py`.
  Vizinhos: `frame_grab_configure_hygiene`, `frame_buttons`, `frame_window_id`, `frame_resize_glue`,
  `frame_initial_buffer`, `frame_follows_resize` OK; `frame_configure_hygiene` exigiu atualização da janela
  de classificação do `source=final` (era +100 ms, era da 02m; agora +400 ms, ver §6) → OK com
  `finals=1 ultimo_cliente=(834,494)`; `frame_resize_sync` FALHA esperada (métrica antiga de p95).
  Gate: `bash scripts/check.sh` → CHECK=0, bun 9 pass/0 fail (`/tmp/opencode/02n_check_final3.log`);
  `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0
  (`/tmp/opencode/02n_clippy_final.log`).
- [~] 02o (MEDIÇÃO, NÃO CORREÇÃO — PARCIAL): instrumentação `frame_pixel_probe.rs` sob `bench` lê pixels reais do
  framebuffer com `ExportMem::copy_framebuffer`+`map_texture` (API real Smithay 0.7.0) e grava grade 64x40 em
  `/tmp/opencode/02o_pixels_*/frame_*.bin`. Baseline sem instrumentação:
  `/tmp/opencode/02o_pixel_baseline2.log` → `frames_capturados=0 ... VEREDITO_MEDICAO: NAO_CAPTUROU`.
  Depois: 180-186 `PIXEL_FRAME` por execução, 0 erros EGL; 3 execuções em `/tmp/opencode/02o_pixel_run{1,2,3}.log`:
  run1 `CAPTUROU` (`PAR diff_medio=0.00`), runs 2/3 `PAR_NAO_CAPTURADO`. `flashes_candidatos=0` e
  `diff_medio_max=4,81` nas 3 — insuficiente para explicar o que o humano viu (§7). Gate CHECK=0
  (`/tmp/opencode/02o_check_final.log`), clippy bench EXIT=0 (`/tmp/opencode/02o_clippy2.log`).
- [x] 02p (CANVAS FIXO, FECHADA): shell com canvas inicial output+insets, nenhum `set_default_size`
  no resize comum, 9-slice em TODO frame e `frame_painted(serial)` após double-rAF. `ReleaseSwap` e
  timers 200/50/400 ms removidos; crescimento só com paint do serial pendente. **Teste full-res visto
  falhando:** `/tmp/opencode/02p_pixel_baseline3.log` → 2 transientes de 1.910 pixels com target
  constante, `changed_pixels=3820 native_frames=107 canvas_changes=1 VEREDITO: FALHA`; o humano também
  viu o flicker rápido nessa execução. Depois, 3 execuções completas (`02p_pixel_after{2,3,4}.log`):
  todas 8/8 motions, target 768x582, `changed_pixels=0 native_frames=0 canvas_changes=0 VEREDITO: OK`.
  Growth real (`02p_pixel_growth1.log`): 1292x836→1344x1167, `growth_completed=1 changed_pixels=0`.
  Regressões: insets do associate (`canvas_tracks_insets_from_initial_associate`, baseline 1280x800→
  esperado 1292x836) e ativação repetida (`frame_repeated_activate.py`, ANTES `duplicate=True
  shell_died=True associates=2`, DEPOIS tudo falso/1, `activate_ignorado=True`).
  Vizinhos: buttons, 8 edges, resize glue, grab/configure/fixed-canvas hygiene, initial buffer,
  window_id, follows_resize e diagnostics normal/faults OK. Gate `02p_check_final.log` EXIT=0;
  clippy bench `02p_clippy_final.log` EXIT=0. ADR novo 0007; nenhum ADR anterior editado.
- [x] 02p — JUIZ VISUAL DO HUMANO (2026-11-20): após testar a instância v2 com 2 `foot`, o humano
  respondeu **"teste visual ok. Passou"**. Instância encerrada pelo próprio humano (notificações dos
  processos); nenhum encerramento foi feito pelo agente. Tarefa FECHADA.
- [x] 02q (FECHADA POR DECISÃO DO JUIZ HUMANO): `tests/integration/frame_cold_start.py` foi escrito antes e visto
  FALHANDO no baseline (`02q_cold_baseline2.log`): janelas 1/2 = **469,320/543,026 ms**, 0 placeholders e
  **3 WebKitWebProcess** (a baseline de 1–2 s NÃO reproduziu). Implementado: placeholder sólido em `winit.rs`;
  âncora oculta + `related_view`; pool de 2 views com HTML carregado, consumo por request e reposição em idle.
  Mesmo teste na via A (`02q_pool_loaded_reverted.log`): takes 0/1/2, pool final=2, placeholders e **1 processo**,
  mas **374,387/415,202 ms** (>300); outra amostra da mesma implementação: 363,121/412,006 ms. O harness de
  tempo NÃO passa e nenhuma amostra foi editada/descartada; o humano decidiu explicitamente que sua verdade
  visual prevalece ("Visualmente está perfeito e bem otimizado") e FECHOU a tarefa. Vizinhos finais
  initial_buffer/fixed_canvas/repeated_activate/buttons/window_id → 5× OK
  (`02q_final2_*.log`). Gate `02q_check_final.log` EXIT=0 (27 Rust + 9 Bun); clippy bench `02q_clippy_final.log`
  EXIT=0. Nenhuma conclusão de desempenho (VM sem 3D).
- [x] 02q — JUIZ VISUAL DO HUMANO (2026-11-21): instância manual com 3 `foot`, decoração imediata e instrução
  de abrir outra janela; humano respondeu **"ok. aprovado"**. Antes, também relatou que no harness as janelas
  abriram rápido e a decoração carregou instantaneamente. Instância encerrada pelo humano, não pelo agente.
  O critério VISUAL passou. Depois, o humano afirmou explicitamente que sua verdade prevalece sobre o harness,
  que está "visualmente perfeito e bem otimizado" e FECHOU a 02q; a falha do cronômetro fica como lacuna de
  medição em §7, não como reabertura automática.
- [x] F0 FECHADA e F1 AUTORIZADA pelo humano em 2026-11-21: "Com isso fechamos a fase 0 f0 do plano master
  e ja poderemos ir para o f1". Registro no `docs/plano-mestre.md` §7; F1 NÃO iniciada nesta sessão.
- [x] F0 (FECHAMENTO FORMAL, cartão 0): `docs/f0-relatorio-go-no-go.md` consolida S1–S5 com números já medidos
  e GO/NO-GO por spike (S2 NO-GO em WPE → GO WebKitGTK-cliente + canvas fixo; frame GO no juiz); S4 deferido
  para F2/F9 e S5/PAM-greetd para F5/F6, com justificativa. Cartão de rastreabilidade:
  `docs/tasks/F0-fechamento-formal.md`. ADRs: **somente a linha de status** de 0001 (ratificado pelo
  relatório F0) e 0006 (superado por 0007); conteúdo técnico intacto e diff mostrado ao humano.
  `grep -n "Fase atual: F1" docs/STATE.md` → encontrado; "modo: SPIKE" removido.
  Gate: `bash scripts/check.sh` → `OK: scripts/check.sh passou` (saída colada no relatório da sessão).
- [x] F1a (REIMPLEMENTAÇÃO, CÓDIGO + JUIZ VISUAL, FECHADA): após `4307cc4 revert(f1)`,
  `render/mod.rs` + `render/elements.rs` voltam a abrigar `AeroRenderElements`, `FrameSlice`, `frame_target`
  e `push_frame_elements` (9-slice do canvas fixo + placeholder), escritos do zero a partir do bloco de
  `backend/winit.rs` (que passou de 712 para 386 linhas e só monta a entrada/consome o resultado). Nenhuma
  mudança de geometria, escala, ordem, cores, timers ou formato de log. Cartão:
  `docs/tasks/F1a-render-compartilhado.md`.
  **Teste primeiro visto falhando:** `cargo test -p aeroshell-compositor render_compartilhado` →
  `E0433: cannot find render in the crate root` (EXIT=101, `/tmp/opencode/f1a-v2/test_baseline.log`);
  depois `1 passed`.
  **Harnesses antes/depois** (`/tmp/opencode/f1a-v2/{baseline,after}_*.log`): glue `VEREDITO: OK` nas duas
  rodadas com `edge_l=[6.00,6.00] edge_r=[6.00,6.00] title_h=[30.00,30.00]`, `titlebar n={3} left=120
  right=96 gap=[480,618]`, `SLICE_COMPOSE n_min=9` (293→322 linhas de cadência; 0→1 malformada por
  interleaving, registrada); `FRAME_PLACEHOLDER` 2 linhas antes/depois. fixed_canvas, release_pixels,
  buttons, window_id, configure_hygiene, grab_hygiene, initial_buffer, diagnostics normal/faults → OK.
  `frame_follows_resize.py` FALHOU no baseline (resize não engatou) e passou depois;
  `frame_fixed_canvas_hygiene.py` falhou depois (timeout do 1º buffer) e passou na **repetição diagnóstica
  única** (`after_fixed_canvas_hygiene_retry1.log`). `frame_cold_start.py` segue baseline-falha (§7).
  Gate `bash scripts/check.sh` → `OK: scripts/check.sh passou`; clippy default e `--features bench`
  `-D warnings` → EXIT=0; fmt → EXIT=0.
- [x] F1a — JUIZ VISUAL DO HUMANO (2026-11-21): nested aberto com 2 `foot` (compositor PID 12983, shell
  12993, socket `wayland-aero-f1a-visual`, log `/tmp/opencode/f1a-v2-visual.log`), deixado aberto para teste:
  barras/bordas das duas janelas, clique na barra da frente sobre o conteúdo da de trás, resize lento/rápido
  e botões maximizar/restaurar/fechar. Humano respondeu **"Aprovado"**. Critério visual PASSOU. A instância
  não está mais em `ps`; nenhum processo foi encerrado pelo agente. **F1a FECHADA.**
- [x] F1b (REIMPLEMENTAÇÃO, código + prova técnica, FECHADA PELO HUMANO em 2026-11-21): `backend/headless.rs`
  passou a renderizar com `GlesRenderer` EGL surfaceless (`EGLSurfacelessDisplay` → `EGLDisplay`/`EGLContext`,
  `GlesRenderbuffer` offscreen via `Offscreen`/`Bind`), `OutputDamageTracker` por output e timer calloop de
  16 ms. A cena usa o **mesmo** `render::elements::push_frame_elements` do winit (que NÃO foi tocado, nem o
  módulo compartilhado). Dois `unsafe` com `// SAFETY:` (display surfaceless e contexto do renderer). Nenhuma
  feature do Cargo, apt, protocolo ou ADR novo. Cartão: `docs/tasks/F1b-headless-render-damage.md`.
  **Teste primeiro visto falhando:** `python3 tests/integration/headless_render_damage.py` →
  `FALHA: timeout esperando HEADLESS_FRAME` (EXIT=1, `/tmp/opencode/f1b/baseline.log`). Depois, 3 execuções
  seguidas + 1 final no código commitado (`/tmp/opencode/f1b/after{1,2,3}.log`, `final.log`) → `VEREDITO: OK`:
  116/104/103 `HEADLESS_FRAME`, 45/43/44 `RENDER_SHARED`, 116/103/102 `PIXEL_FRAME`, dumps 64×40×3 = 7680
  bytes com conteúdo não nulo.
  **Render compartilhado provado nos DOIS branches** (log cru `/tmp/headless-comp-15422.log`):
  `kind=frame` = 33 linhas (9-slice real, com 33 `RESIZE_GLUE`) e `kind=placeholder` = 12; 4 shas distintos
  de `PIXEL_FRAME` (clear → placeholder → frame); nenhum `import do frame falhou`.
  **Damage medido, NÃO otimizado:** `damage=1280x800@0,0` em TODOS os frames — `age=0` marca tudo
  (`smithay-0.7.0/src/backend/renderer/damage/mod.rs:213-231`: `damage: Option<&Vec<_>>`, `None` = skipped)
  e o alvo offscreen não tem buffer-age. Nenhum claim de redução de damage (§7); incremental fica para F1c.
  Gate `bash scripts/check.sh` → `OK: scripts/check.sh passou`; clippy default e `--features bench`
  `-D warnings` → EXIT=0; fmt → EXIT=0. Regressão nested (`/tmp/opencode/f1b/nested_*.log`): 10/12 OK na
  1ª rodada; `frame_configure_hygiene.py` falhou (`frame WebView não commitou buffer inicial`) e passou na
  **repetição diagnóstica única**; `frame_cold_start.py` segue baseline-falha (§7).
- [~] F1c (CÓDIGO + PROVA TÉCNICA; JUIZ VISUAL PENDENTE): winit N>1 renderiza cada output em
  `GlesTexture` persistente com tracker próprio e coordenadas locais, compõe os tiles na janela host;
  N=1 segue no ramo original com `age=0`. Headless usa `age=1` após o primeiro draw. Nenhuma feature,
  dependência, protocolo, ADR, geometria do 9-slice ou arquivo protegido novo/alterado.
  **Testes primeiro vistos falhando:** `multi_output_damage.py` sem `OUTPUT_FRAME output=aero-1`
  (`/tmp/opencode/f1c/baseline-multi-output-final.log`); headless sem `age>0/none` nem dano parcial
  (`f1c/baseline-headless.log`). Depois, multi final 5/6 (`f1c/after-multi-content-final{5,6}.log`)
  → `VEREDITO: OK`: 3 frames estáveis por output com `age=1 damage=none`, dano parcial após o 2º
  foot e dano parcial em `aero-1` após arrastar a janela 1 para o segundo tile. Final 5 preservou
  1 linha malformada por interleaving; final 6 teve 0. Headless: 4× OK (`after-headless-age{1,2,3}.log`,
  `final-headless.log`), com `kind=frame`, `age=1 damage=none`, retângulo parcial e célula da titlebar
  `rgb=(43,43,43)` vs clear `(20,20,25)` (`diff=21,33`), não apenas bytes não nulos.
  Regressão nested (`f1c/regression/`): 11 harnesses OK; `frame_cold_start.py` manteve a baseline-falha
  (421,578/434,266 ms > 300 ms), sem repetição para esconder. Gate `f1c/check-final.log` EXIT=0
  (28 Rust + 9 Bun); clippy bench e fmt EXIT=0 (`f1c/{clippy-bench,fmt}-final.log`). Juiz visual: §7/§9.
- [~] F1c (JUIZ VISUAL 1 NEGADO + CORREÇÃO OBJETIVA): o print humano mostrou os 2 foot de ponta-cabeça.
  A sonda host reproduziu `rgb=(20,20,25) diff=0` na região da titlebar
  (`orientation-baseline-double-flip-private-bus2.log`). `Normal` sozinho NÃO bastou: o framebuffer real era
  1280×800 contra tracker 1280×480; `request_inner_size` (API winit 0.30.13 `window.rs:764-800`) corrigiu o
  tamanho real. Host `Normal` + resize explícito → 2× `VEREDITO: OK`
  (`orientation-after-normal-request-size{,3}.log`), `size=1280x480`, titlebar `rgb=(43,43,43) diff=21,33`.
  Falhas intermediárias de harness e colisão D-Bus estão preservadas em §6. Gate pós-correção
  `check-after-orientation.log` EXIT=0 (28 Rust + 9 Bun), clippy bench/fmt EXIT=0. Novo juiz visual PENDENTE.
- [x] F1c (JUIZ VISUAL 2 — moldura ocupando metade da tela, CORREÇÃO OBJETIVA): humano relatou por texto
  "estou vendo uma moldura ocupando a metade inteira da tela"; o modelo não lê imagem, então o relato
  prevaleceu sobre a medição anterior (~400×336). Nova medição no dump do host (64×40 células/1280×480):
  output 1 tinha `(43,43,43)` em 32/32 colunas (640 px) em y=38..39; output 0, 20/32 colunas (foot).
  Canvas inicial 652×516 é por design (ADR 0007: >= output + insets); a causa foi o fallback de "frame
  sem cliente mapeado" (`winit.rs:184-202`) desenhar a layer surface do OUTRO output inteira, sem 9-slice,
  na origem (0,0) de cada output. Correção: `state.rs frame_has_mapped_client()` + fallback em `winit.rs`
  pula frames cujo cliente já mapeou (desenhados pelo 9-slice no output do cliente). Teste primeiro em
  `multi_output_damage.py` (`decoração_por_metade`): baseline FALHOU (`{0:20, 1:32}`,
  `oversize-baseline.log`), depois 2× OK (`{0:20, 1:0}`, `oversize-after{,2}.log`). Gate
  `check-oversize.log` EXIT=0 (29 Rust + 9 Bun), clippy bench/fmt EXIT=0.
  **JUIZ VISUAL F1c (2026-11-21): humano respondeu "aprovado visualmente"** na instância aberta
  (2 outputs, 2 `foot`, arrasto dos dois lados sem piscar). O próprio humano encerrou a aplicação;
  nenhum PID do agente ficou órfão. **F1c FECHADA.** Gate final de fechamento `check-f1c-fechada.log`
  EXIT=0 (29 Rust + 9 Bun), clippy bench/fmt EXIT=0.
## 4. Em andamento
- **F1d3 NÃO FECHADA — juiz negou 3×; diagnóstico separou cena/import de apresentação (§3/§7).**
  Cartões: `F1d-udev-drm-gbm-libinput.md` e `F1d3-diagnostico-visual.md`. Headless prova que o cliente vira
  elemento e aparece em pixels; udev aceita frames/vblank, mas o juiz vê só a moldura. Fato novo do humano:
  `is_empty=true` é idle normal, **não** é bug e não deve ser alterado; input/VT/vblank estão provados bons.
  Sinal em aberto: `is_empty` fica `true` ~47 s durante interação com o cliente. Instaladas as sondas
  `FOOT_COMMIT`/`UDEV_FRAME_CB` (§3); **falta o log do tty3 com elas** para classificar
  `CALLBACK_NAO_CHEGA` vs `SEM_DAMAGE_FORA_DA_CENA`. Nesta VM (sem VT) `--backend udev` falha em
  `session.open()` com `EAGAIN`: **não posso medir o pending do udev**. Cursor segue ausente (§6).
  Push WIP pedido pelo humano falhou por credencial (`could not read Username`); commits locais
  `f0da56a`/`a56b85a` prontos para o humano empurrar. **Não é push de conclusão**.
- F1c FECHADA (§3): prova técnica + juiz visual "aprovado visualmente" em 2026-11-21. Aplicação encerrada
  pelo humano (não pelo agente). F1d foi autorizada em 2026-11-21 e F1d1/F1d2 foram executadas (§3);
  F1d3 segue pedindo aprovação separada. Sem push.
- A 02p foi concluída com pixels, vizinhos, gate/clippy e juiz visual positivos (§3).
- 02o SUBSTITUÍDA pela medição/correção 02p; não ajustar sua grade antiga nem reabrir sem novo pedido.
- 02n: **JUIZ VISUAL NEGOU (2026-11-20)** — humano: "problema ainda ocorre". Técnica/gate/clippy provados 3× (§3),
  mas o dado visual prevalece; 0 frames inconsistentes no teste NÃO mede o que o humano viu (lacuna registrada em §7).
  A instância `wayland-aero-02n` (PID 118815, shell 118824) NÃO está mais em `ps` — terminou durante a 02o,
  sem intervenção do agente. **PARAR**: cartão 02n proíbe 5ª correção sem nova decisão do humano.
- 02m (3ª abordagem): juiz CONFIRMOU que clique/arrasto está resolvido (§3). Sintoma NOVO no release com arrasto
  (flicker) é o alvo da 02n (autorizada). Press/arrasto NÃO foi alterado pela 02n.
- 02l EM EXECUÇÃO: código/teste/gate/clippy prontos (§3); falta a validação visual com juiz humano (§7).
  Humano (2026-11-20): **"vi na execução dos seus testes que o problema foi resolvido."** Registrado como dado
  favorável, mas veio da execução dos harnesses, não de uma sessão visual formal com 2 `foot`; por §0.9 ainda
  é necessário abrir a aplicação, deixá-la aberta e obter o "ok" olhando a tela.
- 02k: o achatamento relatado pelo juiz foi tratado pela 02l (cartão novo aprovado), sem alterar a 02k.
  02k segue sem "ok" visual formal próprio; não reabrir sem novo pedido (§6/§7).
- 02j FECHADA no critério técnico (§3): instrumentação auditável, gate verde, 6× `VEREDITO: OK`, strings de
  produção = 0. **Não há critério visual novo no cartão**; esta tarefa não muda aparência, geometria, cadência
  nem textura. Só não pode ser chamada de "validada visualmente" — ver §7.
- 02f NÃO CONCLUÍDA: código, teste, 3 repetições, vizinhos e gate prontos (§3), **mas o juiz visual negou**:
  humano: **"A moldura ainda está chegando atrasada"** (critério 5 do cartão FALHOU). Instância aberta com
  2 `foot` (PID 72480, socket `wayland-aero-02f`); não encerrada pelo agente. Aguardando decisão do humano —
  NÃO iniciar nova abordagem por conta própria (AGENTS.md §3.12 / §0.4).
- 02e PERMANECE PARCIAL/NÃO CONCLUÍDA: juiz visual relatou "atraso menor, porém ainda feio; a moldura chega
  atrasada". Código/teste/ADR/gate verdes não bastam. Instância `wayland-aero-02e` (PID 63221) foi encerrada
  pelo humano, não pelo agente. NÃO reabrir 02e sem novo pedido.
- 02c BLOQUEADA: duas abordagens tentadas e insuficientes no critério de pendência — (1) configure no motion e
  (2) timer de despacho do shell de 20→1 ms (revertido: p95 piorou 127→158 ms). Terceira abordagem exige decisão
  do humano (`AGENTS.md §3.12`, regra das duas tentativas). 02e é família nova e autorizada; 02c segue FALHA.
## 5. Próximas tarefas (abrir cartão antes de executar)
0. **F1d3 (PRÓXIMO PASSO, NÃO EXECUTADO):** humano roda `~/u` no tty3 (já com as sondas `FOOT_COMMIT`/
   `UDEV_FRAME_CB` compiladas) e envia o log; `python3 tests/integration/udev_client_compose_probe.py <log>`
   classifica. Se `CALLBACK_NAO_CHEGA`, candidato mínimo é chamar `send_frames_surface_tree` no loop udev
   (API real Smithay 0.7.0 `utils.rs:217`); se `SEM_DAMAGE_FORA_DA_CENA`, investigar composição/`FrameFlags`
   (candidato `FrameFlags::empty()`) e formato/alpha do GBM — trocar `color_formats` exige decisão. Só então
   abrir cartão de correção com teste ANTES visto falhando, e novo juiz visual: painel/`foot`/arrasto/VT.
   Cursor é cartão separado; Plano B exige aprovação explícita. Não reabrir 02q sem pedido.
1. `02o` — SUBSTITUÍDA: a 02p criou sonda full-res que reproduziu o flicker (2×1.910 pixels) e depois
   comprovou canvas fixo sem transiente. A grade antiga não é mais critério; não reabrir sem novo pedido.
2. `02n` — JUIZ NEGOU ("problema ainda ocorre"); instância já não existe. NÃO abrir 5ª correção sem decisão.
3. `02j` — CONCLUÍDA; os 3 modos nomeados (`FRAME_NEVER_ACKED`, `FRAME_NEVER_DREW_AT_SIZE`,
   `FRAME_STRETCH_ACTIVE`) estão no log bench. Próximo passo natural (NÃO executado): correlacionar esses modos
   com um novo arrasto REAL e só então decidir a causa da queixa visual de 02e/02f.
4. `02f` — NÃO CONCLUÍDA: juiz visual negou ("moldura ainda chega atrasada"). NÃO reabrir sem novo pedido.
5. `02g` — shell: fonte de fd GLib no `wl_display` mantendo timer de segurança de 50 ms; limite 1 h, reverter se piorar.
6. `02h` — medições de ambiente VM (sem mudança de código): baseline / Skia CPU / GSK cairo / b+c, 3 repetições.
7. `02i` — correções de documentação (somente docs): presentation-time, background_effect, WPE, cartões 02c/02d, F1.
8. `05-compatibilidade-protocolos.md` — RASCUNHO; 7 mensagens do `foot` registradas em §6. NÃO executar sem ok.
- `02d` NÃO executar (autorização explícita). Decisão sobre modo produto é do humano.
## 6. Dívidas e bugs conhecidos (NÃO corrigir fora de uma tarefa; só registrar)
- F1d3 (ERRO DO HARNESS, corrigido 2026-11-22): `/home/daniel/aeroshell-udev-20261122-051134.log`
  (4,4 MB / 24.518 linhas) não tem `FOOT_COMMIT` nem `UDEV_FRAME_CB` porque `scripts/run-udev-diag.sh`
  (usado por `~/u`) compilava `cargo build --workspace` **sem `--features bench`**. Corrigido: agora
  `cargo build --workspace --features bench`; `strings` do binário com bench = 1/1 para as duas sondas.
  Nenhuma amostra descartada — o log é prova de que o harness estava cego, não de que o cliente não commitou.
- F1d3 (MEDIÇÃO do log 051134, 2026-11-22): sessão/DRM/output `active=true`; `FRAME_BUFFER_COMMIT
  buffer=Some((1932,991)) ok=true` (a decoração chegou); `ordem de render` com `client@…` + `frame@…`;
  16 `UDEV_FRAME is_empty=false queued=ok` e `UDEV_VBLANK n=3..10 submitted=ok` entre 08:11:36.785 e
  08:11:37.107 — **a cena com o cliente foi enfileirada e confirmada pelo vblank**. Depois disso
  `is_empty=true` (idle normal). `PauseSession` às 08:12:01 veio de Ctrl+Alt+F1 (o humano usou F1 em vez
  de F2/F3 do RUNBOOK) e os 23.762 `DeviceInactive` seguintes são esperados. **Não há pixels do cliente
  medidos no udev**: ainda não dá para separar "import do buffer falha no renderer GBM" de
  "composição/direct-scanout/opacidade descarta o cliente".
- F1d3 (FATO DO HUMANO, 2026-11-22 — prevalece sobre interpretação anterior): `is_empty=true` significa
  "sem damage, nada a submeter", isto é, **idle normal**. Não alterar a lógica de `is_empty`. Input, troca de
  VT e vblank estão provados bons no 4º log; não mexer neles. Cursor é cartão separado.
- F1d3 (AMOSTRA, arquivo original sumiu 4×): `/home/daniel/aeroshell-udev-20261122-045035.log` não existe
  mais; recorte literal das linhas `UDEV_*` preservado em `/tmp/opencode/f1d3-4exec/`. O atalho VT FUNCIONOU
  (`UDEV_VT key="F1" vt=1 result="ok"`), mas o humano usou F1 (vt=1) em vez de F2/F3 do RUNBOOK e a sessão
  foi pausada (`UDEV_SESSION_EVENT event=PauseSession active=false`), sem `ActivateSession` até o fim — o
  `DeviceInactive` em loop depois disso é **esperado**, não causa da imagem ausente.
- F1d3 (HARNESS, corrigir sem mexer no produto): `udev_frame_probe.py:26` exige `UDEV_VT key=F(\d+)` sem
  aspas, mas o tracing imprime `key="F1"` → reporta `vt=0`; e o mesmo analisador marca `VEREDITO: FALHA` só
  pelos `render_frame falhou` pós-`PauseSession`. Ambos são do analisador; a evidência de apresentação da
  fase ativa (`session active=true`, `output active=true`, `frame queued=ok`, `vblank submitted=ok`) está boa.
- F1d3-diagnóstico (AMOSTRA, arquivo original sumiu 2×): tanto `aeroshell-udev-20261122-023701.log` quanto
  `aeroshell-udev-20261122-032739.log` não existem mais em `/home/daniel` após o reboot; usei recortes literais
  do texto colado pelo humano (`/tmp/opencode/f1d3-rediag/judge-log-excerpt.txt`,
  `judge-log-032739-excerpt.txt`). **Pedir ao humano que copie o arquivo para `/tmp` ou envie o conteúdo
  antes de reiniciar**, para não depender de transcrição.
- F1d3-diagnóstico (ERRO MEU, corrigido): os keycodes do atalho de VT foram escritos como evdev crus;
  o Smithay soma 8 (XKB) no libinput. Sintoma: `UDEV_VT` nunca apareceu no log. Correção em `udev.rs`,
  com `UDEV_KEY` para auditar os códigos reais no próximo log.
- F1d3-diagnóstico (AMOSTRA, arquivo original sumiu 3×): `/home/daniel/aeroshell-udev-20261122-040108.log`
  não existe mais; o 3º veredito negativo vem do texto colado pelo humano. Nenhuma amostra foi inventada.
- F1d3-diagnóstico (ERROS DO HARNESS, corrigidos e preservados): a 1ª execução do pixel harness morreu por
  colisão do `application_id` do `frame_webview` com a sessão udev viva (`headless-client-baseline.log`);
  a 2ª amostrava a região errada porque não aplicava `Flipped180`; a 3ª usava regex sem `t=` e não separava
  baseline/depois. Logs crus em `/tmp/opencode/f1d3-pixels/headless-client-{baseline,run1,run2,run3}.log`.
  Correção: D-Bus privado, `buffer_y=955-80-494=381`, classificação por `toplevel mapeado`. A regressão
  antiga `headless_render_damage.py` também falhou 1× pela mesma colisão e passou em D-Bus privado.
- F1d3-diagnóstico (AMOSTRA, arquivo original sumiu): o arquivo `/home/daniel/aeroshell-udev-20261122-023701.log`
  não existe mais em `/home/daniel` após o reboot do humano; o recorte literal colado na mensagem foi preservado
  em `/tmp/opencode/f1d3-rediag/judge-log-excerpt.txt` (NÃO é o arquivo original — as linhas `\00` finais
  foram transcritas, não decodificadas). Nenhuma amostra foi descartada; o baseline do analisador usa o recorte.
- F1d3-diagnóstico (FORMATO DOS LOGS, verificado): `tracing_subscriber` imprime mensagem e depois os campos
  na ordem da macro (confirmado em `/home/daniel/aeroshell-udev-20261122-023701.log`:
  `iniciando AeroShell backend="udev" seat=...`). As macros `UDEV_*` seguem essa ordem; se o próximo log
  mostrar outra ordem, o analisador deve ser ajustado (não o produto).
- F1d3 (CURSOR, confirmado no código): `init_udev` não adiciona elemento de cursor — o ponteiro **anda e clica**
  (movimento relativo implementado em `input.rs`), mas é **invisível** no DRM. Caminho: `cursor-icon` 1.2 +
  `MemoryRenderBufferRenderElement` (`element/memory.rs:463`). Exige cartão próprio; o juiz visual foi avisado.
- F1d3 (HOTPLUG): eventos `UdevEvent::Added/Changed/Removed` só são **logados** ("reinicie para usar"); não há
  `DrmScanner` nem adição/remoção dinâmica de outputs. `smithay_drm_extras` seria dependência nova (não aprovada).
- F1d3 (MULTI-GPU/dmabuf/presentation-time): implementado apenas 1 GPU, 1 conector por CRTC, `queue_frame(())`
  sem `OutputPresentationFeedback`. O anvil usa `DrmOutputManager`/`GpuManager`/dmabuf feedback — fora do cartão.
- F1d3 (INPUT relativo): `PointerMotion` passou a mover o ponteiro, mas **não há teste automatizado próprio** —
  nenhum backend automatizado emite evento relativo (headless usa `AERO_TEST_INPUT_SOCK`; winit usa absoluto).
  Cobertura fica com o juiz visual no tty3.
- F1d1 (CURSOR no backend udev): o Smithay 0.7.0 fixado **não tem** `PointerRenderElement` nem
  `SoftwareCursor` (`grep -rn "PointerRenderElement|SoftwareCursor" ~/.cargo/registry/src/*/smithay-0.7.0/src`
  → 0 ocorrências). O caminho é `cursor-icon` 1.2 + `MemoryRenderBufferRenderElement`
  (`element/memory.rs:463`, presente no 0.7.0). F1d3 precisará disso. Não corrigir fora de cartão.
- F1d1 (GBM modifiers, NÃO medido): `backend_gbm_has_create_with_modifiers2` e
  `backend_gbm_has_fd_for_plane` ficam **desligadas**, embora o gbm da VM seja 26.0.8
  (`pkg-config --modversion gbm`). Ligá-las sem hardware real seria chute: teste futuro em GPU real.
- F1d1 (DOCTOR, pré-existente, fora do critério): a linha de sugestão do `doctor.sh:90`
  (`sudo apt-get install -y ${missing[*]} …`) imprime os itens com o sufixo `(pkg-config: X)`, que não são
  nomes de pacote apt válidos. Já existia para gtk4/webkit; F1d1 só aumenta os itens afetados.
- F1d1 (Cargo.lock): 24 pacotes novos, todos por feature **aprovada** do Smithay 0.7.0. Os alvos
  `windows-*`/`hermit-abi`/`io-lifetimes` entram no lock por deps condicionadas a Windows; não afetam o
  build Linux (verificado em `cargo tree -p aeroshell-compositor`).
- F1d2 (MODIFIERS, medição limitada): `format_modifier_plane_count` retornou `None` para os 4 modifiers
  consultados (`Invalid`, `Linear`, `I915_x_tiled`, `I915_y_tiled`) em todos os formats. O símbolo existe
  no `libgbm` da VM (`nm -D /usr/lib/x86_64-linux-gnu/libgbm.so.1 | grep format_modifier_plane_count`), e a
  alocação real escolheu `Linear` com sucesso — portanto `modifiers: []` é **lacuna da API usada, não prova
  de ausência de modifiers**. Enumeração completa fica para F1d3 via EGL.
- F1d2 (DT_NEEDED, amostra intermediária preservada): a 1ª versão da sonda mostrava `libdrm.so.2` direto em
  `readelf -d`; a versão final (com enumeração DRM via `smithay::reexports::drm`) mostra só `libgbm.so.1` direto,
  com `libdrm.so.2` carregado como dependência transitiva (`ldd`). Ambas carregam em runtime; nada descartado.
  `libinput`/`libseat`/`libudev` **ainda não** aparecem — nenhum código os chama até F1d3.
- F1d2 (GATE, amostra preservada): a 1ª execução da bateria falhou **só** em `cargo fmt --all -- --check`
  (2 divergências em `backend/udev.rs`); `cargo fmt` corrigiu e a 2ª bateria ficou toda verde. Nenhuma checagem
  foi desligada; `/tmp/opencode/f1d2/validate.log` e `validate2.log` mantidos.
- Docs: `docs/STATE.md` medido em **989 linhas** no fim de F1d2 (962 no fim de F1d1; 903 antes de F1d1), contra
  o teto de 150 anunciado no próprio cabeçalho (linha 4). Cartão de documentação próprio, não resolvido.
- F1c (FALLBACK N>1, descoberto ao corrigir a moldura): o mesmo padrão de fallback "frame sem cliente"
  existe em `winit.rs:699` (N=1), `headless.rs:223` e `headless.rs:314`. Só o caminho N>1 foi alterado,
  porque é o único onde outro output pode cair no fallback; nos demais não há sintoma relatado. Não
  reabrir sem novo pedido.
- F1c (HARNESS, amostra preservada): `oversize-after.log` teve 1 linha malformada por interleaving de
  `libEGL warning` (`OUTPUT_FRAME output=aero-1 seq=libEGL warning...`); a repetição `oversize-after2.log`
  teve `malformadas=0`. Nenhuma amostra apagada; o warning intercalado não é evidência de dano.
- F1c (VISUAL/HARNESS, amostras preservadas): a 1ª execução com a instância antiga aberta colidiu o
  `application_id` e não mediu orientação; D-Bus privado isolou as provas. O avaliador teve 1 TypeError
  (strings de `SLICE_GEOM`) e 2 corridas de estado estável parcial (`KeyError`), corrigidos sem alterar produto;
  `orientation-after-normal-request-size2.log` é uma dessas falhas, não uma amostra válida. O 1º launcher da
  nova instância usou `$$` em vez de `$!` e terminou o D-Bus (`wait ... não é um processo filho`), sem processos
  sobreviventes; o launcher foi corrigido, sem mexer no compositor.
- F1c (HOST, limitação explícita): o damage incremental com `age=1` vale para as texturas por output e para
  o renderbuffer headless. A composição winit do host usa `age=0`/full damage em todo vsync, para manter a
  cadência; reduzir o damage do host NÃO foi implementado nem medido.
- F1c (1ª POLÍTICA, amostra preservada): pular compose+submit quando tudo estava estável causou busy-loop
  (`after-multi-1.log`: 99.312 `OUTPUT_FRAME`, seq=49.656; 2º frame real ~31 s após o map). Full host a cada
  vsync removeu esse gargalo funcional; não é conclusão de desempenho em VM sem 3D.
- F1c (HARNESS, amostras preservadas): `after-multi-content-final2.log` aceitou uma linha intercalada
  (`target=408FRAMEWEB...window_id=1`) com regex frouxo; `final4.log` esperou dano novo DEPOIS do buffer real,
  mas a cena estável corretamente emitiu só `none` até seq=1713. O teste agora exige `RESIZE_GLUE` completo +
  `SLICE_GEOM` válido e mede o intervalo desde antes do spawn; final 5/6 OK. Nenhuma amostra foi apagada.
- F1b (DAMAGE, limitação medida — não é bug): o alvo é um `GlesRenderbuffer` offscreen sem buffer-age e o
  render usa `age=0`, então TODOS os frames saem com `damage=1280x800@0,0`. Redução de damage (age>0) é
  critério de F1c, não foi tentada nem medida aqui.
- F1b (HARNESS, intermitência preservada): `frame_configure_hygiene.py` falhou na regressão nested
  (`frame WebView não commitou buffer inicial`) e passou na repetição diagnóstica única
  (`/tmp/opencode/f1b/nested_configure_hygiene_retry1.log`). Mesma família da intermitência de 1º buffer já
  registrada na F1a; nenhuma amostra descartada.
- F1a (HARNESS, intermitências preservadas — NÃO corrigir nesta tarefa): `frame_follows_resize.py` falhou no
  BASELINE (`resize detectado=None`, A não mudou) e passou depois; `frame_fixed_canvas_hygiene.py` falhou
  depois (`timeout esperando buffer inicial`) e passou na repetição diagnóstica única. No glue pós-extração
  houve 1 linha `SLICE_COMPOSE` malformada por interleaving de `eprintln!` (0 no baseline). Logs crus em
  `/tmp/opencode/f1a-v2/`. Nenhuma amostra descartada; não chamar de "todos os harnesses verdes".
- 02q (POOL, 2 tentativas preservadas): a 1ª marcou ready por double-rAF sem realizar/apresentar as views;
  `02q-cold-comp-36184.log` só contém `pool_vazio`, nenhum ready, e o teste deu timeout (`02q_after_pool1.log`).
  A 2ª esperou `LoadEvent::Finished` + `get_snapshot`; `02q-cold-comp-36843.log` mostra
  `pool_snapshot_falhou pool_id=0/1: Ocorreu um erro ao criar a captura instantânea` e o mesmo timeout
  (`02q_after_pool2.log`). Ambas foram revertidas; o humano então autorizou explicitamente a via A
  (HTML carregado + placeholder), implementada no estado final.
- 02q (HARNESS, amostra NÃO descartada): `02q_cold_baseline1.log` contou wrappers `bwrap` como WebProcesses
  (9 linhas). Corrigi somente o filtro para o argv0 `WebKitWebProcess`; a repetição crua `02q_cold_baseline2.log`
  mediu os 3 processos reais. O placeholder também teve 1 erro de compilação por tipo de `dst` ausente;
  corrigido com `Size<i32, Physical>`, sem desligar checagem (`02q_after_placeholder1.log`).
- 02q (VIA A, tentativas preservadas): antecipar `realize` piorou para 480,071/518,650 ms e foi revertido
  (`02q_pool_prerealize2.log`); `queue_draw` após present não impediu timeout e foi removido
  (`02q_queue_draw_initial1.log`). Nenhuma delas permanece no código final.
- 02q (INTERMITÊNCIA, causa NÃO identificada — não chamar de flake): `frame_initial_buffer.py` falhou 2× por
  20 s com 2.221/2.235 placeholders e 0 `RESIZE_GLUE` (`/tmp/ibuf-comp-42805.log`, `43155.log`). Sem qualquer
  mudança adicional de produto, depois passou (`02q_final2_frame_initial_buffer.log`). Remover/adicionar
  `AERO_TEST_INPUT_SOCK` em memória não provou causalidade; allocation 0×0 no present também ocorreu num caso
  bem-sucedido. Amostras preservadas; nenhuma correção foi declarada para essa intermitência.
- 02p (MEDIÇÃO, amostras preservadas): a 1ª sonda usou retângulo sem transform e foi INVALIDADA por
  `sanity_titlebar=0` (`02p_pixel_baseline1.log`); não foi usada como evidência. A 2ª falha foi compilação
  da sonda. O baseline válido só foi aceito com `sanity_titlebar=205` e os transientes crus.
- 02p (REGRESSÕES DO JUIZ, corrigidas e testadas): o primeiro configure antes de `set_insets` deixava
  canvas compositor=1280x800 vs shell=1292x836 e quebrava caps direito/inferior; reconciliação agora só
  altera o canvas inicial antigo. Uma segunda ativação do `GApplication` reassociava id 0 e derrubava o
  shell (`aero-02p-visual.log:9413-9414`); `ACTIVATED` agora torna o handler idempotente. Evidências em §3.
- 02p (INFRA): `frame_resize_edges` não executou uma vez porque outra instância manual estava viva e o
  application_id era compartilhado; repetido após o humano fechar → OK. Não foi contado como flake/produto.
- 02n (MUDANÇA DE TESTE VIZINHO, transparente): `frame_configure_hygiene.py` classificava `source=final` só até
  +100 ms do release — janela do comportamento 02m. A 02n adia o final para o idle (medido 205-209 ms), então o
  teste reportou `final configs=0` com 1 final real no log cru. Ajustei SÓ a janela de classificação para
  `FINAL_DELAY_MAX_MS = 400`, mantendo a exigência de exatamente 1 final com o tamanho real; não foi verificação
  desligada nem medição descartada. Reexecutado OK (`finals=1 ultimo_cliente=(834,494)`).
- 02n (RESSALVA DA SPEC, a confirmar no juiz): "os dois estados são visualmente idênticos em tema uniforme" é
  HIPÓTESE do humano. Com gradiente/sombra/blur, o 9-slice do buffer 708 e o nativo 768 podem diferir. O teste
  automatizado mede só geometria/sequência de estados; a sentença final é o olho do humano.
- 02l (DESCOBERTA DURANTE A TAREFA, necessária ao critério visual): a 02k deixava as fatias direita/inferior
  nas coordenadas do buffer congelado (1292×836) enquanto o alvo era 708×530 — `SLICE_GEOM` mostrava
  canto em x=1340/y=880. Corrigido na 02l com deslocamento por borda (`delta_rx`, `delta_by`, `delta_rb`);
  as 9 fatias passaram a cobrir x 54→762 e y 50→580 (`02l_after5.log`). Reportado no relatório como mudança
  de autoria, não estava no rascunho original.
- 02l (AMOSTRA TRANSITÓRIA, NÃO descartada): `02l_glue_postgate2.log` falhou com `snap=0.4602 em sx`. A causa
  não é a 02l: o buffer ficou em 708×530 (tamanho antigo) por 6 frames antes de virar 1292×836
  (`/tmp/glue-comp-99721.log:243,257`), trocando `slice_scale_x` de 1,0086 para 0,5484. As zonas protegidas
  permaneceram constantes (`title_left_w=120`, `title_right_w=96`) nas duas linhas. É o congelamento do buffer
  da 02k com corrida de escalonamento; 3 repetições seguidas depois (`02l_glue_repeat{1,2,3}.log`) OK.
  Registrado como dívida pré-existente, sem alterar o gate para esconder.
- 02l (AUTORIZAÇÃO): humano aprovou explicitamente a edição de `protocols/aero_frame_v1.xml` em 2026-11-20
  ("Protocolo novo (Recomendado)"). Nenhum ADR editado; nenhuma dependência nova.
- 02k (RELATO VISUAL do humano, 2026-11-20 — DADO, prevalece sobre a medição automática): "notei no seu teste
  de resize que praticamente resolveu, com um único detalhe ainda, o conteúdo da barra de título achata no
  sentido horizontal enquanto está fazendo o resize". Onde: faixa top do 9-slice em
  `backend/winit.rs` (`(slice_sx, 1.0)` na spec da titlebar), exatamente o que o cartão 02k:37 prescreve.
  Hipótese: o HTML da barra (texto à esquerda, botões à direita — `frame_webview.rs`, `RIGHT_CONTROLS=96`) é
  amostrado da fatia central do topo e esticado em X. Preservar o canto direito exigiria linha de corte extra
  (largura dos controles), hoje conhecida só pelo shell; passá-la ao compositor mexeria em protocolo/ADR.
  **NÃO corrigi**: mudar a receita do cartão aprovado exige decisão do humano (AGENTS.md §0.8 / §5).
- 02k (LACUNA DE MEDIÇÃO): `frame_resize_glue.py` lê a geometria/escala **calculadas pelo próprio compositor**;
  não amostra pixels. Por isso `edge_l/edge_r/title_h == insets` e `SLICE_COMPOSE n=9` provam que o caminho do
  9-slice executou e que os insets são constantes, mas **não capturam o achatamento do conteúdo da barra**.
  Relato do humano prevalece (AGENTS.md §0.4). Falta medição objetiva de fidelidade da titlebar (largura do
  texto/botões por frame), a criar em cartão próprio.
- 02k (contagens dos vizinhos, sem descartar amostra): `frame_configure_hygiene.py` OK (1 configure
  `source=grab-start`, nenhum `source=grab` no motion, ratio 0,071); `frame_buttons.py` OK;
  `frame_resize_edges.py` FALHOU 2/4 na 02k, mas também 1/4 na 02j e 2× OK isolado depois (`..._retry{1,2}.log`)
  — pré-existente/intermitente, não regressão confirmada; `frame_follows_resize.py` e `frame_window_id.py`
  tiveram 1 timeout de IPC / 1 configure ausente na rodada final e 2/2 OK isolados depois;
  `frame_resize_sync.py` FALHA como esperado (métrica antiga, fora de escopo).
- 02j (DIVERGÊNCIA DO CARTÃO, registrada): o cartão pedia 14 passos/10 px a 50 ms; medi 0 `rescaled=true` com
  esse gesto (`/tmp/opencode/02j_diag_normal2.log`) e o `FRAME_STRETCH_ACTIVE` não aparecia. Troquei o harness
  para **40 passos/10 px a 10 ms** e o episódio de escala passou a existir (68 `rescaled=true` na 1ª rodada).
  Nenhuma amostra foi descartada; a mudança é só do gesto sintético, não da política do compositor. Reverter se o
  humano discordar.
- 02j (INJEÇÃO BENCH): `AERO_DIAG_SUPPRESS_ACK_ONCE=1` e `AERO_DIAG_SUPPRESS_DRAW_ONCE=1` existem só com
  `--features bench` e tornam determinísticos os 2 timeouts. A supressão de ack vale para TODOS os acks do
  primeiro serial rastreado (a 1ª versão era consumida por um probe sintético sem entrada rastreada).
- 02j (corrida de foco no press dos harnesses, pré-existente): sintoma — 1 em 3 execuções do
  `frame_configure_hygiene.py` e 4/8 probes do `frame_resize_edges.py` recebiam o `button press` no compositor
  mas sem `input_seat:...:set_focus`, logo nenhum `FRAMEWEB: button` e nenhum grab. Onde — `input.rs:79-117`
  (`focus_on_press`) e `state.rs:159-164` (`window_for_id`); amostras cruas
  `/tmp/hygiene-comp-81009.log` e `/tmp/edges-comp-80105.log`. Hipótese: `surface_under`/`window_for_id`
  retornam vazio num intervalo de escalonamento. **Não corrigi o produto** (fora da tarefa): os harnesses agora
  repetem o press e imprimem cada tentativa; nenhuma amostra descartada.
- 02j (classificação de `source=final` no hygiene): o configure final é enviado no release, mas o relógio do
  harness pode registrá-lo antes de `t_release`. Agora é classificado pelo rótulo semântico `source=final`, com
  tolerância de 20 ms antes do release; amostra crua `/tmp/opencode/02j_hygiene_final1.log` (ratio 0,357).
- 02j (RELATOS VISUAIS do humano, 2026-11-20 — TODOS são dados, o último prevalece):
  (1) em tempo real durante os harnesses: "o redimensionamento aparentemente resolveu visualmente, está a
  decoração seguindo muito bem o resize"; (2) "notei um problema no acompanhamento da barra de título. Ela
  ficava piscando entre uma posição antiga mais pra trás e a atual mais pra frente."; (3) refinamento: "só na
  barra de título, a moldura está ok"; (4) **TESTE MANUAL com aplicação aberta (compositor PID 84904, socket
  `wayland-aero-02j-visual`, log `/tmp/opencode/aero-02j-visual.log`): "no teste manual pude ver que o flicker é
  geral, incluindo as bordas"**. O item (4) contradiz (3): com o arrasto manual a janela aberta, o flicker
  ATINGE também as bordas. Registro a contradição, não a resolvo — a medição automática (geometria/escala por
  frame) NÃO capturou o que a pessoa viu (AGENTS.md §0.4). Hipótese (NÃO investigada): o `RescaleRenderElement`
  da 02e desenha o frame na geometria antiga por um ou mais frames antes do configure seguinte, e o
  `FRAME_STRETCH_ACTIVE` medido no arrasto real (4 episódios, 482/5439/846/937 ms, `max_scale` até 2,165;
  1 `FRAME_NEVER_DREW_AT_SIZE` com `buffer=964x530` vs alvo `708x530`) é compatível com o flicker geral.
  **NÃO reabrir 02e/02f nem mudar escala/debounce por conta própria** — exige cartão novo e decisão do humano.
- 04b: `tests/integration/s3_input.sh` falha **também no baseline** (`0fc05e7`, worktree isolado): a input region
  é aplicada (`rects=[(24,8,189,36)]`) mas o clique "dentro" dá `under=None`. Não é regressão do painel React;
  S3 não está no gate. 04b não mexeu no teste (tentei `AERO_PANEL_LEGACY=1`, sem efeito, e revertei).
- **REQUISITO DO PROJETO (decisão do humano, 2026-11-20): compatibilidade com compositores modernos.** O humano
  relatou na tela estas mensagens do `foot` e exigiu que todas sejam implementadas no AeroShell (ou registradas
  como requisito se não for o momento): `compositor does not implement the primary selection interface`;
  `compositor does not implement xdg activation`; `bell.urgent will fall back to coloring the window margins red`;
  `compositor does not implement fractional scaling`; `compositor does not implement server-side cursors, falling
  back to client-side cursors`; `compositor does not implement the xdg toplevel icon protocol`;
  `text interface not implemented compositor, IME will be disabled`. Nomes de protocolo prováveis (CONFIRMAR na
  versão fixada de Smithay/foot antes de implementar): wp-primary-selection-v1, xdg-activation-v1,
  wp-fractional-scale-v1, wp-cursor-shape-v1, xdg-toplevel-icon-v1, zwp-text_input_v3/IME, além de urgência/atenção
  de toplevel. Não implementados nesta fatia (exige ADR + cartões novos); rascunho em `docs/tasks/05-*.md`.
- 03: o socket IPC é síncrico (request/response); não há eventos de map/unmap. Os patches do SDK são derivados da
  resposta `list` do compositor no `createAero.refresh()`. Streaming de eventos + codec MessagePack é o S4 do plano
  mestre (exige ADR e dependências novas) — NÃO foi feito aqui.
- 03: `send_close()` é assíncrono. A 1ª versão de `sdk_cli.py` checou o `--list` imediatamente após o `OK` e falhou
  (`2 janelas` ainda). Amostra crua em `/tmp/opencode/sdk_cli_run.log`, não descartada; o teste agora espera o unmap
  por até 10 s. Não é bug do CLI.
- Atraso da decoração no resize (frame repinta depois do conteúdo; VM sem 3D piora). Causas baratas aplicadas
  (frame usa geometria commitada; fundo transparente). Timer de 20 ms no frame_webview.rs ainda existe (loop de despacho).
  Alternativas se persistir: nove fatias no compositor ou buffer sobredimensionado + src-rect (ADR 0001, Plano B).
- PRIORIDADE ALTA (relato do humano, 2026-11-20, logo após fechar 02b): "o conteúdo acompanha o mouse muito bem,
  porém a decoração fica atrasada correndo atrás". É o mesmo item acima, agora com sentença do juiz humano —
  NÃO é só dívida teórica. Cartão rascunho: `docs/tasks/02c-frame-resize-sync.md` (aguarda confirmação para executar).
  Leitura preliminar (HIPÓTESE, não medida): o frame só é reconfigurado no commit do cliente
  (`protocols/compositor.rs:130-149`) e o shell despacha eventos Wayland num timer de 20 ms
  (`frame_webview.rs:458`), somando um ciclo entre conteúdo e decoração.
- Maximizar não move o cliente para a origem do output; restaurar manda `size = None`.
- `button: 0x110` fixo no grab de resize (só botão esquerdo).
- `--command` via `sh -c` pode orfanar netos (usar exec/lançamento direto).
- `pkill -x aeroshell-compositor` NÃO casa (comm é truncado a 15 chars: `aeroshell-compo`); harnesses que dependem disso
  não matam instâncias antigas. Usar `pkill -f` com padrão específico ou rastrear PIDs.
- Presentation-time não existe; socket órfão; lock/greeter/PAM/idle/XWayland: não iniciados.
- Probe `wayland-client` no shell deve ficar atrás da feature `probe-registry` (optional).
- `AGENTS.md` tem dois protocolos sobrepostos: novo `§0.1–0.9` (linhas 7–106) e antigo "Início/Foco/Fim" (linhas ~108–131).
  Conflito: linha 16 manda corrigir o gate se falhar; antiga linha ~94 manda parar. Há também um título órfão no fim.
  As referências a `progress.md` como fonte única JÁ foram corrigidas (tarefa 05); estes itens seguem pendentes.
- Flake do S1: falhou 1× no passado (exit 1, ~16 s sob carga; isolado 7,67 s). Na tarefa 00 NÃO reproduziu:
  3 gates seguidos verdes, nenhuma amostra descartada (tempos em §3). Diagnóstico melhorado (stderr + PID),
  causa ainda DESCONHECIDA — não declarar "corrigido". Reexecutar até passar continua proibido (§0.5).
- Erro meu corrigido nesta sessão: eu havia rebaixado `docs/plano-mestre.md` a histórico. O usuário corrigiu: é o
  plano ATIVO e um dos documentos mais importantes (visão, fases, onde queremos chegar). Banner removido, texto
  original restaurado; só os ponteiros de atualização de `progress.md` viraram `STATE.md`. Não repetir.
- 02b (relato do humano, 2026-11-19/20, 2 foot): 2 janelas deslocadas, só UMA decorada; arrastar/resize move a outra;
  ✕ e maximizar não faziam nada; clicar na janela focada focalava a outra. DUAS causas, ambas corrigidas nesta sessão:
  (1) o map não anunciava `frame_requested` para a 2ª janela; (2) `nth(window_id)` usava ordem z. Confirmado por log.
- 02b (relato do humano, 2026-11-20, após "ok" parcial): clicar na BARRA/DECORAÇÃO da janela de trás não a elevava
  (só com arrasto). Confirmado e CORRIGIDO: `element_under` ignora layer surfaces; fallback por `surface_under` +
  `associated_frames` em `input.rs`.
- 02b (relato do humano, 2026-11-20, 3º detalhe): barra da janela da FRENTE sobre o CONTEÚDO da de trás — o clique
  "passava direto" e elevava a de trás. Confirmado e CORRIGIDO (duas camadas: ordem em `surface_under` e fonte em
  `focus_on_press`). Amostra falha crua em `/tmp/opencode/overlap-before2.log`, não descartada.
- Amostra falha de infraestrutura do teste (registrada, não descartada): `frame_follows_resize.py` falhou 1× com
  `ConnectionRefusedError` no socket IPC antes da inicialização; `ipc()` agora espera até 15 s pelo socket.
  Também houve 1 execução com falha apenas na asserção de foco (B já estava focada, então não emite `focus=true`);
  a asserção foi corrigida para "foco não salta para A", que é o critério real.
- 02c (MEDIÇÃO, não é bug do produto): o `foot` arredonda o tamanho pedido para múltiplos de 6 (`want=706` →
  conteúdo real `702`). A primeira versão do teste comparava o frame contra o `want` do motion; a métrica foi
  corrigida em `frame_resize_sync.py` para usar o tamanho REAL commitado pelo cliente. Amostra falha crua em
  `/tmp/opencode/sync-before.log` e `/tmp/opencode/sync-before2.log`, não descartadas.
- 02c (LIMITAÇÃO ARQUITETURAL): com a decoração em WebView, o commit do frame leva ~60–100 ms após o configure;
  motions de 50 ms tornam "zero pendentes no próximo motion" inatingível sem tirar o WebKit do caminho crítico
  (Plano B, ADR 0001). Duas abordagens falharam; NÃO tentar uma terceira sem aprovação (`AGENTS.md §3.12`).
- 02c (timer 1 ms sem ganho): `frame_webview.rs` foi temporariamente trocado de 20 ms para 1 ms; p95 piorou
  (127,19 → 158,05 ms) e a pendência ficou em 13/14. Revertido para 20 ms. Amostra em `/tmp/opencode/sync-after3.log`.
- `frame_resize_edges.py` é intermitente: 1 de 3 execuções nesta sessão falhou no `BottomRight` (`hit_no_frame=False`).
  Causa identificada no log cru `/tmp/edges-comp-25191.log`: o probe `Right` aplicou `content_w=1002`, mas o teste
  calcula `EDGE_POINTS` UMA vez com `696`; o ponto (759,577) passa a cair dentro do conteúdo. Não é regressão de
  `focus_on_press`. Amostra falha registrada, não descartada; corrigir o teste (recalcular pontos) é tarefa futura.
- 02b (dívida menor): `window_id` ainda é `space.elements().count()` no map; se uma janela fecha e outra abre, o id
  pode ser reatribuído. O mapa `window_surfaces` é limpo no hook de destruição, mas ids monotonicamente estáveis
  (não reutilizados) ficam para uma tarefa futura se o cenário aparecer.
- Gate 04b: aviso não bloqueante em `cargo test` — `unused dependency gdk4-wayland` em `crates/aeroshell-shell/Cargo.toml:26`.
  Não é erro (clippy e gate passam); remover dependência exige conferir uso em builds com feature, fora desta tarefa.
- 02e (descoberta lateral, pré-existente da tarefa 03): `window_ipc.rs` passou a devolver
  `W id x y w h <titulo-json>` ordenado por `window_surfaces` (`window_ipc.rs:80-100`), não por ordem z.
  `frame_window_id.py` e `frame_follows_resize.py` tentavam converter o título em `int` e falhavam; corrigi só o
  parser para `[:6]`. `frame_follows_resize.py` também media z-order por `list` — trocado pelo log
  `ordem de render (frente->tras)` de `winit.rs:272-276`. Nenhuma asserção foi enfraquecida; 4 vizinhos OK.
- 02e (amostra ruim de infraestrutura, registrada e não descartada): `/tmp/glue-comp-58443.log` teve 28
  `FRAME_BUFFER_COMMIT`, todos `buffer=None`; o harness agora espera o primeiro `FRAME_BUFFER_COMMIT buffer=Some`
  antes de arrastar. Linhas corrompidas por interleaving de stdout (`buffer=818798xx530530`,
  `rescaled=xtrue530`) são impressas como `malformadas=N` e nunca usadas como amostra válida.
- 02e (gate de partida): `bash scripts/check.sh` falhou no clippy com `unused variable: window_id` em `winit.rs:195`
  porque a variável só era lida sob `cfg(feature="bench")`. Corrigido renomeando para `_window_id`, sem `allow`
  nem desligar checagem; gate e clippy `--features bench` verdes depois.
- 02f (amostra ruim de infraestrutura, registrada e não descartada): 3 execuções do `frame_resize_glue.py` falharam
  com `FALHA: frame WebView não commitou buffer inicial` (`/tmp/opencode/02f_glue_after{1,2,3}.log`) porque a
  instrumentação nova pôs `t=` no início de `FRAME_BUFFER_COMMIT` e o regex antigo exige o prefixo
  `FRAME_BUFFER_COMMIT buffer=`. Corrigido movendo `t` para o FIM da linha (nenhum parser antigo muda); as 3
  execuções seguintes ficaram OK. Não foi flake nem problema do produto.
- 02f (erro de medição do teste novo, corrigido): a 1ª versão registrava `t_release` DEPOIS do `sleep(0,1)` do
  `send("button release")`, classificando o configure `source=final` como "durante o grab"
  (`/tmp/opencode/02f_step3_test.log`: `configures/motions=0,357`, `finals=0`). O instante passou a ser logo após
  o `sendall`; amostra crua preservada.
- 02f (gate de partida): `bash scripts/check.sh` falhou com `unused variable: source` em `aero_frame.rs:63` porque
  o valor só era lido sob `cfg(feature="bench")`. Renomeado para `_source` (mesmo padrão de `_window_id` na 02e),
  sem `#[allow]`; gate e clippy `--features bench` verdes depois.
- 02f (REGRADO PELO HUMANO nesta sessão): "nos testes considere que a decoração aparece 1 ou 2 segundos depois que
  a janela abre". O teste novo agora espera 2,0 s depois do primeiro `FRAME_BUFFER_COMMIT` antes de arrastar.
  Outros harnesses (`frame_resize_glue.py`, `frame_resize_sync.py`) ainda não têm essa espera explícita — não
  alterados aqui para preservar a evidência da 02e; adotar em tarefa futura se aparecer falsa falha.
- 02f (dados do arrasto REAL que o humano negou, `/tmp/opencode/aero-02f-visual.log`, 118.188 linhas): 8.152
  `RESIZE_SYNC_MOTION`, 8.152 `RESIZE_SYNC_CLIENT_CONFIGURE`, 202 `FRAME_CONFIGURE source=grab` (2,5% dos motions),
  1 `source=commit`, 3 `source=final`; delta mediano configure→motion = 0,19 ms. **O storm foi eliminado de fato** —
  e ainda assim o juiz viu a moldura atrasada. 99.557 `RESIZE_GLUE` válidas, invariante espacial 0 quebras;
  6.860 com `rescaled=true` (6,9% no total, ~98% DENTRO dos 41 segmentos de arrasto). Descompasso
  `|buffer-target|`: mediana 26 px, p75 81 px, p95 **434 px**, max **945 px**; fator de escala p95 **1,399**,
  max **6,839**. Ou seja: o que sobrou não é contagem de configures, é o buffer do WebView não alcançar o alvo e
  a escala 02e precisar distorcer a moldura durante o gesto.
- 02f (ERRO DE ANÁLISE MEU, corrigido antes de concluir): a 1ª leitura do log usou `m.group(5)` como `source`
  (é o campo `h`) e por isso relatei falsamente "0 configures `source=grab` dentro dos arrastos". Refeito com `group(6)`:
  132 configs na janela 0 e 70 na janela 1, dentro dos intervalos de motion. O dado falso NÃO foi usado como
  achado; registrado para não ressurgir.
- CONTEXTO DO HUMANO (2026-11-20, prior art `github.com/cprussin/domicile`): no domicile o Chromium forkado é o
  compositor final (viz); Rust/Smithay é host/producer, a decoração é DOM na MESMA cena e a cola vem de
  surface-sync com deadline — não é portável para o AeroShell (WebView nativa do sistema, WebKitGTK 6.0).
  Lição aproveitável: geometria de apresentação dona do compositor final + **diagnósticos nomeados** para
  "página que nunca faz ack", "página que nunca desenha no tamanho novo" e "por que nada foi enviado".
  **NÃO copiar o fork do Chromium nem CEF OSR** — não eliminaria o invariante da Tarefa 1 e contradiz a premissa
  de WebView nativa. Política confirmada pelo humano: conteúdo NÃO estica; só a textura do FRAME estica
  transitoriamente; move NÃO muda (já correto no Rust).
- CORREÇÃO DE AUTORIA (humano, 2026-11-20): **cprussin é contribuidor do Smithay**, não o criador; o criador é
  **Victor Berger (elinorbgr)**. Não repetir a atribuição errada.
- `docs/architecture/WINDOW-COMPOSITING.md` e `docs/architecture/ARCHITECTURE.md` são a referência de prior art
  citada pelo humano. Registrado aqui como contexto; **ADR 0006 (aditivo/proposto) NÃO foi editado** — incluir
  prior art em ADR exige aprovação explícita (AGENTS.md §0.8 / §6).
- 02o (desvio de infraestrutura, fora da tarefa): abrir um 2º nested enquanto a instância 02n está viva faz o
  `frame_webview` novo ativar a instância antiga e morrer (`application_id("dev.aeroshell.frame-webview")` em
  `frame_webview.rs:470`; log `/tmp/pxp-comp-120110.log`: "conexão privilegiada do shell caiu"). Contorno no harness
  02o: `DBUS_SESSION_BUS_ADDRESS` privado via `dbus-daemon --session --nofork`, sem encerrar a 02n.
- 02o (bug de detecção corrigido, não limiar): o cartão exigia `buf==target` no frame 9-slice, mas com o readback
  ativo o buffer chega ao target no mesmo frame em que o 9-slice cai. A comparação correta é entre o último frame
  `rescaled=true` e o primeiro `rescaled=false` com o MESMO `target` de apresentação — é o par 9-slice/nativo que
  o humano vê. Limiares de diff/flash não foram alterados; amostra crua preservada em `/tmp/opencode/02o_pixels_*`.
## 7. Não medido / não validado (não trate como fato)
- F1d3 (**PENDENTE NO 2026-11-22 — sondas instaladas, sem log do tty3**): `FOOT_COMMIT` e `UDEV_FRAME_CB`
  existem sob `bench` e o analisador `udev_client_compose_probe.py` distingue os cenários, mas **nenhum valor
  de `pending` do backend udev foi medido** (VM sem VT). O headless que funciona mediu 7 commits em 0,76 s
  com `pending_seq=0,1,1,0,1,1,0`; não é evidência do udev. Nenhuma causa foi escolhida e nenhuma correção
  foi proposta — o juiz visual do tty3 segue pendente.
- F1d3 (**3º JUIZ VISUAL NEGOU — 2026-11-22, dado do humano**): rodou `~/u` (script com `RUST_LOG=debug`)
  no tty3 e respondeu **"mesma coisa aconteceu"**. Agora há evidência objetiva no log:
  `UDEV_SESSION active=true`, `UDEV_DRM active=true`, `UDEV_OUTPUT active=true`,
  `UDEV_FRAME seq=1/2 is_empty=false queued="ok"` e `UDEV_VBLANK n=1/2/60 submitted="ok"`.
  Analisador corrigido (aspas): `VEREDITO: OK`. Ou seja, **o DRM aceitou frames e confirmou vblank**;
  o que falta saber é se os frames com o `foot`/frame mapeados foram enfileirados — o filtro antigo do
  `UDEV_FRAME` escondia exatamente esses frames (agora registra todo não-vazio). Também não houve
  `UDEV_VT`/`UDEV_SESSION_EVENT`: o atalho estava com keycode errado (§6), corrigido mas **não validado**.
  Causa da imagem ausente **AINDA NÃO identificada**; não escolher hipótese sem o próximo log.
- F1d3 (**EVIDÊNCIA NOVA, 2026-11-22 — medição do agente, não juiz visual**): no headless com o MESMO
  `render/scene.rs`, a região de conteúdo `(60,80,696,494)` passou de `0/528` células fora do clear para
  `483/528` após o `foot` mapear (`headless-client-final.log`), e a sonda `CLIENT_ELEMENT_PROBE` registrou
  `delta=1`, `from_surface=Some`, `err=none` em 68/68 linhas (`element-final-afterfmt.log`). Portanto a
  afirmação antiga "não provamos que o elemento do cliente foi criado" está **fechada**: foi criado e
  renderizado. O que segue **NÃO medido** é o framebuffer de scanout do udev (sem VT aqui) — a causa final
  dentro do `DrmCompositor`/GBM (direct-scanout/opaco vs. formato/alpha) continua em hipótese.
- F1d3 (**2º JUIZ VISUAL NEGOU — 2026-11-22, dado do humano**): rodou `~/run-aeroshell-udev.sh` no tty3.
  Relato cru: *"o mouse funciona, mas nao aparece cursor, consegui mover a janela as cegas e tambem digitei
  exit no terminal e dei enter, a janela fechou, mesmo sem ter renderizado o foot nela. Nao deu pra sair do
  tty de volta pressionando alt + ctrl + f2 nem f1 nem f4."* Log (recorte em §6): heartbeat `tick=250..1500`
  (~24 s), 0 `render_frame/queue_frame falhou`, `FRAME_BUFFER_COMMIT ... ok=true`, cena com `client` + `frame`,
  foco de teclado no `foot`, 3 grabs de move/resize, **0 linhas de evento de sessão**, fim com NULs binários.
  Interpretação mínima: **não houve travamento do loop de render**; cursor invisível é dívida conhecida;
  o atalho de VT falhava porque `grep -rn change_vt crates/` era 0 (agora corrigido, mas **não validado** no tty3).
  Causa da tela sem imagem **NÃO identificada**: hipóteses em aberto — (a) `queue_frame` sem vblank de volta,
  (b) `vmwgfx` não faz scanout do dmabuf sob `LIBGL_ALWAYS_SOFTWARE=1`, (c) sessão nunca ativada
  (`is_active()`/`device.is_active()` não medidos no 1º log). Nenhuma confirmada; instrumentação pronta (§3).
- F1d3 (**1º JUIZ VISUAL NEGOU — 2026-11-22, dado do humano**): rodou `~/run-aeroshell-udev.sh` no tty3.
  Relato cru: *"abriu uma janela decorada que deveria ser o foot, porém antes mesmo de exibir o conteúdo do
  foot travou tudo e tive que reiniciar. Não apareceu cursor do mouse na tela também nem aceitou comandos do
  teclado, provavelmente porque travou, já que nem chegou a abrir o conteúdo da janela com o foot dentro."*
  Interpretação mínima: a moldura (shell/frame) **chegou a renderizar** — DRM/GBM/EGL e o 9-slice funcionaram
  ao menos 1 frame — e o travamento ocorreu antes do cliente `foot` mapear. Cursor invisível é dívida conhecida
  (§6), não causa do travamento. **O agente NÃO reproduziu** (sem VT ativo nesta VM) e **NÃO vai corrigir no
  escuro** (AGENTS.md §0.3). Hipóteses em aberto, todas não confirmadas: (a) ciclo `queue_frame`/`frame_submitted`
  dessincronizado com o timer fixo de 16 ms sob renderer software (llvmpipe), (b) `frame_webview`/WebKitGTK travou
  ao renderizar no DRM, (c) modeset de `Virtual-1` (1920x955@60) contra o GPU virtual do VirtualBox, (d) o código
  não rejeita `EGLDevice::is_software()` como o anvil faz. **Nenhuma foi testada.**
- F1d3 (INSTRUMENTAÇÃO DE DIAGNÓSTICO adicionada após o relato): logs de progresso em `init_udev`
  (`DRM/GBM abertos`, `renderer EGL/GLES criado`, `formatos de render`, `conectores inicializados`) e heartbeat
  `udev: loop de render vivo` a cada 250 ticks (~4 s). O script `~/run-aeroshell-udev.sh` agora grava
  `~/aeroshell-udev-YYYYMMDD-HHMMSS.log` via `tee`. Testado nesta VM: log criado e falha `EAGAIN` registrada.
  **Falta o humano reexecutar no tty3 e enviar o log** — é a única forma de reproduzir o travamento sem VT aqui.
- F1d3 (JUIZ VISUAL — pendente a reexecução): o comando do RUNBOOK precisa ser repetido com logging e o veredito
  dado depois. O agente NÃO trocou VT, NÃO abriu sessão real no tty3 e NÃO encerrou aplicação alguma nesta etapa.
- F1d3 (LACUNA DE AMBIENTE, amostra preservada em `/tmp/opencode/f1d3/validate.log`): `--backend udev` nesta VM
  (sem VT ativo) loga `sessão libseat aberta seat0`, `dispositivo DRM selecionado /dev/dri/card0` e termina
  com `Error: "abrir \"/dev/dri/card0\" via libseat: Failed to open device: Resource temporarily unavailable
  (os error 11)"`, `EXIT=1`. Diagnóstico (`estou supondo`): a seat não está ativa sem VT, então libseat concede
  a sessão mas não o device. Isso NÃO é evidência de que o backend funciona ou falha em DRM real — só de que a
  cadeia até a abertura do device compila e falha graciosamente (sem panic).
- F1d3 (NÃO MEDIDO): formats/modifiers negociados com o EGL renderer, modo/CRTC efetivamente aplicado, vblank,
  damage, ausência de flicker, retomada após VT switch, desempenho/RAM em GPU real (VM sem 3D).
- F1d1 (LACUNA **FECHADA** por F1d2, amostra preservada): o plano pedia
  `ldd … | grep -E "gbm|libdrm|libinput|libseat|libudev"` → 5 linhas; em F1d1 mediu **0** (`readelf -d` só
  libxkbcommon/libgcc_s/libc/ld-linux) porque nada chamava as funções C (`udev.rs` era stub) e `--as-needed`
  descartava as `.so`. F1d2 mediu `libgbm.so.1` em `DT_NEEDED` e `libgbm.so.1` + `libdrm.so.2` no `ldd`.
  A hipótese do `--as-needed` estava correta. A amostra de 0 linhas NÃO foi apagada.
- F1d2 (NÃO MEDIDO): enumeração completa de modifiers (a API `format_modifier_plane_count` devolveu `None`
  para todos os consultados, §6), `EGLDevice`/`EGLDisplay`/`ElesRenderer`, abrir seat real via libseat
  (logind/PAM, líder de sessão), cursor no backend udev (§6), renderizar/conectar modo de fato (F1d3),
  desempenho/RAM em GPU real (VM sem 3D). A validação visual em tty3 (F1d3) é **do humano** (troca o VT e fecha).
- F1d1 (S1 SOB CARGA, amostra preservada): a 1ª execução de `bash tests/integration/s1_nested.sh`
  imediatamente depois de `cargo build --workspace` falhou o `sleep 4` da fase visual → EXIT=1
  ("FALHA: cliente não mapeou toplevel dentro do compositor"). A reexecução em máquina ociosa deu EXIT=0
  (duas linhas `OK:`). Nenhuma amostra apagada; o gate (`bun test`) passou na mesma sessão. Coerente com o
  flake de carga já registrado em `docs/RUNBOOK.md:12-14`; causa exata não identificada.
- F1c (JUIZ VISUAL — FECHADO, 2026-11-21): humano relatou "somente o arrastar do foot a esquerda
  funciona e mal", "vejo janela com decoração nos outputs; não deveria" e, por fim, "estou vendo uma
  moldura ocupando a metade inteira da tela". O modelo NÃO lê imagem; os relatos por texto são dados.
  Causa 1 (arrasto): `input.rs:136` usava só o primeiro output (640×480) contra host 1280×480 — corrigido
  por união das geometrias (`absolute_pointer_space`), teste falhou antes (`640` vs `1280`) e passou depois.
  Causa 2 (moldura gigante): fallback N>1 desenhava o frame do outro output inteiro (652×516) na origem;
  corrigido em §3, medição `{0:20, 1:32}` → `{0:20, 1:0}` colunas. A decoração dos FEET (titlebar "janela")
  é por design do F0/F1b e permanece; os outputs em si não têm título. Humano TESTOU a instância aberta
  (2 outputs, 2 `foot`, arrasto dos dois lados) e respondeu **"aprovado visualmente"**; encerrou a aplicação.
  F1c FECHADA. Não iniciar F1d sem aprovação separada.
- F1c (NÃO MEDIDO): desempenho/FPS/latência em GPU real (VM sem 3D); resize dinâmico da janela host N>1
  (o host fica fixo em N tiles); damage incremental do host (full por decisão, §6). N=1 manteve `age=0`.
- F1b (NÃO MEDIDO): desempenho/FPS/latência do headless em GPU real (VM sem 3D, llvmpipe); o que foi medido
  foi funcionamento, contagens de frames, damage (sempre full, §6) e pixels não nulos. Nenhum juiz visual se
  aplica ao headless (sem tela); o winit não foi tocado nesta tarefa.
- F0 (BASELINE-FALHA PRESERVADO, NÃO SÃO GATES): `frame_cold_start.py` falha o sub-cronômetro >300 ms
  (374–415 ms medidos na via A) e `frame_resize_sync.py` falha pela métrica antiga de p95. Ambos ficam
  registrados como baseline-falha; 02p e 02q foram FECHADAS pelo juiz visual, não por esses harnesses.
  Não alterar os testes para passar, não re-litigar resize/cold-start, não descartar amostras.
- 02q (LACUNA DE MEDIÇÃO, tarefa FECHADA pelo humano): o cronômetro map→`RESIZE_GLUE` continua >300 ms
  (363–415 ms nas amostras da via A), enquanto o humano viu decoração instantânea e declarou "visualmente
  perfeito e bem otimizado". Por `AGENTS.md` §0.4/§0.9, o relato visual prevalece e fecha a tarefa; o harness
  provavelmente mede um intervalo que não corresponde ao gap percebido. NÃO alterar o teste nem descartar
  números. Baseline de 1–2 s não reproduziu (469–543 ms). VM sem 3D: nenhuma conclusão de FPS/GPU real.
  A intermitência do primeiro buffer segue sem causa identificada (§6) e não foi "corrigida".
- 02p: desempenho/PSS/latência em GPU real NÃO medidos (VM sem 3D). Readback síncrono altera cadência;
  os testes provam pixels/geometria/funcionamento, não fps. Multi-output e crescimentos repetidos além
  de um salto não foram medidos. O critério visual nested com 2 `foot` foi APROVADO pelo humano (§3).
- **02o (JUIZ VISUAL NEGOU A MEDIÇÃO — 2026-11-20): humano, ao observar as 3 execuções do harness de pixels,
  disse "nas 3x da medição o problema ocorre ainda".** Ou seja: o flicker aparece enquanto a grade 64x40 reporta
  `flashes_candidatos=0` e `diff_medio_max=4,81/255`. A medição NÃO capturou o que o humano vê — lacuna, não
  amostra descartada. Não escrever "sem flicker" com base nesses números. Causas em aberto (suposições, não
  conclusão): agregação 20x20 pode apagar o transiente; o flicker pode estar fora da janela amostrada ou ser
  percepção de movimento não capturada por diff de médias. Par 9-slice/nativo foi capturado em só 1/3 execuções
  (diff 0,00) — insuficiente para confirmar ou refutar a hipótese "visualmente idênticos" da 02n (§6).
- **02n (JUIZ VISUAL NEGOU — 2026-11-20): humano, ao arrastar E SOLTAR bordas das 2 janelas na instância aberta
  `wayland-aero-02n` (2 `foot`): "problema ainda ocorre."** Dado prevalece sobre os 3× `VEREDITO: OK` do teste
  automatizado (AGENTS.md §0.4). O teste mede SÓ a sequência de estados (`rescaled`, tamanho do buffer, serial do
  configure); ele NÃO mede pixels nem a percepção de flicker do humano — lacuna de medição registrada, não amostra
  descartada. A hipótese "visualmente idênticos em tema uniforme" (§6) segue SEM confirmação e pode explicar a
  divergência: o 9-slice do buffer 708 e o nativo 768 podem diferir mesmo com scale=1.
  Não foi registrado se o flicker voltou no press/arrasto (02m aprovada) ou segue só no soltar — perguntar ao humano
  antes de qualquer diagnóstico. NÃO corrigir sem decisão; a instância 02n já não existe (§4).
- 02n (NÃO MEDIDO): desempenho em GPU real — VM sem 3D; só funcionamento/geometria foram medidos. Diferença de
  pixels entre 9-slice(708) e nativo(768) com tema não-uniforme também não foi medida (hipótese em §6).
- 02l (JUIZ VISUAL NEGATIVO PARCIAL — 2026-11-20, teste feito na instância 100840/101188): humano:
  **"teste visual feito. Está quase perfeito o ponto do achatamento resolveu. Mas ao clicar nas bordas (na
  barra de titulo NAO), vejo que pisca, a barra de titulo inteira encolhe horizontalmente incluindo seu
  conteudo, achatando ele e depois volta ao normal, a decoracao inteira parece encolher para uma janela
  menor. TUdo isso muito rapido, me milesimos de segundos."** Dado: o achatamento DURANTE o arrasto foi
  aprovado; resta um flash de milissegundos no `button press` das bordas (não no move pela barra).
  **REPRODUZIDO com medição objetiva** em `/tmp/opencode/aero-02l-visual.log` (log do teste do humano):
  7 frames consecutivos de `window_id=1` com `buf=200x200 target=708x530 rescaled=true
  slice_scale_x=3.7021 slice_scale_y=3.0122` (t=1795160630897993417 … 1795160630949615576, ~5,2 ms),
  imediatamente antes de `buf=708x530 rescaled=false`. É o buffer mínimo inicial do WebKit (200×200)
  sendo escalado 3,7× para a moldura inteira no instante em que a janela é associada/clicada — causa
  DIFERENTE do achatamento corrigido pela 02l (que era escala no arrasto). Sinal de apoio:
  `FRAME_BUFFER_COMMIT buffer=None expected=Some((630,504)) ok=false`. Hipótese anterior (geometria alvo
  mudar antes do repaint grande) foi **REFUTADA**: diagnóstico com press sem motion
  (`/tmp/opencode/02l_flash_probe1.log`) mostrou `target=708x530` estável, transição de buffer em +79,8 ms
  com `SLICE_GEOM` correto (126/480/102) e nenhum frame anômalo; o defeito só apareceu com a 2ª janela.
  Critério 5 do cartão 02l **NÃO passou**; tarefa NÃO concluída. NÃO corrigir sem decisão: é causa nova,
  fora do cartão aprovado (AGENTS.md §0.4/§0.8). Instância 2 encerrou graciosamente após o teste.
- **02m (JUIZ VISUAL NEGOU — 2026-11-20): humano, ao clicar/arrastar as bordas das 2 janelas na instância
  `wayland-aero-02m`: "Ainda pisca". Dado prevalece sobre o teste automatizado (AGENTS.md §0.4/§0.9).
  `buf=200x200` foi mesmo eliminado (3× OK), mas o flash persiste por OUTRA causa, encontrada nos logs já
  coletados (sem nova correção): `/tmp/ibuf-comp-104057.log:1325-1326` →
  `RESIZE_GLUE window_id=1 content=0x0 target=12x36 buffer=708x530 rescaled=true` e
  `RESIZE_SLICE ... target=12x36 slice_scale_x=0.0014 slice_scale_y=0.0020 title_gap_w=1` — no primeiro
  `map` o cliente ainda tem geometry 0×0 (`aero-02m-visual.log`: `width: 0, height: 0`), então
  `target = content + extras = 12x36` e o 9-slice comprime a decoração inteira por 1 frame. Isso bate com o
  relato original do juiz ("a decoracao inteira parece encolher para uma janela menor").
  Causa está no COMPOSITOR, fora da decisão 0 autorizada (shell + spike). Regra das duas tentativas atingida:
  (1) adiar criação no shell → deadlock; (2) `set_default_size` no shell → teste passou, juiz negou.
  PARAR e pedir decisão antes de qualquer terceira abordagem. 02m e 02l NÃO CONCLUÍDAS no visual.
  Instâncias 105740/105748/105755/105827/105852/105864 encerradas após o teste (não pelo agente).**
- **02m (3ª ABORDAGEM — JUIZ VISUAL PENDENTE, 2026-11-20): humano autorizou nova especificação no compositor
  (remover `source=grab-start`, zero configures no grab, só `source=final` no unset — §3). Técnica pronta e
  provada por medição (3× `configures_no_grab=0`), MAS ainda NÃO houve juiz visual desta versão. Até o humano
  clicar/arrastar as bordas das 2 janelas na instância nova e dar o "ok" explícito, 02m e 02l seguem NÃO
  CONCLUÍDAS (AGENTS.md §0.9). A hipótese do `target=12x36` permanece registrada acima como descoberta; a
  correção autorizada ataca a causa apontada pelo humano (configure de freeze no press), não aquela.**
  Instância visual desta 3ª versão: socket `wayland-aero-02m-v3`, compositor 112693, shell 112699,
  `foot` 112772/112826, log `/tmp/opencode/aero-02m-v3-visual.log`, 2 associates. Humano testou e encerrou
  (não foi o agente). Sem `--features bench`, por isso a prova do zero-configure veio do harness.
- **02m (JUIZ VISUAL DA 3ª ABORDAGEM — VEREDITO MISTO, 2026-11-20): humano, ao clicar/arrastar bordas das 2
  janelas na instância v3: "o proplema no clique/arrasto resolveu totalmente. Mas agora tem o mesmo problema
  no soltar o arraste. Se eu clicar e segurar uma borda e nao arrastar, nao ocorre problema ao soltar o clique.
  Mas se eu clicar, segurar, arrastar e depois soltar ocorre o problema da moldura redimensionar rapido dando
  efeito de flicker."** Dado prevalece (AGENTS.md §0.4/§0.9). Press/drag = OK; release com arrasto = flicker.
  Medição objetiva do release no log do harness `/tmp/gh-comp-110069.log`: após `FRAME_CONFIGURE ... source=final
  w=756 h=546` (target 768×582), o buffer fica CONGELADO em 708×530 por 10 frames ≈ 79,7 ms com
  `rescaled=true sx=1.0862 sy=1.1053`, depois salta para 768×582 `rescaled=false sx=1.0` — é o "redimensionar
  rápido" visto. Sem arrasto não há mudança de conteúdo, logo o `source=final` não causa flicker (bate com o
  relato). Causa = `source=final` no `unset` (`grab.rs:403-420`), que a ESPECIFICAÇÃO do humano mandou MANTER.
  NÃO mexer sem decisão. 02m/02l NÃO CONCLUÍDAS no visual.**
- 02l (JUIZ VISUAL — RELATO FAVORÁVEL ANTERIOR, agora superado): humano (2026-11-20):
  **"vi na execução dos seus testes que o problema foi resolvido."** Veredito positivo registrado como dado.
  Como o relato veio da execução dos harnesses (que abrem janela winit + `foot`), e não de uma sessão manual
  formal com 2 `foot`, abri instância do RUNBOOK (compositor 100840, 2 `foot`: 100924/100972) e pedi arrasto
  lento/rápido + "ok" explícito olhando a tela (cartão 02l:46, AGENTS.md §0.9). A instância **encerrou com
  shutdown gracioso** (`FRAME_SUMMARY samples=2`, sem `panicked`/SIGSEGV; foot com `Broken pipe`) — o agente
  não executou nenhum encerramento; muito provavelmente o humano fechou a janela ao terminar o teste.
  O log `/tmp/opencode/aero-02l-visual.log` contém medição de arrasto REAL das duas janelas
  (`window_id=0` e `1`): `title_left_w=120`, `title_right_w=96` constantes em todas as amostras, com
  `title_gap_w` variando (96→402/618) — prova complementar, não substitui o olho do humano.
  Vou reabrir UMA instância e deixá-la aberta para o "ok" formal. Até lá, item NÃO concluído.
- 02l/02k: fidelidade de PIXELS (amostragem de texto/botões na textura) e suavidade percebida em GPU real —
  **não medidas**. A prova objetiva nova (`SLICE_GEOM`) lê a geometria REAL dos elementos compostos, não os
  valores calculados para `RESIZE_SLICE`, mas ainda não amostra a textura; VM sem 3D, sem conclusão de
  desempenho. O relato do humano prevalece sobre a medição (AGENTS.md §0.4).
- 02k: Juiz visual do achatamento foi atendido pela 02l; 02k não tem "ok" visual próprio e não é reaberta.
- 02j (VALIDAÇÃO VISUAL REALIZADA, RESULTADO NEGATIVO): teste manual com aplicação aberta (PID 84904, socket
  `wayland-aero-02j-visual`) e instrução explícita de olhar barra e bordas; humano respondeu: **"no teste manual
  pude ver que o flicker é geral, incluindo as bordas"**. Veredito: **NÃO validado**; o "ok" NÃO foi dado.
  O relato contradiz o anterior ("só na barra, moldura ok") — o dado do teste manual prevalece (§6).
  A 02j não muda aparência, logo NÃO é ela que introduziu o flicker, mas a instrumentação só prova que a escala
  esteve ativa por 5,4 s seguidos; **não mede o flicker em si**. Falta a medição que Capture posição da decoração
  por frame durante o arrasto (lacuna registrada, exige cartão novo). Aplicação deixada aberta; quem fecha é o
  A instância 84904 já não está rodando: o log termina com `FRAME_SUMMARY samples=1` (shutdown gracioso) e
  não há `panicked`/`SIGSEGV`; o agente NÃO executou nenhum comando de encerramento — muito provavelmente o
  humano fechou a janela ao terminar o teste (confirmar). Instância 02f (PID 72480) continua aberta.
- 02j: não medido — latência/qualidade visual em GPU real; os 3 modos nomeados foram medidos só em VM sem 3D.
- 02f: JUIZ VISUAL NEGATIVO (2026-11-20): humano: **"A moldura ainda está chegando atrasada"**. O pré-filtro
  (razão 0,286, 14/14 configures no cliente, final no tamanho real) NÃO mediu o que a pessoa viu — mesma lacuna
  da 02e. 02f NÃO está concluída. Instância `wayland-aero-02f` (PID 72480) deixada aberta; aguarda decisão.
- 02f: nenhuma conclusão de desempenho — VM sem 3D; GPU real não medida. O debounce de 200 ms satisfez a métrica,
  mas NÃO resolveu a queixa visual; não aumentar nem diminuir o debounce sem nova decisão.
- 02f/02e (decisão pendente): 3 famílias já falharam no juiz — (1) 02c configure no motion + timer, (2) 02e escala
  no render, (3) 02f higiene de configures. Nova tentativa exige escolha do humano (AGENTS.md §3.12).
- 02e: JUIZ VISUAL NEGATIVO/PARCIAL (2026-11-20, instância `wayland-aero-02e`, PID 63221 deixada aberta):
  humano: **"o atraso está menor, porém ainda feio. Não está natural suave como Windows, GNOME, etc."**
  Ou seja: a colagem espacial melhorou, mas a QUALIDADE VISUAL do arrasto não passou. o "ok" NÃO foi dado;
  02e NÃO está concluída. Instância não foi fechada pelo agente.
- 02e (LACUNA DE MEDIÇÃO): `frame_resize_glue.py` mede só geometria por frame (`target == content + extras`);
  ele não mede distorção da escala, suavidade da cadência nem aparência de bordas/cantos. O relato do humano
  prevalece sobre o teste automatizado (`AGENTS.md §0.4`). Criar medição que faltava antes de qualquer nova
  mudança de código.
- 02e (dados do arrasto REAL, `/tmp/opencode/aero-02e-visual.log`): 21.821 linhas `RESIZE_GLUE` válidas;
  `target != content+extras` = 0; `rescaled != (buffer!=target)` = 0; 1.276 com `rescaled=true`,
  20.545 com `false`; `max |buffer-target|` = 331 px. O invariante espacial tambem vale no arrasto humano —
  logo a métrica NÃO captura a queixa. Uma correlação ingênua por timestamp sugeriu 2.858 divergências,
  mas estava ERRADA (dois clientes intercalados); corrigida por `window_id`, não há divergência. Não descartada:
  foi diagnóstico meu, registrado para não virar "achado".
- 02e (sinal para 02f, medido no mesmo log): durante um único arrasto os seriais de `frame configure` foram
  1→92 e houve 199 linhas "geometria real do cliente". O compositor inunda o WebKit com configures; o buffer
  nativo dificilmente alcança o alvo, deixando a escala ativa. Hipótese: a higiene de configures de 02f pode
  reduzir a sensação de atraso sem mudar a arquitetura de 02e.
- 02e: nenhuma conclusão de desempenho — medições feitas em VM sem 3D; GPU real não foi medida.
- 04b: nada pendente — veredito visual POSITIVO do humano e causa do travamento registrados em §3. Instância da
  validação fechada pelo próprio humano; nenhum PID meu ficou órfão.
- 03: latência do IPC, taxa de perda e reconexão NÃO medidos (S4 do plano mestre, fora do cartão).
- Latência de resize com WebKit real; latência de ponteiro; qualquer desempenho em GPU real (tudo foi em VM sem 3D).
- Número de processos/PSS com `related-view` foi medido só em VM; ADR 0001 está "aceito provisoriamente".
- Estabilidade do `ext-background-effect` (blur): não implementado; Smithay 0.7 não tem handler.
- 02c: validação visual REALIZADA COM RESULTADO NEGATIVO (2026-11-20): humano arrastou a borda (lento e rápido) na
  instância com 2 `foot` e disse "ainda está do mesmo jeito". NÃO é "validado"; item segue em aberto em 02d.
  Instância fechada pelo próprio humano; log `/tmp/opencode/aero-vis5.log`.
- 02c: desempenho em GPU real (VM sem 3D); 3 execuções seguidas com a LOW final (a métrica foi reprocessada de 3
  execuções históricas, uma por variante — não são 3 repetições da mesma variante).
- Nenhuma validação visual pendente de 02b: validada pelo humano em 2026-11-20 (registro em §3). Demais itens visuais
  das seções anteriores seguem como histórico, não reabertos nesta sessão.
- 02b (honestidade metodológica): **resize**, **clique na barra da janela de trás** e **barra da frente sobre conteúdo**
  foram vistos FALHANDO antes das respectivas correções. A medição de **foco** foi adicionada depois do `nth` e não
  foi vista falhando antes; medida depois: foco em A, nunca em B. No 3º caso, B já estava focada: o critério medido
  é "o foco não salta para A", não "novo focus=true em B".
## 8. Ambiente
Ubuntu 26.04 em VirtualBox (sem 3D), host pode ser X11. WebKitGTK 6.0 (2.52.x), gtk4-layer-shell, Rust via rustup, Bun.
WPE ausente no Ubuntu 26.04. Alvo de pacote: Debian 13 (trixie) + Ubuntu 26.04.
 
## 9. Handoff (reescreva a cada sessão, 10 linhas no máximo)
- **Fase atual: F1.** F1a/F1b/F1c FECHADAS pelo juiz; F1d1/F1d2 FECHADAS no técnico. **F1d3 NÃO FECHADA: juiz negou 3× + nova execução em 2026-11-22 negada.** Push WIP falhou por credencial; commits locais `f0da56a`/`a56b85a`/`9b5db36` + o novo commit prontos. Sem push de conclusão.
- **Fato do humano:** `is_empty=true` = idle normal (sem damage), NÃO bug. Não alterar. Input/VT/vblank provados bons; cursor é cartão separado (§6).
- **5º log analisado:** `/home/daniel/aeroshell-udev-20261122-051134.log`. Sessão/DRM/output ativos; decoração commitou `1932x991 ok=true`; `ordem de render` com `client@`+`frame@`; 16 frames `queued=ok` + vblank `submitted=ok` n=3..10 às 08:11:36.785–08:11:37.107. A cena com o cliente foi submetida; o juiz ainda viu só a decoração.
- **Erro do harness corrigido:** esse log NÃO tem `FOOT_COMMIT`/`UDEV_FRAME_CB` porque `~/u` compilava sem `--features bench`. `scripts/run-udev-diag.sh` agora usa `cargo build --workspace --features bench`; `strings` com bench = 1/1 para as sondas. RUNBOOK atualizado.
- **Sondas (código já commitado `9b5db36`):** `FOOT_COMMIT`/`UDEV_FRAME_CB` só sob bench, leitura sem drenar; analisador `udev_client_compose_probe.py` escrito antes e visto FALHANDO. Headless real: 7 commits/0,76 s, `pending_seq=0,1,1,0,1,1,0`.
- **Provas:** gate `check.sh` EXIT=0, clippy bench 0, `--no-default-features` 0, fmt 0, `strings` sem bench = 0.
- **Próximo passo imediato (NÃO executado):** humano roda `~/u` **novamente** (agora com bench) no tty3, interage ~10 s com o `foot` e envia o log; agente roda `python3 tests/integration/udev_client_compose_probe.py <log>`. Se `CALLBACK_NAO_CHEGA`, candidato: `send_frames_surface_tree` no loop udev (`utils.rs:217`); se `SEM_DAMAGE_FORA_DA_CENA`, investigar import GBM/composição. Só depois: cartão de correção com teste-first + novo juiz visual.

