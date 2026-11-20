# Tarefa 05 — Compatibilidade com protocolos modernos (RASCUNHO — aguarda "ok")

> Cartão de tarefa: um agente deve conseguir executá-la lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
O compositor AeroShell anuncia e implementa os protocolos Wayland que clientes modernos (ex.: `foot`) esperam, eliminando os avisos de incompatibilidade observados na tela.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/state.rs` — `rfb.new_global`/registry onde os globals são anunciados.
- `crates/aeroshell-compositor/Cargo.toml` — features do Smithay 0.7.0 fixado (versão a conferir antes de propor API).
- `docs/STATE.md §6` — registro literal das 7 mensagens relatadas pelo humano em 2026-11-20.
- Regra `AGENTS.md §3.6`: NÃO inventar API; ler a doc/code do Smithay 0.7.0 fixado antes de cada implementação.

## Requisitos relatados (mensagem exata do `foot` → provável protocolo → CONFIRMAR)
1. `compositor does not implement the primary selection interface` → wp-primary-selection-v1?
2. `compositor does not implement xdg activation` → xdg-activation-v1
3. `bell.urgent will fall back to coloring the window margins red` → urgência/atenção de toplevel
4. `compositor does not implement fractional scaling` → wp-fractional-scale-v1
5. `compositor does not implement server-side cursors, falling back to client-side cursors` → wp-cursor-shape-v1 (ou equivalente)
6. `compositor does not implement the xdg toplevel icon protocol` → xdg-toplevel-icon-v1
7. `text interface not implemented compositor, IME will be disabled` → zwp_text_input_v3 + input-method

**IMPORTANTE:** os nomes acima são hipóteses a confirmar na versão fixada. O critério é a ausência das mensagens no stdout/stderr do `foot`, não a presença de um nome específico.

## Critério de aceite (comandos exatos + resultado esperado)
1. Levantamento: para cada item, citar `arquivo:linha` do Smithay 0.7.0 fixado que implementa (ou não) o protocolo, e dizer se a feature está ativada.
2. Dividir em subcarts entregáveis (cada um com teste de cliente real): (a) activation + urgency, (b) cursor + fractional scale, (c) primary selection + text/IME + icon. A ordem exige decisão do humano.
3. Teste de regressão: `foot` conectado ao nested **não** imprime nenhuma das 7 mensagens; output cru colado no relatório.
4. `bash scripts/check.sh` verde em cada subcartão.
5. Validação visual (se aplicável) com o humano como juiz final.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Implementar qualquer um dos 7 protocolos nesta sessão ( este é um rascunho).
- Mexer em 04b (painel React), 02c/02d (resize), lock/greeter.
- Ativar features do Smithay sem antes citar a fonte e obter aprovação (AGENTS.md §5: dependência/arquitetura).

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md §6` (1 a 3 linhas) e VOLTE à tarefa.

## Limites
- Tempo: levantamento em até 2 h; cada subcart ≤ 2 h.
- Dependências novas: provavelmente nenhuma (Smithay já tem features para vários); se houver, listar nome/versão/licença/motivo/alternativa e aguardar aprovação.
- Arquivos protegidos: `protocols/*.xml` e ADRs só com aprovação explícita.

## Entrega (relatório curto)
O que funciona · como testar à mão · o que quebrou/não foi testado · descobertas laterais.
Ao final: atualizar STATE.md e comitar localmente (sem push).
