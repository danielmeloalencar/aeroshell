# Tarefa 02d — Diagnosticar a defasagem da decoração antes de decidir a arquitetura

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.
> **RASCUNHO — aguarda o "ok" do humano para executar.**

## Objetivo (uma frase, verificável)
Produzir evidência suficiente para decidir entre adotar o Plano B e redefinir o critério de 02c: medir a defasagem
conteúdo↔decoração em várias cadências de motion, com 3 repetições por cadência, e obter o juízo visual do humano
no arrasto real — sem mudar a arquitetura nesta tarefa.

## Contexto mínimo (arquivos que importam)
- 02c ficou PARCIAL no commit `91354de`: `send_frame_configure` passou a ser chamado no `ResizeGrab::motion`
  (`grab.rs`), o que reduziu o lag p95 de 177,22 ms para 127,19 ms, mas a pendência permaneceu em `13/14`
  (`python3 tests/integration/frame_resize_sync.py` → `VEREDITO: FALHA`).
- `tests/integration/frame_resize_sync.py` mede com `STEPS=14`, `STEP_PX=10`, `MOTION_DELAY=0.05` (50 ms por
  passo) — uma única cadência, uma execução por variante. É a lacuna principal.
- O `foot` arredonda o tamanho pedido para múltiplos de 6 (`want=706` → conteúdo real `702`); a métrica já foi
  corrigida para usar o tamanho REAL commitado pelo cliente. Não confundir com descompasso visual.
- Timer de despacho do shell de 20 → 1 ms foi tentado em 02c, não ajudou (p95 158,05 ms) e foi revertido.
- Com a decoração em WebView, o commit do frame leva ~60–100 ms após o configure. O Plano B (9-slice no
  compositor / buffer sobredimensionado + src-rect) tiraria o WebKit do caminho crítico, mas exige ADR NOVO e
  mudança de arquitetura — fora de escopo aqui (ADR 0001, `AGENTS.md §5`).
- `docs/STATE.md` §4 e §6 registram a decisão do humano (2026-11-20): "Manter parcial, cartão novo".

## Critério de aceite (comandos exatos + resultado esperado)
1. Parametrizar `tests/integration/frame_resize_sync.py` para aceitar `--delay-ms <ms>` (padrão 50, sem quebrar
   a chamada atual do cartão 02c). Rodar `--delay-ms` com **16, 33, 50, 100 e 150**, **3 execuções seguidas cada**
   (15 execuções ao total), imprimindo uma tabela com `cadência | p95_ms | pendentes | amostras`. Nenhuma amostra
   descartada; se alguma execução falhar por infraestrutura, registrar em `STATE.md §6` e repetir só a causa
   corrigida, sem apagar a falha.
2. Teste manual (juiz humano): executar o nested do RUNBOOK (`docs/RUNBOOK.md §21`) com 2 `foot`, arrastar a borda
   direita em **dois ritmos** — lento (pausas curtas) e contínuo/rápido — e responder: "a barra/borda fica colada
   ao conteúdo ou corre atrás?". Deixar a aplicação aberta; o humano dá o "ok" e fecha (`AGENTS.md §0.9`).
3. Escrever em `docs/STATE.md §6` a recomendação final com os números medidos e o veredito visual do humano,
   indicando explicitamente se o Plano B é necessário ou se o critério atual é suficiente. NÃO implementar a
   recomendação nesta tarefa.
4. `bash scripts/check.sh` → `OK: scripts/check.sh passou` (EXIT=0, gate completo numa única execução).

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Não implementar o Plano B, não criar/editar ADR aceito, não alterar `protocols/*.xml`, `Cargo.lock` ou
  `rust-toolchain.toml` (`AGENTS.md §6`).
- Não alterar o critério de aceite do cartão 02c — ele permanece FALHO e registrado como tal.
- Não reabrir 02b (window_id, foco, hit-test) nem maximizar/origem do output.
- Não trocar WebKitGTK/gtk4-layer-shell por outra estratégia sem ADR aprovado.
- Não medir desempenho em GPU real: ambiente é VM sem 3D.
- Não desligar verificação para esconder erro (`--release`, `#[allow]`, comentar teste).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado → pare e descreva o bloqueio. Se a mesma abordagem falhar 2 vezes, PARE
  (regra das duas tentativas, `AGENTS.md §4.1`).
- Dependências novas: nenhuma sem aprovação (`AGENTS.md §3.10`).
- Arquivos protegidos: ver `AGENTS.md §6` — NENHUMA mudança sem aprovação explícita.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar `docs/STATE.md` (§3, §4, §5, §6, §9) e comitar localmente (sem push).
