# Tarefa F1-toplevel-icon — ícone real na titlebar

> **FECHADO PELO JUIZ VISUAL em 2026-11-22.** Humano, na instância `wayland-aero-iconvisual2-1795391487`:
> *"vi o icone perfeitamente"*. Ele também relatou `foot`/`zenity` cortados, handles em lugar diferente e que
> "arrastar não funcionava, nem nada" — sintomas de geometria/input **fora de escopo**, medidos à parte em
> `STATE.md §6/§7` (o mesmo `frame_follows_resize` falha no commit pré-ícone; hover sem clique não moveu a janela no
> teste controlado). **Reteste limpo da mesma instância: humano confirmou que ícone, mover, resize, maximizar e
> fechar funcionam perfeitamente — sintomas não reproduzidos, cartão novo NÃO aberto.** Nenhuma instância foi
> encerrada pelo agente.
> **Descoberta durante a execução:** o global é `xdg_toplevel_icon_manager_v1` (não `xdg_toplevel_icon_v1`), e o
> `foot` envia **nome** (`set_name` → `name=foot`), não buffers. O compositor expõe pixels ou nome; o shell resolve
> o nome pelo `gtk4::IconTheme` (guia: `domicile-engine/docs/SHELL-CONFIG.md:388-393`, sem copiar código).

> **Decisão do humano (2026-11-22):** cartão **único** com as duas etapas (protocolo/IPC + renderização), em vez de
> dois cartões. Uma tarefa por sessão; se o objetivo mudar, PARE e pergunte.

## Objetivo (uma frase, verificável)
O compositor anuncia `xdg_toplevel_icon_v1`, recebe o ícone enviado por `foot`/`gedit` e o expõe ao shell, que passa
a mostrar o ícone ao lado do título real na barra do frame — sem mudar `protocols/*.xml`, ADRs ou dependências.

## Contexto mínimo (arquivos e fontes confirmadas)
### Etapa 1 — protocolo/IPC
- `~/.cargo/registry/src/.../smithay-0.7.0/src/wayland/xdg_toplevel_icon.rs`:
  `XdgToplevelIconManager::new::<D>(&dh)` (`:211`), `add_icon_size` (`:222`), `XdgToplevelIconHandler::set_icon` (`:40`),
  `ToplevelIconCachedState::icon_name()/buffers()` (`:76-92`), `delegate_xdg_toplevel_icon!` (`:390`). O módulo vem de
  `wayland_frontend`, já ativo — **nenhuma dependência nova**.
- `wayland/shm/mod.rs:241` `with_buffer_contents` → ponteiro + `BufferData` (`stride`, `format`, `:454-464`). Copiar
  para `Vec` exige bloco `unsafe` com `// SAFETY:` explícito (AGENTS.md §3.7), sem manter a referência viva.
- `strings /usr/bin/foot` contém `add_buffer`, `xdg_toplevel_icon_v1`, `xdg_toplevel_icon_manager_v1` e **não** contém
  `set_icon_name`: o foot envia pixels, não nome. O cliente de teste deve exercitar `add_buffer`.
- `crates/aeroshell-compositor/src/window_ipc.rs:30-41,78-125`: padrão do IPC. **NÃO alterar o formato de `list`** —
  `packages/sdk/src/index.ts:12-25` faz `JSON.parse` do restante da linha. Novo comando: `icon <id>`.

### Etapa 2 — renderização
- `crates/aeroshell-shell/examples/frame_webview.rs`: `TITLEBAR_HTML` (`<span id="title">janela</span>`),
  `window.aeroSetTitle`, `refresh_titles()` — padrão de polling + `evaluate_javascript` do cartão de título real.
- `glib::base64_decode` (`glib-0.22.11/src/auto/functions.rs:20`) e `gdk-pixbuf` (já transitivo via gtk4) para montar a
  imagem em memória. **Sem dependência nova**; se a conversão exigir crate nova, PARE e peça aprovação (AGENTS.md §3.10).
- Layout: ícone dentro da zona de texto de 120 px já reservada (`TITLE_TEXT_W`), sem mexer em insets/slices.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first (ambas as etapas):** cliente de teste (example, `wayland-client` + bindings reexportados) falha se
   `xdg_toplevel_icon_v1` estiver ausente. Baseline: `VEREDITO: FALHA`, `frame_webview` sem `<img>` preenchido e o
   `foot` imprime `compositor does not implement the XDG toplevel icon protocol`. Amostra crua colada.
2. **Protocolo:** cliente cria toplevel + `xdg_toplevel_icon_v1`, `add_buffer` com shm ARGB8888 16×16 de cor conhecida
   (ex.: vermelho) e `set_icon` no toplevel. Com `--features bench`, logar `ICON_SET window_id=<id> w=16 h=16 scale=1`
   (sonda ausente do binário sem bench).
3. **IPC:** comando `icon <id>` responde `ICON <id> <w> <h> <scale> <formato> <base64-dos-bytes>` ou `NONE`.
   Base64 sem dependência nova (helper pequeno no compositor); `list` permanece idêntico. Teste unitário/de protocolo
   cobre o parser de `icon`.
4. **Render:** o shell busca `icon <id>` no mesmo polling do título (≤ 1 s), decodifica, gera `data:` URI e aplica via
   `evaluate_javascript`; log `FRAMEWEB: titlebar_icon window_id=<id> w=16 h=16 source=protocolo` (ou
   `source=fallback` quando `NONE`). 3× `VEREDITO: OK` do harness, amostras falhas preservadas.
5. **Textual:** o `foot` não imprime mais `compositor does not implement the XDG toplevel icon protocol`.
   `Hangup` e outros avisos fora de escopo podem permanecer e devem ser listados.
6. **Regressão:** `frame_title.py`, `frame_window_id`, `frame_buttons`, `frame_resize_glue`,
   `headless_render_damage`, `sdk_cli.py` e `bash scripts/check.sh` verdes numa única execução pós-fix.
7. **Juiz visual obrigatório:** com `foot` e `gedit` reais, o ícone aparece na barra ao lado do título. Aplicação
   deixada aberta; quem fecha é o humano (AGENTS.md §0.9). Sem "ok", fica em `STATE.md §7` e o cartão NÃO fecha.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- `protocols/*.xml` (o protocolo é upstream, reexportado), ADRs, dependências novas, mudança no formato de `list`.
- Geometria/9-slice, insets/slices, o título (já feito) e o aviso de `Hangup`.
- Baixar ícones da internet, escrever arquivos no host, animação de ícone, temas completos ou DPI alto (vários
  sizes/scale) — se necessário, cartão separado.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1–3 linhas: sintoma, onde, hipótese) e VOLTE. Máx. 15 min de desvio.

## Limites
- Tempo: cartão único maior que o recomendado por decisão do humano; se não houver critério verificável em ~2 h,
  PARE e relate o bloqueio (não improvise terceira abordagem).
- Nenhuma dependência nova; arquivos protegidos (AGENTS.md §6) intocados.

## Entrega
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · gate colado · STATE atualizado
(§3/§6/§7/§9) · commit local (sem push). Só fechar com o "ok" visual.
