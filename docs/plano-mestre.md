# AeroShell — Plano de Trabalho (Agents.md)

> Documento-guia para agentes e desenvolvedores. Toda implementação deve seguir
> este plano. Mudanças de arquitetura exigem ADR (regra 3). **Sempre commitar
> quando uma etapa for validada** (regra 10): commits pequenos, descritivos e
> com o gate que os autorizou. Idioma do projeto: português (PT-BR).

---

## 1. Visão

Construir o **AeroShell**: um *framework* para criar compositores Wayland
completos cuja **toda parte visual** — taskbar, widgets, menus, workspaces,
decoração de janela, notificações, **tela de lock** e **tela de login** — é
escrita em TSX/JSX e renderizada em WebView nativo do sistema (Ubuntu/Debian).
Nada visual é desenhado em código nativo, exceto o conteúdo dos clientes
Wayland, o cursor e a tela de cor sólida do fallback seguro do lock.

Inspiração: `github.com/wnayes/bond-wm` (X11 + Electron). Aqui: Wayland +
WebView do sistema + Rust.

---

## 2. Stack obrigatória

- **Rust + Smithay** (estável fixado, ver §8) + calloop; backends winit (dev) e
  udev/DRM/libinput/libseat/GBM. Referência: exemplo `anvil` **no tag fixado**.
- **Runtime de política em Bun (TypeScript)**; UI modular em TSX com adapters
  para React, SolidJS e Vue.
- **WebView**: WPE WebKit embutido, offscreen → dmabuf → textura GLES do
  Smithay, atrás do trait `WebSurfaceBackend` (fallback: CEF OSR). Versão/API
  **devem ser verificadas na F0 (S2)** — não presumir.
- **Autenticação**: PAM (crate Rust adequada ou FFI mínima auditável) e cliente
  do protocolo IPC do greetd. Formato das mensagens **consultado na spec** (§8).

---

## 3. Arquitetura (não desviar sem ADR)

- **Mecanismo em Rust, política em TS.** Alta frequência (mover/redimensionar,
  animações de geometria, hit-test, timers de idle, estado de lock) no Rust;
  layouts, regras, atalhos, workspaces e política de idle no Bun.
- **Processos:** `aeroshell-compositor` (Rust) ⇄ runtime (Bun) via Unix socket +
  MessagePack; WebViews em processo web compartilhado, carregando `aero://`
  (produção) ou dev server local (HMR, **só em dev**).
- **Superfícies web:** desktop (fundo), panels (anchor/size/exclusiveZone),
  overlay (tela cheia acima das janelas), frame (1 WebView por janela, pool
  reutilizado), lock (1 por output, só com sessão trancada), greeter (1 por
  output, só no modo `--greeter`).
  Z-order: `desktop < [frame+cliente por janela] < panels/overlay < lock`.
- **Input:** hit-test pelo canal alfa do último frame da WebView; pixel
  transparente passa o input aos clientes.
- **Decoração:** força SSD via xdg-decoration. O Frame declara insets; o
  compositor calcula a área do cliente e cria zonas de resize invisíveis.
  `aero.windows.beginMove/beginResize` entregam o grab ao Rust.
- **Estado:** Rust é a fonte da verdade; emite snapshot + patches com revisão.
  SDK expõe stores reativos; adapters via `useSyncExternalStore` (React),
  signals (Solid), refs (Vue).
- **Layouts:** funções puras `(ctx) => Geometry[]` no Bun, aplicadas como
  transação atômica no próximo frame, com `animate` interpolado no Rust.
  Floating, tiling e híbrido por workspace.
- **Tipos Rust→TS gerados automaticamente** (ts-rs ou specta) a partir de
  `aeroshell-ipc`. **Nunca duplicar tipos à mão.**
- **Blur-behind:** o compositor aplica; a web só declara a região.
- **Limites de crate:** `aeroshell-web` não conhece Smithay;
  `aeroshell-compositor` não conhece WPE/CEF (só o trait). Tudo tempo-real e
  sensível (lock, auth) em Rust; decisões de política no Bun.

### Idle e energia

- Timers de inatividade no Rust (calloop), reiniciados por qualquer input.
  Política em TS: `idle: [{ after, action }]` com `dim | lock | dpms-off |
  suspend | custom(fn)`, eventos `onIdle/onResume`, `aero.idle.inhibit()/release()`.
- Implementar `ext-idle-notify-v1` e `idle-inhibit-unstable-v1`. Respeitar
  inibidores do logind (`org.freedesktop.login1 Inhibit`) e do portal de
  Inhibit. Heurísticas configuráveis (fullscreen, áudio via PipeWire).
- DPMS via propriedade ACTIVE do DRM (verificar suporte no Smithay fixado) e,
  opcionalmente, `wlr-output-power-management`.
- Suspend via logind. **Delay inhibitor `PrepareForSleep`: a sessão DEVE estar
  trancada ANTES de dormir.** Lid switch (libinput) configurável para
  lock/suspend.

### Lock screen

- Máquina de estados **no Rust**. Lock embutido (UI TSX) **e**
  `ext-session-lock-v1` (swaylock etc.).
- **Fail-secure**: se WebView do lock ou runtime Bun morrer, a sessão permanece
  trancada (cor sólida), a UI reinicia; nunca desbloquear por erro/timeout/crash.
- Trancado: só superfícies de lock recebem foco/input; atalhos globais,
  screencopy/screenshots e gravação recusados; janelas/overlays não renderizados
  nem recebem eventos. Troca de VT permitida (configurável).
- PAM: serviço próprio `/etc/pam.d/aeroshell` (incluindo pilha padrão do
  sistema). Conversa repassada à UI de forma **genérica** (secretos, visíveis,
  informativos, erro) para senha, digital (fprintd) etc. PAM em
  thread/processo dedicado (bloqueante). Respeitar atraso após falha, limitar
  tentativas por tempo, mostrar erros.
- **A senha NUNCA passa pelo Bun, SDK geral, logs ou métricas.** Capability
  restrita `lock.auth` só para superfícies de lock; buffers com `zeroize`.
- Bundle de lock autônomo, só `aero://`: sem dev server, sem DevTools, sem
  navegação externa. Funciona com Bun fora do ar.
- Notificações na lock: opção de ocultar conteúdo.

### Greeter

- Modo `aeroshell-compositor --greeter`, executado sob greetd. Uma superfície
  `greeter` por output, mesmo componente TSX, **sem runtime Bun** (bundle
  pré-compilado, menor superfície de ataque).
- Cliente IPC do greetd via `GREETD_SOCK`: criar sessão, repassar prompts PAM à
  UI, responder, iniciar sessão ou cancelar.
- Dados: usuários (AccountsService via D-Bus, fallback `/etc/passwd` por faixa
  de UID), sessões em `/usr/share/wayland-sessions`, último usuário/sessão,
  ações de energia, troca de layout de teclado, multi-monitor.
- Mesmas regras de segurança do lock (senha não sai do Rust).

### Contrato mínimo do SDK

`aero.windows` (list, focus, close, minimize, maximize, fullscreen, move,
resize, beginMove, beginResize, thumbnail), `aero.workspaces`, `aero.outputs`,
`aero.apps`, `aero.input` (binds, gestos), `aero.idle` (configure, inhibit,
onIdle, onResume), `aero.system` (battery, audio, network, bluetooth,
brightness, media, session/power), `aero.notifications`, `aero.tray`,
`aero.clipboard`, `aero.surfaces`.
Entradas restritas: `@aeroshell/sdk/lock` (state, auth session: prompt,
respond, cancel, result) e `@aeroshell/sdk/greeter` (users, sessions, login,
power, layouts).
Config: `desktop.config.tsx` com
`defineDesktop({ surfaces, frame, layouts, rules, keybindings, idle, lock, greeter })`.
Pacotes npm: `@aeroshell/sdk`, `@aeroshell/react`, `@aeroshell/solid`,
`@aeroshell/vue`.

---

## 4. Estrutura de arquivos (tree completa — criar só o necessário por fase)

```
aeroshell/
├─ Cargo.toml, package.json, justfile, rust-toolchain.toml
├─ crates/
│  ├─ aeroshell-compositor/src/{main.rs (--greeter), state.rs, bridge.rs, xwayland.rs,
│  │    backend/{winit,udev}.rs,
│  │    protocols/{xdg_shell,xdg_decoration,layer_shell,dmabuf,data_device,input_method,session_lock,idle_notify,idle_inhibit,screencopy,output_management,output_power}.rs,
│  │    input/{keyboard,pointer,grabs,gestures}.rs,
│  │    render/{mod,elements,damage,animation}.rs,
│  │    shell/{window,workspace,output,transaction,z_order,idle,lock}.rs}
│  ├─ aeroshell-web/src/{lib.rs (trait WebSurfaceBackend), surface.rs (Desktop|Panel|Overlay|Frame|Lock|Greeter), pool.rs, scheme.rs, message_handler.rs, capabilities.rs, backends/{wpe,cef}.rs}
│  ├─ aeroshell-ipc/src/{messages,codec,server,types}.rs
│  ├─ aeroshell-auth/src/{lib,pam,conversation,greetd,users,sessions}.rs
│  ├─ aeroshell-services/src/{battery,network,audio,bluetooth,brightness,media,session,logind_inhibit,notifications,tray,clipboard}.rs
│  └─ aeroshell-apps/src/{lib,desktop_entry,icon_theme,search,launch}.rs
├─ packages/
│  ├─ sdk/src/{index,define,client,store}.ts + generated/ + api/{windows,workspaces,outputs,apps,input,idle,notifications,tray,clipboard,surfaces,system/*}.ts + lock.ts + greeter.ts
│  ├─ react/ solid/ vue/  (adapters; incluem /lock e /greeter)
│  ├─ runtime/src/{main,config-loader,ipc-client,state,keybindings,rules,idle-policy,dev-server}.ts + layout/{engine,float,tile,monocle,types}.ts
│  ├─ cli/ (comando `aero`: create | dev | build | doctor)
│  └─ ui-kit/ (opcional)
├─ templates/{win11-like,gnome-like,tiling-minimal,blank}/  (cada um com Lock.tsx e Greeter.tsx)
├─ examples/, protocols/, tests/{integration,layouts,security,bench}/
├─ docs/{architecture.md, security-model.md, getting-started.md, sdk-reference/, guides/, adr/, progress.md}
├─ packaging/{debian/, aeroshell.desktop, pam.d/aeroshell, greetd/config.toml, systemd/}
└─ .github/workflows/
```

Ajustes só com justificativa em ADR.

---

## 5. Fases (implementar UMA por vez; só avançar com aprovação)

> **Cada etapa validada gera um commit imediato (regra 10).** O fim de uma fase
> também gera commit próprio com o relatório de números e os ADRs. Não agrupar
> entregas de fases diferentes.

| Fase | Conteúdo | Gate de aceite |
|---|---|---|
| **F0** | Spikes go/no-go S1–S5 + relatório com números e ADR de decisão | ver §7 |
| **F1** | Núcleo do compositor: DRM, multi-monitor, damage, xdg-shell, decoration, layer-shell, popups, subsurfaces, viewporter, fractional-scale, dmabuf, presentation-time, data-device/primary-selection, cursor-shape, text-input/input-method | — |
| **F2** | Ponte web + SDK v0 + HMR via `Bun.serve` | — |
| **F3** | WM: float/tile, workspaces, regras, atalhos, snap, animações, overview com thumbnails | — |
| **F4** | Serviços: apps, notificações, tray, áudio, rede, bluetooth, bateria, brilho, MPRIS, logind | — |
| **F5** | IDLE + LOCK: timers/política, ext-idle-notify, idle-inhibit, inibidores logind/portal, DPMS, suspend com lock prévio, lid switch, máquina de estados, ext-session-lock, lock TSX com PAM, fail-secure | matar Bun NÃO destranca; matar WebView do lock NÃO destranca e UI reinicia; swaylock funciona; trancado: nenhum input a janelas e screencopy recusado; suspend volta trancado; fullscreen inibe idle; senha em nenhum log/Bun |
| **F6** | GREETER: `--greeter`, cliente greetd, usuários/sessões, UI TSX, packaging | testado em VM |
| **F7** | Compat: XWayland, screencopy/portal, output-management, gestos, tablet, xdg-activation | — |
| **F8** | DX: CLI `aero`, 3 templates (com Lock/Greeter), docs, .deb + wayland-sessions | — |
| **F9** | Endurecimento: recuperação de crash WebView (UI fallback), fuzz de protocolo, testes de segurança, benchmarks, CI headless, permissões por superfície, CSP | — |

---

## 6. Regras de trabalho

1. Antes de cada fase: listar o que vai fazer, riscos e critérios de aceite;
   depois implementar. Criar só os arquivos da árvore necessários à fase atual.
2. Código compilável a cada passo (`cargo check`, `bun test`, lint). Testes para
   IPC, layouts, estado, idle e máquina de estados do lock; integração com
   backend headless/winit.
3. **Não inventar APIs** de Smithay, WPE, PAM, greetd ou D-Bus: consultar doc ou
   código-fonte da versão fixada; se não tiver certeza, dizer e criar spike.
4. Performance é requisito: sem cópia de CPU por frame, renderizar só com
   damage, pausar WebViews invisíveis. **Medir e reportar.**
5. Segurança é requisito de primeira classe: WebViews só carregam `aero://` ou
   dev server local (só em dev); sem navegação externa; permissões por
   superfície; fail-secure no lock; segredos nunca em logs.
6. DX é o produto: dev frontend cria desktop completo (incluindo lock/login) sem
   saber Wayland. Toda API pública com tipos, JSDoc e exemplo.
7. Manter `docs/adr/`, `docs/security-model.md` e `docs/STATE.md` atualizados. `docs/progress.md` é só histórico.
8. **Testar lock, PAM e greetd apenas em VM/ambiente isolado. Avisar antes de
   qualquer passo que altere `/etc/pam.d` ou config do display manager do host.**
9. Se um requisito conflitar com a realidade técnica (ex.: WPE inviável),
   **parar, explicar com dados e propor alternativa** em vez de improvisar.
10. **Sempre commitar quando uma etapa for validada.** Assim que um passo passar
    em seus critérios de aceite (`cargo check`, `bun test`, lint, gate medido),
    criar um commit pequeno e descritivo **antes de avançar** para o próximo
    passo. O commit deve incluir o código, os testes, o ADR e a atualização de
    `docs/STATE.md` relacionados à etapa. Convenção: `tipo(escopo): descrição
    — gate <id> OK`. Nunca acumular duas etapas validadas em um único commit,
    nunca deixar uma etapa validada sem commit. Se uma etapa falhar, **não
    commitar** como se estivesse pronta: registrar o resultado em
    `docs/STATE.md` e commitar apenas esse registro cheio de dados, com
    mensagem `docs(state): resultado <id> — NO-GO`.

---

## 7. F0 — detalhamento (fase atual)

### 7.1 Verificação preliminar já feita (somente-leitura)

| Fato | Evidência | Status |
|---|---|---|
| Smithay estável = **0.7.0** (pub. 2025-06-24, MSRV 1.80.1, edition 2021) | crates.io API | confirmado |
| Smithay 0.7.0 depende de `calloop ^0.14.0` → toolchain mínima **1.86** (calloop 0.14.5 MSRV 1.86) | crates.io API | confirmado |
| `wayland-server ^0.31.9`, `winit ^0.30`, `drm-ffi ^0.9`, `gbm ^0.18`, `input ^0.9`, `libseat ^0.2.3` | deps do smithay 0.7.0 | confirmado |
| greetd IPC = `u32` **native byte order** + JSON UTF-8; requests `create_session`, `post_auth_message_response`, `start_session`, `cancel_session`; responses `success`, `error`, `auth_message` com tipos `visible/secret/info/error` | `greetd-ipc(7)` | confirmado |
| `greetd_ipc` 0.10.3, `pam-client` 0.5.0, `pam` 0.8.0, `rmp-serde` 1.3.1, `zeroize` 1.9.2 | crates.io API | confirmado |
| WPE: Debian bookworm traz 2.36; Ubuntu jammy 2.36.0; WPE ≥2.54 troca `libwpe`+`wpebackend-fdo` por **WPEPlatform**; pacotes `libwpewebkit-2.0-*`, `libwpebackend-fdo-1.0-*`, `libwpe-1.0-*` | packages.debian.org / packages.ubuntu.com / wpewebkit.org | **versão/API exata do alvo ainda não confirmada — objetivo do S2** |

Risco já identificado para o ADR do S2: `wpebackend-fdo` é backend *legado*
que assume conexão Wayland/EGL própria; o caminho "offscreen → dmabuf → textura
GLES do Smithay" depende de qual API de exportação de buffers existe na versão
fixada. **Não presumir.** Se não houver exportação dmabuf sem cópia, parar e
propor alternativa (regra 9).

### 7.2 Spikes

#### S1 — Compositor mínimo (go/no-go: mecanismo)

- **Fazer:** workspace Cargo; `aeroshell-compositor` com `main.rs` (parse de
  args, reservando `--greeter` como stub), `state.rs` mínimo, backend `winit`
  via calloop; registrar `xdg_wm_base`, commitar superfície, renderizar cliente
  de teste com renderer GLES do Smithay. Ler `anvil` **no tag 0.7.0** (não
  master) como referência.
- **Riscos:** API de `DesktopFacade`/`Compositor` mudou entre 0.6 e 0.7;
  winit 0.30 + calloop 0.14 exigem adaptação do event loop.
- **Gate:** sobe, cliente xdg-shell aparece, 30 min sem crash, `cargo check`
  limpo.
- **Fora do escopo:** DRM/udev, multi-monitor, damage — vão para F1.

#### S2 — WebView → dmabuf → textura GLES ⭐ (spike mais arriscado)

- **Fazer:**
  1. Inventário exato nos alvos: `apt-cache policy libwpewebkit-2.0-*
     libwpebackend-fdo-1.0-* libwpe-1.0-*`; `pkg-config --modversion
     wpe-webkit-2.0 wpe-fdo-1.0 wpe-platform-2.0`; presença de
     `wpe-platform-headless-2.0`.
  2. Definir trait `WebSurfaceBackend` **sem Smithay**: entregar
     `DmaBufFrame { fd, format, modifier, width, height, offset, stride, damage }`.
  3. Implementar `backends/wpe.rs` produzindo esse frame; no compositor,
     importar via API de dmabuf do renderer e compor.
  4. Medir.
- **Medições:** 1920×1080 @60 fps; contagem de cópias CPU por frame (**0** —
  instrumentar ponto de import + contador interno); latência commit→presente
  p50/p99/p99.9; **GPU ociosa com página parada** (sem repaints,
  `gpu_busy_percent`/`intel_gpu_top`, alvo <2% em 10 s com damage vazio);
  suporte a modifiers (linear vs. modifiers de driver).
- **Riscos:** WPE 2.54+ muda API; FDO exportable pode exigir Wayland client
  connection; CEF OSR entrega SHM (cópia) — fallback com custo diferente,
  explicitar no ADR.
- **Gate:** 60 fps sustentado, 0 cópias CPU, GPU ociosa em página estática.
  Se falhar: **parar, dados na mão, propor alternativa**.

#### S3 — Input e hit-test por alfa

- **Fazer:** feed de eventos de ponteiro no compositor; para cada posição,
  amostrar canal alfa do último frame da WebView. Comparar: (a) `glReadPixels`
  1×1 via FBO (lazy, cache por frame); (b) máscara de alfa em memória mantida
  pelo lado web. Integrar com o trait do S2 sem vazar Smithay para
  `aeroshell-web`.
- **Medições:** p99 por evento de motion sob 1000 eventos/s; número de
  readbacks por frame; custo opaco vs. transparente (transparente repassa
  input).
- **Riscos:** stall de sincronização GPU→CPU; modificar textura durante import.
- **Gate:** p99 < 1 ms, **zero** readbacks de frame inteiro, pixel transparente
  repassa input.

#### S4 — IPC Rust ↔ Bun

- **Fazer:** `aeroshell-ipc` com framing `u32 LE` + MessagePack (`rmp-serde`);
  lado TS com `msgpackr` em `packages/runtime/src/ipc-client.ts`; harness de
  100k round-trips; medir p50/p99/p99.9 e taxa de perda; testar reconexão e
  mensagens parciais/fragmentadas.
- **Riscos:** custo de serialização no event loop do Bun; mensagens grandes
  (thumbnails) poluindo a métrica — separar bench de controle pequeno vs.
  payload grande.
- **Gate:** p99 < 1 ms para mensagens de controle ≤512 B, 0 perdas, reconexão
  sem duplicar eventos. ADR escolhe framing e codec.

#### S5 — PAM fora da thread principal + greetd

- **Fazer:**
  1. Ler docs/código de `pam-client` 0.5 (`ConversationHandler`) e `pam` 0.8
     (`Conversation`) e decidir qual expõe `prompt_echo_off / prompt_echo_on /
     text_info / error_msg` de forma genérica.
  2. Implementar conversa em **thread/processo dedicado** e medir impacto no
     loop do compositor.
  3. Subir **greetd de teste em VM/container isolado** com pilha PAM fictícia e
     `GREETD_SOCK`, executar handshake completo `create_session → auth_message
     → post_auth_message_response → start_session`.
  4. Verificar que nenhum segredo aparece em logs, `ps`, métricas ou no Bun.
- **⚠️ Regra 8:** antes de qualquer passo que altere `/etc/pam.d` ou config de
  display manager, **parar e pedir confirmação**. Todo o S5 roda dentro da VM,
  sem tocar no host.
- **Medições:** p99 do loop principal durante auth < 5 ms; nenhuma ocorrência do
  token de teste em `journalctl`/stdout; tempo de resposta da conversa.
- **Gate:** handshake completo, loop não bloqueia, zero vazamento de segredo.

### 7.3 Gates go/no-go (resumo)

Cada linha abaixo, quando validada, **deve ser commitada na hora** (regra 10).
Em caso de NO-GO, commitar só o registro com dados em `docs/STATE.md`.

| Spike | Métrica | Gate | Commit ao validar |
|---|---|---|---|
| S1 | compositor sobe, cliente xdg-shell aparece, sem crash 30 min | OK | `feat(compositor): spike S1 mínimo com backend winit — gate S1 OK` |
| S2 | 1920×1080 @60 fps, **0** cópias CPU/frame, GPU <2% com página parada | OK | `feat(web): spike S2 dmabuf→GLES + ADR 0002/0005 — gate S2 OK` |
| S3 | hit-test alfa p99 <1 ms/evento, 0 full-frame readbacks | OK | `feat(input): spike S3 hit-test alfa + ADR 0006 — gate S3 OK` |
| S4 | p99 <1 ms em 100k round-trips (≤512 B), 0 perdas | OK | `feat(ipc): spike S4 msgpack + ADR 0003 — gate S4 OK` |
| S5 | conversa PAM não bloqueia loop (p99 <5 ms), handshake greetd em VM, 0 segredos em logs | OK | `feat(auth): spike S5 PAM/greetd + ADR 0004 — gate S5 OK` |
| F0 | relatório com números + todos os ADRs | OK | `docs(f0): relatório go/no-go e ADRs — F0 OK` |

### 7.4 Artefatos da F0

`docs/progress.md`, `docs/adr/0001-smithay-pin.md`,
`0002-webview-backend.md`, `0003-ipc-codec.md`, `0004-pam-integration.md`,
`0005-dmabuf-gles-path.md`, `0006-alpha-hit-test.md`.

Cada artefato entra no commit da etapa que o produziu; o commit final `docs(f0)`
só consolida o que já foi validado e commitado — ele não substitui os commits
individuais dos spikes.

---

## 8. Esqueleto inicial da F0

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
    "crates/aeroshell-compositor",
    "crates/aeroshell-web",
    "crates/aeroshell-ipc",
    "crates/aeroshell-auth",
]

[workspace.package]
version = "0.0.1"
edition = "2021"
rust-version = "1.86"
license = "MIT OR Apache-2.0"

[workspace.dependencies]
smithay = { version = "=0.7.0", default-features = false, features = [
    "backend_winit", "backend_udev", "backend_gbm", "backend_libinput",
    "backend_session_libseat", "desktop", "renderer_gl", "renderer_multi",
    "wayland_frontend", "xwayland",
] }
calloop = "=0.14.5"
serde = { version = "1", features = ["derive"] }
rmp-serde = "=1.3.1"
tracing = "0.1"
zeroize = "1"
greetd_ipc = "=0.10.3"
pam-client = "=0.5.0"
# demais deps entram conforme cada spike as usa
```

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.86.0"          # piso: calloop 0.14.5; subir para stable vigente no S1
components = ["rustfmt", "clippy"]
profile = "minimal"
```

```json
// package.json (versões exatas fixadas com `bun add` no S4)
{
  "name": "aeroshell-workspace",
  "private": true,
  "workspaces": ["packages/*", "templates/*"],
  "scripts": {
    "check": "tsc --noEmit",
    "test": "bun test",
    "bench:ipc": "bun packages/runtime/bench/ipc.ts"
  },
  "devDependencies": {
    "@types/bun": "latest",
    "typescript": "latest"
  }
}
```

```just
# justfile
set shell := ["bash", "-uc"]

default:
    just --list

check:
    cargo check --workspace --all-targets
    bun test

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

spike-s1:
    cargo run -p aeroshell-compositor -- --backend winit

spike-s2:
    cargo run -p aeroshell-web --example dmabuf-throughput

spike-s3:
    cargo run -p aeroshell-compositor --example alpha-hit-test

spike-s4-server:
    cargo run -p aeroshell-ipc --example bench-server
    # outro terminal: just bench:ipc

spike-s5:
    # SOMENTE na VM isolada — nunca no host
    cargo run -p aeroshell-auth --example pam-greetd-smoke
```

**Pastas/arquivos da F0 (só o necessário):**

```
crates/aeroshell-compositor/{Cargo.toml, src/main.rs, src/state.rs, src/backend/winit.rs, examples/alpha-hit-test.rs}
crates/aeroshell-web/{Cargo.toml, src/lib.rs, src/surface.rs, src/backends/wpe.rs, examples/dmabuf-throughput.rs}
crates/aeroshell-ipc/{Cargo.toml, src/{lib,messages,codec,server,types}.rs, examples/bench-server.rs}
crates/aeroshell-auth/{Cargo.toml, src/{lib,pam,conversation,greetd}.rs, examples/pam-greetd-smoke.rs}
packages/runtime/src/{main.ts, ipc-client.ts}
packages/runtime/bench/ipc.ts
tests/integration/
examples/
protocols/
docs/adr/
docs/progress.md
```

Adiado deliberadamente para F1+: `backend/udev.rs`,
`crates/aeroshell-services`, `crates/aeroshell-apps`, `packages/sdk`, adapters
React/Solid/Vue, templates, packaging. Isso segue a regra 1.

---

## 9. Decisões pendentes (precisam de resposta antes de iniciar)

1. **Aprovação do plano** (ou ajustes de gates/métricas).
2. **Alvos exatos do S2:** Debian `bookworm` ou `trixie`? Ubuntu `22.04` ou
   `24.04`? Define legado `libwpe`/`wpebackend-fdo` vs. WPEPlatform ≥2.54.
3. **Ambiente de teste:** GPU real disponível para medições de 60 fps/ociosidade,
   ou S2 roda em VM com GPU virtual (invalidaria números e exigiria spike
   separado)?
4. **VM isolada para PAM/greetd:** já existe ou deve ser criada no S5?
