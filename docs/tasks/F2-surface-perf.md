# Tarefa F2-surface-perf — responsividade e compartilhamento de WebProcess

> Só medição. NENHUMA correção neste cartão. Visual do F2-surface-host APROVADO; cartão anterior
> continua NÃO fechado até esta mediía terminar.

## Objetivo (uma frase, verificável)
Produzir dados crus que confirmem ou refutem os invariantes de WebProcess compartilhado, cena estática quase idle e latência de input estável em superfícies web do AeroShell.

## Contexto mínimo
- `docs/adr/0001-go-no-go.md` — related-view/pool: nº de WebProcesses não deve crescer com nº de superfícies.
- `crates/aeroshell-shell/examples/frame_webview.rs` — pool/related-view/âncora dos frames.
- `crates/aeroshell-shell/src/surfaces/` — host genérico (desktop/panel/overlay).
- `tests/integration/f2_surface_host.py` — criação de superfícies e input sintético.
- `docs/STATE.md` §6/§7 — veredito visual aprovado com responsividade reprovada e 9 WebProcesses.

## Passos (só medição)
1. A/B: instância só com `foot` (sem desktop/overlay/panel) vs instância com superfícies; mesma VM e backend. Registrar CPU do compositor e nº de WebProcesses.
2. Contagem de WebProcesses por kind de superfície + verificação de `related_view` da âncora para TODOS os kinds; logar pid/processo por superfície. Invariante: nº de WebProcesses NÃO cresce com nº de superfícies/janelas.
3. Build COM `bench`: redraws/s, área de damage e commits/s por superfície com cena estática (2 janelas, sem input). Invariante: compositor quase idle com damage vazio.
4. Latência input→resposta: eco de tecla no `foot` e clique→efeito, p50/p99, antes/depois dos 20–30 s. Invariante: p99 não degrada com o tempo.
5. Cold-start: estágios da tela preta (superfícies + frames) com o harness de stages estendido.

## Limiares e escopo
- Invariantes arquiteturais acima valem NA VM.
- fps/60 Hz e GPU ociosa <2% só em GPU real (regra 4): registrar como NÃO medido aqui.
- NENHUMA correção; se invariante falhar, apenas entregar dados e aguardar cartão dirigido.
- Sem push. `STATE.md` atualizado com a tabela crua. Instância atual é fechada pelo humano.
