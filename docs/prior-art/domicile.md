# Prior art: Domicile (referência conceitual, NÃO receita)

> Fonte: https://github.com/cprussin/domicile · Licença: MIT (arquivos Chromium
> modificados mantêm as licenças do Chromium: BSD-3-Clause / LGPL-2.0-or-later).
> Este doc é um resumo com trechos curtos citados — não é código vendored.
>
> **Atribuição (correção registrada no STATE):** cprussin (Connor Prussin) é
> **contribuidor** do Smithay, não o criador. O criador do Smithay é
> **Victor Berger (elinorbgr)**. Não repetir a atribuição errada.
>
> **Como usar:** leia antes de tocar no resize do frame. É contexto para validar
> invariantes. A referência de CÓDIGO para implementar é **Smithay 0.7.0 + anvil**
> (já em `~/.cargo/registry/...`), não o domicile.

## 1. O que é o domicile (em 1 parágrafo)

Desktop Linux experimental onde um **Chromium forkado É o compositor Wayland final
(viz)**. O Rust/Smithay é apenas o host Wayland + *producer* viz. Janelas nativas são
elementos DOM `<app>` (= `cc::SurfaceLayer`); sites são `<webview>`. O desktop inteiro
é uma página. Toda a chrome (painéis, **decorações**, overlays) é conteúdo web dessa
página — composta pelo mesmo viz, no mesmo display frame.

```
wayland client ──dmabuf──▶ domicile-compositor (Smithay, viz producer) ──CompositorFrame──▶ viz
a página: <app> ──SurfaceLayer(SurfaceId)──▶ cc layer tree ──▶ viz ──▶ display
```

- O browser importa o dmabuf do cliente como `SharedImage`; o compositor submete um
  `CompositorFrame` com `TextureDrawQuad`; `<app>` embute essa surface; viz agrega.
  **Zero cópia de CPU; nenhum frame cruza socket.**
- **A página não vê os pixels da janela; o compositor não vê o layout da página.**
  O layout box do `<app>` vira o `xdg_toplevel.configure` do cliente.

## 2. Mecanismos relevantes para o resize do AeroShell

### 2.1 Uma cena só, um compositor final dono da geometria
Decoração e conteúdo são compostados juntos → nunca desacolam. A geometria de
apresentação é do compositor final (viz), não de um cliente separado.

### 2.2 Surface-sync com token de frame + deadline (o "cola" do resize/move)
> "Viz holds the page's frame until frame n activates. Until the page embeds it, viz
> draws the newest active surface not newer than the embedded id
> (`FindLatestActiveSurfaceInRange`)." — `WINDOW-COMPOSITING.md`

- `<app frame="n">` embute o frame n; o número ride no canal de controle (mesmo render).
- **"Resizes compose. A resize allocates `(p+1, 1)`; the producer and the page both
  apply n to the allocated id."**
- Há um **deadline** com fallback para o último surface ativo ≤ id embutido.

### 2.3 Conteúdo do cliente NUNCA é esticado num ack-only
> "Not remapped: a commit that only acks a new configure. The window keeps its old
> frame at its old box rather than stretching it." — `WINDOW-COMPOSITING.md`

Commits sem buffer (viewport/scale/transform/geometry) remapeiam o buffer retido
(`EngineSession::remap`); um ack puro mantém o frame velho na box velha.

### 2.4 Diagnósticos nomeados (modos de falha explícitos)
Changesets que nomeiam falhas, em vez de só logar geometria:
- `a-page-whose-resize-is-never-acked-says-so`
- `a-page-that-never-draws-at-its-new-size-says-why`
- `a-pages-compositor-thread-says-why-it-sent-nothing`
- `a-lost-page-flip-does-not-freeze-the-desk`
- `adaptive-sync-frame-pacing`, `client-title-bar-drags`

## 3. Mapeamento para o AeroShell (Smithay + WebKitGTK-cliente)

| Domicile (viz) | Equivalente AeroShell | Tarefa |
|---|---|---|
| viz é o compositor final | Smithay é o compositor final | — |
| chrome é DOM na mesma cena (síncrona) | frame é cliente WebKit separado (assíncrono) → **9-slice** + **canvas fixo** | 02k/02l/02p |
| token `frame=n` + deadline + fallback | serial de configure + **sinal `frame_painted(serial)`** (não heurística de tamanho) | 02p |
| "old frame at old box, sem esticar" no ack | **conteúdo nunca estica**; textura do frame 9-sliced; buffer do canvas apresentado até pronto | 02k/02p |
| diagnósticos nomeados | `frame_diagnostics.rs`: `FRAME_NEVER_ACKED` / `FRAME_NEVER_DREW_AT_SIZE` / `FRAME_STRETCH_ACTIVE` | 02j |
| move via synced frames | move **já correto** (geometria no Rust, um pass) — **não mexer** | — |

**Lição central transferível:** o compositor final deve ser dono da geometria de
apresentação e **transformar o último buffer bom até o novo estar pronto** — com um
**sinal explícito de prontidão**, não igualdade de tamanho nem timers.

## 4. O que NÃO copiar (explícito)

- **Fork do Chromium / viz / `CompositorFrame` / `SharedImage`**: APIs específicas do
  Chromium, não portáveis para Smithay. Custo: rebases + build de ~4 h; série de patches.
- **CEF OSR**: não eliminaria o invariante (compositor dono da geometria + sinal de
  pronto) e contradiz a premissa de **WebView nativa do sistema**.
- **Electron host, browser profiles, extensões, Miracast, portais, Nix**: fora do escopo.
- **Ritmo de desenvolvimento**: domicile é mantido com modelo frontier (Claude) + humano
  nas decisões de arquitetura; não é replicável com modelo local. Não usar como benchmark
  de velocidade.

## 5. Lições já aplicadas no AeroShell (rastro)

- **02j**: diagnósticos nomeados (eco do §2.4).
- **02k/02l**: 9-slice com insets + zonas protegidas da titlebar (`set_titlebar_slices`).
- **02m**: zero configures durante o grab (buffer congelado no tamanho assentado).
- **02p (raiz)**: canvas de tamanho FIXO + 9-slice sempre + repaint in-place +
  `frame_painted`. Causa-raiz provada: `from_surface` re-lê o buffer CURRENT (sem
  pinning); `ack_configure` imediato (antes do paint); redimensionar a janela GTK a cada
  interação gera buffers "tamanho novo / layout velho". O canvas fixo elimina a classe
  de transientes por construção — o equivalente AeroShell do §2.1–2.3.

## 6. Para consultar além deste doc (clone externo, NÃO commitado)

```bash
git clone --depth 1 --filter=blob:none --sparse \
  https://github.com/cprussin/domicile /tmp/domicile-ref
cd /tmp/domicile-ref && git sparse-checkout set docs/architecture
```

Docs-chave: `WINDOW-COMPOSITING.md`, `ARCHITECTURE.md`, `ONE-PAGE-FOR-THE-DESK.md`,
`ENGINE-FORK.md`, `STACKING-PARITY.md`.