# Tarefa F1-csd-sem-flash — esconder a moldura CSD sem atraso visível

> **FECHADO PELO JUIZ VISUAL em 2026-11-22.** Humano respondeu **"aprovado"** após observar a Calculadora na nested
> deixada aberta (e/ou tty3): a moldura do AeroShell não aparece mais nem por um instante. Nenhuma instância foi
> encerrada pelo agente.

> Provas: baseline com poll de 1 s → `VEREDITO: FALHA` em `≤200ms` (`/tmp/opencode/f1-flash/baseline-final.log`, sem
> `csd_applied` dentro da janela; amostra anterior com detecção → aplicação em 829 ms). Com o timer de 100 ms: 3×
> `VEREDITO: OK` com `hide_ms` 71,3 / 7,0 / 104,1 (`final-run{1,2,3}.log`). Gate verde. Falta o juiz visual no tty3.

> Melhoria opcional pedida pelo humano ao fechar `F1-csd-gtk-move-resize` (2026-11-22). Um cartão por sessão.

## Objetivo (uma frase, verificável)
Quando um cliente GTK4 expõe a `window_geometry` com sombra, nossa moldura HTML desaparece em **≤ 200 ms** após esse
sinal, em vez dos ~0,5 s atuais.

## Contexto mínimo
- `crates/aeroshell-shell/examples/frame_webview.rs` — `refresh_titles()` roda a cada **1 s** e hoje faz título, ícone
  e CSD na mesma passada; o CSD tardio do GTK4 só é visto no poll seguinte.
- `window_ipc.rs` — comando `csd <id>` já devolve `1` por `window.geometry().loc != 0` ou pedido KDE/XDG; sonda bench
  `CSD_GEOM` já existe.
- `tests/integration/f1_csd_gtk.py --geometry-only` — cliente define a geometria com atraso de 2 s (imitando GTK4);
  hoje só verifica que a moldura foi escondida, sem medir **quando**.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Teste-first:** estender o harness `--geometry-only` para medir o intervalo entre `CSD_CLIENT geometry_set` e
   `FRAMEWEB: csd_applied ... bar=hidden border=hidden` (timestamps `t=` dos logs). Baseline atual: `VEREDITO: FALHA`
   com atraso > 200 ms (amostra crua colada).
2. Implementar uma checagem de CSD separada e rápida (timer ~100 ms), ativa só enquanto houver janela ainda sem
   decisão CSD estável; parar quando todas as entries já foram classificadas. extrair a lógica de `refresh_titles()`
   para `refresh_csd()` e chamar nos dois caminhos, sem duplicar log.
3. Depois: 3× `VEREDITO: OK` com atraso medido ≤ 200 ms. Nenhuma regressão em `frame_title`, `f1_toplevel_icon`,
   `frame_buttons`, `f1_kde_decoration`.
4. `bash scripts/check.sh` verde numa única execução; rodar `cargo build -p aeroshell-compositor --features bench`
   **depois** do gate (o `check.sh` reconstrói o bin sem as sondas).
5. Juiz visual: no tty3, abrir a Calculadora e confirmar que a moldura não "pisca" perceptivelmente. Aplicação deixada
   aberta, humano julga (AGENTS.md §0.9); sem "ok", cartão fica em §7.

## Fora de escopo (NÃO faça)
- `protocols/*.xml`, ADR, dependências novas, heurística por `app_id`, mudar a cadência de título/ícone.
- Reabrir o cartão CSD/GTK já fechado; Nautilus/Tracker (spike D-Bus separado).

## Limites
- Tempo: ~1 h. Nenhuma dependência nova; arquivos protegidos intocados.

## Entrega
O que funciona · atraso medido (3 amostras) · o que NÃO foi testado · gate colado · STATE atualizado · commit local (sem push).
