# ADR 0005 — Supervisor interno ao compositor e ciclo de vida dos filhos

- Status: **aceito pelo usuário em 2026-11-18** (decisões: `rustix` 1.1.6; rota única, migrar testes)
- Data: 2026-11-18
- Relação: complementa o ADR 0004 (o `associate` continua exigindo a ponta privilegiada).
  Não edita ADRs aceitos. Escopo: **shell apenas** — lock/greeter permanecem nas Fases 5/6.

## Contexto

Até aqui o caminho privilegiado era montado **fora** do compositor, pelos harnesses Python:
um `socket.socketpair()` externo, a ponta do compositor passada por `AERO_PRIVILEGED_FD`
(env) e a ponta do shell por `WAYLAND_SOCKET`. Isso era uma rota de teste, não de produção:

- O privilégio nascia de um env (`AERO_PRIVILEGED_FD`) que o compositor lia no startup —
  exatamente o tipo de decisão por env que o AGENTS.md §3.5 proíbe para privilégio de cliente.
- Não havia garantia de ciclo de vida: se o compositor morresse (SIGKILL/crash), o shell e os
  clientes lançados podiam sobreviver como órfãos. Para uma sessão fail-secure isso é inaceitável.
- Duas rotas para a mesma função (externa nos testes, inexistente em produção) violam a lição
  do AGENTS.md §10 ("manter duas rotas para a mesma funcionalidade").

## Decisão

1. **Supervisor dentro do compositor** (`crates/aeroshell-compositor/src/supervisor.rs`).
   O próprio compositor cria o `UnixStream::pair()`, insere a ponta servidor com
   `insert_client(..., ClientState { is_shell: true, .. })` e entrega a ponta do filho ao shell
   via `WAYLAND_SOCKET` (nome reservado às bibliotecas cliente). O privilégio é decidido
   **pela ponta do socket**, nunca por PID/env (AGENTS.md §3.5). O socket público nunca é
   privilegiado; `aero_frame_manager_v1` só é visível a `is_shell=true`
   (`protocols/aero_frame.rs:112-117`).
2. **CLI:** `--shell CAMINHO`, `--shell-arg ARG` (repetível), `--shell-env CHAVE=VALOR`
   (repetível). O env do filho é isolado: `WAYLAND_DISPLAY` = socket do AeroShell,
   `GDK_BACKEND=wayland`, `DISPLAY` e `AERO_PRIVILEGED_FD` removidos.
3. **Ciclo de vida fail-secure via `PDEATHSIG`.** No `pre_exec` de cada filho:
   guarda contra a corrida fork/morte-do-pai (`getppid() != parent_pid` → `Err`, o std dá
   `_exit` no filho) e `PR_SET_PDEATHSIG(SIGKILL)`. Se o compositor morre abruptamente, o
   **kernel** mata os filhos — garantia explícita, não o comportamento incidental do GDK.
   A ponta do socketpair tem CLOEXEC limpo **só no filho privilegiado** (`fcntl_setfd`), para
   sobreviver ao exec; clientes comuns nunca a herdam.
4. **Todos os filhos são rastreados**, inclusive o cliente público `--command`: recebe
   `configure_lifecycle` (PDEATHSIG, sem privilégio) e é lançado por `Supervisor::spawn`.
   Shutdown gracioso: SIGTERM → polling `try_wait` (50×10 ms) → SIGKILL → `wait`/reap.
5. **`rustix` 1.1.6** para `set_parent_process_death_signal`, `getppid`, `kill_process`,
   `fcntl_setfd`. Licença: `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`. Motivo:
   wrappers async-signal-safe sem `libc` manual. Alternativa rejeitada: FFI `libc` direta
   (mais `unsafe` artesanal). Já estava no `Cargo.lock`; aprovado pelo usuário nesta data.
6. **Rota única:** `AERO_PRIVILEGED_FD` removido do compositor. Harnesses Python migrados para
   `--shell`/`--shell-env`/`--shell-arg`; os logs dos filhos saem no stdout do compositor.

## Consequências

- `supervisor.rs` novo; `main.rs` lança `--command` e `--shell` pelo supervisor e chama
  `supervisor.shutdown()` ao sair. `Cargo.toml` (workspace + compositor) ganham `rustix`.
- Harnesses migrados: `frame_spike_real.py`, `frame_spike_two.py`, `supervisor_real.py`,
  `launch_manual.py`, `run_frame_bench.py`. Novo `supervisor_lifecycle.py` prova:
  (1) shell vê o manager / cliente público não vê; (2) kill -9 no compositor mata o shell;
  (3) kill -9 no compositor mata o cliente `--command`.
- `supervisor_real.py` passa a compilar o shell com `--features probe-registry` (o probe do
  registry não está no binário padrão), tornando o teste autocontido.
- O `pre_exec` é `unsafe` com comentário `// SAFETY:` (só syscalls async-signal-safe; caminho
  de erro sem alocação). Nenhum `unwrap`/`allow` novo no caminho de produção.
- Medições de desempenho do bench seguem **relativas à VM sem 3D**; a latência do ponteiro
  não estabilizou (variação < 2% em 3 leituras) e foi registrada como "não medida".