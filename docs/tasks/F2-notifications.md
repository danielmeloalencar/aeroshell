# Tarefa F2-notifications — notificações nativas do shell

> Cartão de tarefa: um agente deve conseguir executá-lo lendo só AGENTS.md, STATE.md, RUNBOOK.md e este arquivo.
> Uma tarefa por sessão. Se o objetivo mudar no meio, PARE e pergunte.

## Objetivo (uma frase, verificável)
Exibir e dispensar notificações internas do shell por uma API tipada do SDK Bun, por eventos no stream e por toasts no painel React, incluindo o comando IPC `notify <title> <body>`.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-ipc/src/types.rs` — tipos públicos e eventos gerados para TS por ts-rs.
- `crates/aeroshell-compositor/src/window_ipc.rs` — comandos IPC e stream `AeroEvent` existentes.
- `packages/sdk/src/` — API pública, transporte IPC, cache reativo e eventos.
- `packages/panel/src/index.tsx` — painel React e UI existente de workspaces/launcher/janelas.
- `crates/aeroshell-shell/src/bridge.rs` — entrega dos eventos do stream ao WebView do painel.
- `tests/integration/f2_launcher.py` e `f2_workspaces_panel.py` — padrões e vizinhos obrigatórios de painel/stream.
- ADRs relevantes: nenhum ADR aceito será alterado.

## Mecanismos
1. **SDK Bun:** `showNotification(title: string, body: string, icon?: string, timeoutMs?: number): string`, `dismissNotification(id: string): void` e `onNotification(callback: (n: Notification) => void)`; manter o estado reativo das notificações ativas recebidas pelo stream.
2. **Painel React:** toasts no canto inferior direito, acima do painel inferior (alternativa de layout permitida no pedido), com título, corpo, ícone fornecido ou padrão e botão “X”; auto-dismiss após `timeoutMs` (padrão 5000 ms).
3. **Compositor:** aceitar `notify <title> <body>` pelo IPC interno e publicar a notificação no stream existente, sem D-Bus nem protocolo Wayland novo.
4. **Fase:** o humano autorizou explicitamente antecipar à F2 somente este subconjunto interno de notificações; o serviço D-Bus completo permanece na F4.

## Critério de aceite (comandos exatos + resultado esperado)
1. **Baseline test-first:** `python3 tests/integration/f2_notifications.py` → `VEREDITO: FALHA` antes da implementação, por ausência da API/stream/UI; depois → `VEREDITO: OK`, cobrindo SDK com IPC real, `notify`, toast visível, timeout padrão, timeout de 10000 ms e fechamento manual.
2. `bun test packages/sdk` → testes novos da API, estado, eventos, timeout e dismiss passam; cada teste novo é visto falhando antes da implementação.
3. `python3 tests/integration/f2_workspaces.py && python3 tests/integration/f2_workspaces_panel.py && python3 tests/integration/f2_launcher.py` → todos `VEREDITO: OK`.
4. Vizinhos de frame/CSD/policy/hot-reload relevantes continuam `VEREDITO: OK`; qualquer falha é preservada e comparada com o HEAD antes de atribuir regressão.
5. `bash scripts/check.sh` → `OK: scripts/check.sh passou`; `cargo clippy --workspace --all-targets --features bench -- -D warnings` → exit 0; `cargo check --workspace --no-default-features` → exit 0.
6. **Teste manual (juiz humano):** `showNotification("Teste", "Corpo da mensagem")` → toast legível no canto inferior direito, acima do painel; esperar 5 s → desaparece; notificação com `timeoutMs=10000` → permanece após 5 s; clicar em “X” → desaparece imediatamente; `notify` pelo IPC → toast aparece. O humano deve responder explicitamente **“ok/aprovado”**; a aplicação fica aberta para ele fechar.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Notificações com ações ou botões personalizados.
- Persistência/histórico entre reinicializações.
- Integração com D-Bus `org.freedesktop.Notifications`.
- Snap/F7/F1.
- Mudanças em protocolos Wayland/XML, ADRs ou arquitetura.
- Novas dependências externas.

## Se você encontrar OUTRO problema
Anote em `docs/STATE.md` §6 (1 a 3 linhas: sintoma, onde, hipótese) e VOLTE à tarefa.
Não investigue desvios por mais de 15 minutos. Só pare tudo se o problema bloquear o critério de aceite.

## Limites
- Tempo: 2 h sem resultado visível → pare e descreva o bloqueio.
- Dependências novas: não.
- Arquivos protegidos: ver AGENTS.md §6; nenhuma alteração prevista.
- Sem push; commits locais pequenos somente após validação e confirmação exigida pelo harness.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: atualizar STATE.md (§3, §4, §5, §6, §7, §9), obter o veredito visual e comitar localmente (sem push).
