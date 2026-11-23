# Tarefa F1-subsurfaces — wl_subsurface de cliente real (APROVADA em 2026-11-22)

> Cartão 1 da sequência F1-final aprovada pelo humano. Um cartão por sessão; PARE para ok visual antes da F2.
> Guia: `domicile-engine` para invariantes de hit-test/geometria, **não** receita. Fonte executável: Smithay 0.7.0 fixado.

## Objetivo (uma frase, verificável)
O compositor compõe e faz hit-test de `wl_subsurface` de clientes reais (menu/tooltip de GTK/Qt) acima da janela pai,
provado por cliente sintético no headless/nested e pelo juiz visual, sem regressão de geometria/frame.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/protocols/compositor.rs:204-216` — `is_sync_subsurface`/`get_parent` e commit propagado ao root.
- `crates/aeroshell-compositor/src/state.rs:457-550` — `surface_under` já usa `WindowSurfaceType::ALL` em layers e janelas; é o caminho de input a exercitar.
- `crates/aeroshell-compositor/src/render/scene.rs:60-114` — render pela árvore de superfícies do Smithay.
- Smithay 0.7.0 fixado: `~/.cargo/registry/src/.../smithay-0.7.0/src/wayland/compositor/mod.rs` e
  `src/desktop/space/wayland/window.rs:106-121` — confirmar APIs/antes de editar; NÃO inventar API.
- Gap analysis (`docs/STATE.md §3`) marcou subsurfaces como ✅ **com ressalva**: nunca houve teste dedicado de cliente.

## Critério de aceite (comandos exatos + resultado esperado)
1. Cliente `wl_subsurface` real (uma superfície **synced** e uma **desynced**, com `set_position`) no headless:
   `python3 tests/integration/f1_subsurfaces.py` → `VEREDITO: OK`; pixels do filho aparecem no offset esperado acima do pai
   (sonda de dump sob `bench`), 0 erros de protocolo.
2. Input: clique dentro do filho é entregue ao filho via `surface_under(...ALL)`, não ao pai — provado por contagem de
   eventos no cliente (`POINTER_ENTER/BUTTON` no filho, 0 no pai).
3. Nested com app real (gedit ou app Qt): menu/tooltip aparece acima da janela e responde a clique. Se o app não abrir
   nesta VM (ex.: nautilus/Tracker3, já registrado em §6), registrar a limitação e pedir o juiz no tty3.
4. Vizinhos: `headless_render_damage.py`, `frame_buttons.py`, `frame_window_id.py`, `frame_resize_glue.py`,
   `frame_fixed_canvas_hygiene.py` → OK (nenhuma amostra descartada; falha intermitente registrar e repetir uma vez diagnóstica).
5. Gate (uma execução cada, sem reexecutar "até passar"): `bash scripts/check.sh` → `OK: scripts/check.sh passou`;
   `cargo clippy --workspace --all-targets --features bench -- -D warnings` → EXIT=0;
   `cargo check --workspace --no-default-features` → EXIT=0.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- `xdg_popup`/grab (F1-popups-render, FECHADO pelo juiz), viewporter/escala, dmabuf, presentation-time.
- `protocols/*.xml`, ADR aceito, dependência nova, sudo/VT/push, avanço de fase.
- Reabrir geometria/9-slice/escala aprovados ou qualquer cartão 02*/F1* fechado.

## Teste primeiro (obrigatório)
`examples/subsurface_client.rs` + `tests/integration/f1_subsurfaces.py` escritos **antes** de qualquer correção;
mostrar a saída crua do baseline com `VEREDITO: FALHA`. Se o mecanismo JÁ funcionar sem código novo (ressalva da gap
analysis), registrar com honestidade que o cartão é de **evidência**, não de correção — não inventar bug.
Linhas de sonda, se necessárias, SOMENTE sob `cfg(feature = "bench")`, com `strings` do binário sem bench = 0.

## Se encontrar OUTRO problema
Anotar 1 a 3 linhas em `docs/STATE.md §6` (sintoma, onde, hipótese) e VOLTAR. Máx. 15 min de desvio; regra das duas
tentativas: se a mesma abordagem falhar 2×, PARE e peça decisão.

## Limites
- Tempo: 2 h sem cliente funcional → PARAR e reportar com dados.
- Dependências: nenhuma (`wayland-client` já é dev-dep do compositor).
- Arquivos protegidos: ver `AGENTS.md §6`.

## Entrega (relatório curto)
O que funciona · como testar à mão (comandos exatos) · o que quebrou/não foi testado · descobertas laterais (já anotadas).
Ao final: `STATE.md §3/§7/§9` + commit local (sem push). **PARAR para o juiz visual antes da F2.**
