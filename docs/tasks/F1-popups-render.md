# Tarefa F1-popups-render — subcartão de `F1-gap-protocolos.md` (RASCUNHO — aguarda "ok")

> Um agente deve executar lendo só AGENTS.md, STATE.md, RUNBOOK.md,
> `docs/tasks/F1-gap-protocolos.md` e este arquivo. Uma tarefa por sessão; se o objetivo mudar, PARE e pergunte.
> Aberto com autorização do humano em 2026-11-22, depois de cursor-shape e primary-selection fechados.

## Objetivo (uma frase, verificável)
Popups xdg de clientes reais recebem o grab explícito, abrem sobre a janela/painel e são dispensados
corretamente, sem erros de protocolo e sem regredir frame/painel/conteúdo.

## Contexto mínimo — premissas CONFIRMADAS na fonte fixada (Smithay 0.7.0)
**Correção ao mapa original da F1 (registrada em `STATE.md §7`):** a renderização de popups **já existe**.
- `desktop/space/wayland/window.rs:106-121` — `AsRenderElements for Window` percorre
  `PopupManager::popups_for_surface(surface)` e inclui os elementos do popup **antes** da superfície da
  janela, com offset `(geometry().loc + popup_offset - popup.geometry().loc)`.
- `desktop/space/wayland/layer.rs:31-48` — o mesmo para `LayerSurface` (cobre popups do painel).
- Nosso `render/scene.rs:60-114` já chama `AsRenderElements` para layers e janelas ⇒ **não há gaps de
  render a preencher aqui**. Mexer na cena só com medição que prove o contrário (regra 2 do cartão-mãe).

O que falta de fato:
- `desktop/wayland/popup/manager.rs:54-69` — `PopupManager::grab_popup(root, popup, seat, serial)`;
  exige `D: SeatHandler + 'static`, `KeyboardFocus: WaylandFocus + From<PopupKind>` e
  `PointerFocus: From<KeyboardFocus> + WaylandFocus`. Nosso `KeyboardFocus = WlSurface` satisfaz:
  `desktop/wayland/popup/mod.rs:36` implementa `From<PopupKind> for WlSurface`.
- `manager.rs:112-128` — o grab trata `ParentDismissed` e `NotTheTopmostPopup` (erros de protocolo) e
  guarda o grab no `seat.user_data()`; `manager.rs:205` `cleanup()` já é chamado nos 3 backends.
- `protocols/xdg_shell.rs:117` — `fn grab(&mut self, …)` do `XdgShellHandler` é **no-op** (a lacuna real).
- `xdg_shell.rs:85-102` já faz `track_popup`/`unconstrain_popup`/`reposition_request`;
  `xdg_shell.rs:149-178` já faz `popups.commit` + configure inicial.
- `manager.rs:177` — `popups_for_surface` é estático; é o caminho pelo qual o Smithay já renderiza.

## Escopo (mínimo)
1. Implementar `grab()` em `protocols/xdg_shell.rs`: resolver o root via `find_popup_root_surface`,
   confirmar que a superfície com foco de teclado é esse root, chamar `self.popups.grab_popup(root.clone(),
   popup, seat, serial)`; tratar erro sem `unwrap`/`allow`.
2. Instrumentação sob `cfg(feature = "bench")`: `POPUP_NEW`, `POPUP_COMMIT`, `POPUP_GRAB`,
   `POPUP_DISMISS` (serial/window_id/geometry/root), sem log em produção.
3. Harness de teste com cliente real (popup xdg de verdade), escrito ANTES da mudança e visto falhando.
   **Cliente preferido: GNOME Terminal** (determinação do humano em 2026-11-22) — o `foot` **não** tem menu
   de botão direito; o terminal do GNOME tem. O menu de contexto do GNOME Terminal usa `xdg_popup` via GTK.
   **Fato medido nesta VM:** `command -v gnome-terminal` → vazio (binário AUSENTE). Instalar pacote do
   sistema exige aprovação (AGENTS.md §3.8/§5) e não foi autorizado. Portanto o teste-first automatizado
   nesta VM usa um **cliente `wayland-client` mínimo** (deps já no workspace: `wayland-client` +
   `wayland-protocols`) que cria um `xdg_popup` de verdade; o GNOME Terminal fica como teste manual do
   humano no tty3, se ele existir no host dele.
   O `AERO_TEST_INPUT_SOCK` de hoje só injeta botão esquerdo (`input.rs:72-94`, `0x110`); para abrir o menu
   é preciso um comando de **botão direito** (`0x111`) no harness, **adicionado sob `cfg(feature = "bench")`
   para não mudar o binário de produção**. Se o GNOME Terminal não abrir o popup neste nested (dependência
   de D-Bus/GTK), registrar a limitação com saída crua e cair para um cliente `wayland-client` mínimo que
   crie `xdg_popup` — sem mudar o escopo do cartão.
4. Só mexer em `render/scene.rs`/`unconstrain_popup` se a medição com cliente real provar necessidade —
   caso contrário, registrar como **não tocado**.

## Critério de aceite (comandos exatos + saída esperada)
1. Teste-first: com o **GNOME Terminal** conectado ao nested/headless e o menu de contexto aberto por
   botão direito sintético (hook bench), o harness mostra o popup mapeado e **sem grab** no baseline
   (ou erro/dismiss incorreto); depois da mudança, `POPUP_GRAB` presente e `POPUP_DISMISS` quando o cliente
   fecha/pede dismiss. Saída crua colada no relatório.
2. Nenhum `xdg_wm_base`/`xdg_popup` protocol error no log do compositor nem no `stderr` do cliente.
3. Visual/geometry: o popup aparece **acima** da janela e do painel, na posição do positioner; se houver
   critério visual, humano decide (AGENTS.md §0.9) — o agente não encerra aplicação.
4. `bash scripts/check.sh` verde; clippy default/bench `-D warnings` EXIT=0;
   `cargo check --workspace --no-default-features` EXIT=0; harnesses de frame do RUNBOOK seguem OK.
5. Commit pequeno por etapa validada (regra 10); sem push.

## Como usar o domicile — guia, NÃO receita
Determinação do humano (2026-11-22): invariants apenas — quem detém o grab/apresentação responde pelo
estado, e diagnósticos nomeados para "por que este popup não abriu/fechou". **Não copiar** Chromium/Ozone,
`cc::SurfaceLayer`, prior art do fork. Fonte executável: Smithay 0.7.0 fixado, citado acima.

## Fora de escopo
- Reescrever a cena de render (a menos que medição prove necessidade).
- Lock/PAM/greeter, services/apps, mudança de protocolo XML, ADR, dependência nova, sudo/VT/push.
- Os outros avisos do `foot` e os demais subcartões do cartão-mãe.

## Se encontrar OUTRO problema
Anotar 1–3 linhas em `STATE.md §6` (sintoma, onde, hipótese) e VOLTAR; no máximo 15 minutos de desvio.

## Limites
- Tempo: 2 h sem popup funcional no cliente real → PARAR e reportar com dados, sem terceira via por conta própria.
- Dependências: nenhuma prevista.

## Entrega
Relatório curto: o que funciona · como testar à mão · o que NÃO foi testado · gate colado · descobertas
laterais. Ao final: `STATE.md §3/§7/§9`, commit local e PARAR — não abrir o próximo cartão sem ok.
