# Tarefa 02q — cold-start da decoração

> FECHADA pelo juiz humano em 2026-11-21: "Visualmente está perfeito e bem otimizado". A verdade visual prevalece sobre o cronômetro divergente; F0 fechada e F1 autorizada, sem iniciar F1 automaticamente.

## Objetivo (uma frase, verificável)
Ao abrir uma janela, a decoração sólida aparece no primeiro render e a decoração HTML real chega em até aproximadamente 300 ms, sem o intervalo visual atual de 1–2 s.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/backend/winit.rs` — composição do frame; desenhar placeholder sólido na geometria alvo até o primeiro buffer.
- `crates/aeroshell-compositor/src/protocols/aero_frame.rs` — estado de associação e chegada do primeiro buffer do frame.
- `crates/aeroshell-shell/examples/frame_webview.rs` — criação das WebViews, view-âncora oculta, `related_view` e pool pré-aquecido.
- `docs/adr/0001-go-no-go.md` — decisão já tomada: related-view + âncora sempre viva + pool pequeno.
- Causa informada no pedido: WebView sem `related_view` cria processo por janela; `load_html` é assíncrono; não há pool nem placeholder. Confirmar no código e reproduzir antes de corrigir.

## Implementação delimitada
1. Compositor: enquanto o frame não tiver buffer, desenhar titlebar/borda sólidas (`SolidColorRenderElement`/`single_pixel_buffer`) na geometria alvo; trocar pelo frame real no primeiro buffer.
2. Shell: manter uma WebView-âncora oculta viva e criar cada frame com `.related_view(&anchor)`, compartilhando o processo.
3. Shell: manter pool de 2–4 WebViews pré-aquecidas, com HTML carregado (`LoadEvent::Finished`); `frame_requested` consome uma pronta, realiza/apresenta a view e o placeholder cobre até o paint real; reposição assíncrona. O pré-paint offscreen foi descartado após 2 falhas e esta adaptação foi autorizada pelo humano.
4. Escrever primeiro a medição integrada e mostrá-la falhando no código atual; não concluir desempenho em VM sem 3D.

## Estado da execução (2026-11-21)
- Implementado: placeholder sólido; âncora oculta + related-view (3 → 1 WebKitWebProcess); pool de 2 views
  com HTML carregado, consumo por request e reposição assíncrona.
- Baseline: 469,320/543,026 ms, sem placeholders e 3 processos (`02q_cold_baseline2.log`). Via A:
  374,387/415,202 ms, placeholders e 1 processo (`02q_pool_loaded_reverted.log`).
- Pré-paint offscreen falhou 2× (double-rAF/snapshot); via A foi autorizada. Pré-realize e queue_draw também
  foram revertidos após falhas. Todas as amostras estão em `docs/STATE.md` §6.
- Vizinhos finais initial_buffer/fixed_canvas/repeated_activate/buttons/window_id → 5× OK; gate e clippy bench OK.
- Juiz humano aprovou a instância manual e depois decidiu: "O que prevalece é minha verdade. Visualmente está
  perfeito e bem otimizado". Tarefa/F0 FECHADAS; a divergência do cronômetro fica registrada, sem editar o teste.

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste primeiro: criar `tests/integration/frame_cold_start.py`, executar `python3 tests/integration/frame_cold_start.py` no baseline e preservar a saída crua com `VEREDITO: FALHA`, o tempo janela-mapeia → primeiro buffer real do frame (esperado no baseline: cerca de 1–2 s) e a ausência de placeholder no primeiro render.
2. Depois da correção: repetir `python3 tests/integration/frame_cold_start.py` 3 vezes, sem descartar amostras; cada execução deve terminar com `VEREDITO: OK`, comprovar placeholder sólido no primeiro render, primeiro buffer real do frame em `<= 300 ms` após o map e consumo/reposição do pool com âncora e related-view ativos. **Resultado:** placeholder/pool/related-view passam; o cronômetro continua >300 ms e o humano decidiu explicitamente que sua verdade visual prevalece, fechando a tarefa sem alterar o teste.
3. Vizinhos: `python3 tests/integration/frame_initial_buffer.py`, `python3 tests/integration/frame_fixed_canvas_hygiene.py` e `python3 tests/integration/frame_repeated_activate.py` → todos `OK`.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou` em uma única execução completa.
5. Juiz visual obrigatório: executar o nested pelo RUNBOOK, deixá-lo aberto, abrir 2–3 `foot` e pedir confirmação explícita de que a decoração aparece imediatamente, sem gap visual; quem fecha a aplicação é o humano. **Resultado: APROVADO — "ok. aprovado" e "visualmente perfeito e bem otimizado"; aplicação encerrada pelo humano.**

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Não alterar resize, 9-slice, canvas fixo, grabs ou qualquer comportamento aprovado na 02p.
- Não mudar protocolo, ADR aceito, arquitetura ou dependências.
- Não medir/concluir FPS, PSS ou desempenho em GPU real; esta VM sem 3D valida funcionamento e a métrica relativa deste cartão apenas.
- Não reabrir 02c/02e/02f/02n/02o nem alterar a remoção pré-existente de `docs/prior-art/domicile.md`.

## Se você encontrar OUTRO problema
Anotar em `docs/STATE.md` §6 (sintoma, onde, hipótese) e voltar à tarefa. Investigar desvio somente se bloquear o aceite, por no máximo 15 minutos.

## Limites
- Tempo: 2 h sem resultado visível → parar e descrever o bloqueio.
- Regra das duas tentativas: após duas falhas da mesma abordagem, parar e pedir decisão.
- Dependências novas: nenhuma.
- Arquivos protegidos: nenhum; ver `AGENTS.md` §6.
- APIs de Smithay/GTK/WebKitGTK: confirmar na versão fixada antes de usar e citar a fonte.

## Entrega (relatório curto)
Resultado · teste manual exato · confirmação visual · provas cruas antes/depois · não executado/não medido · descobertas laterais · próximo passo proposto sem executá-lo.
Ao final: atualizar `docs/STATE.md` (§3, §4, §5, §6, §7, §9), rodar o gate completo e comitar localmente somente com autorização, sem push.
