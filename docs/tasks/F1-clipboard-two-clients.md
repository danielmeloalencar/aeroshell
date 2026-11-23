# Tarefa F1-clipboard-two-clients — clipboard entre 2 clientes (RASCUNHO — aguarda "ok")

> Cartão 2 da sequência F1-final aprovada (ordem 1→5). Abrir só depois do cartão 1 fechado pelo juiz.
> Decisão do humano: clientes sintéticos A→B aprovados; `wl-copy` timeout é limitação de instrumento, não falha do produto.

## Objetivo (uma frase, verificável)
O compositor transfere dados de seleção (`data-device`) entre DOIS clientes reais — A escreve, B lê — no headless e no
nested, com hamess versionado e gate verde.

## Contexto mínimo (arquivos que importam)
- `crates/aeroshell-compositor/src/state.rs:208,336` — `DataDeviceState` + global já existem.
- `crates/aeroshell-compositor/src/protocols/seat.rs:81,125-141` — `set_data_device_focus`/`delegate_data_device!`;
  o foco acompanha o teclado (mesmo padrão de `set_primary_focus`, F1-primary-selection).
- `docs/STATE.md §3/§7` — primary-selection FECHADO pelo humano; `wl-copy|wl-paste` deram timeout sem e com foco
  sintético (`/tmp/opencode/f1-primary/clipboard{,-focus}.sh`), registrado como lacuna do instrumento.
- Smithay 0.7.0 fixado: `~/.cargo/registry/src/.../smithay-0.7.0/src/wayland/selection/data_device/` — confirmar
  `DataDeviceHandler`, `Selection` e `set_data_device_focus` antes de editar.

## Critério de aceite (comandos exatos + resultado esperado)
1. Teste-first: `examples/clipboard_client.rs` (modo `--offer`/`--receive`) + `tests/integration/f1_clipboard_two_clients.py`
   escritos antes; baseline `VEREDITO: FALHA` (B não recebe o texto).
2. Headless: A oferece `text/plain;charset=utf-8` com texto único; B recebe exatamente o mesmo bytes; foco de teclado
   de A→B via `AERO_TEST_INPUT_SOCK`; 0 erros de protocolo.
3. Nested: o mesmo round-trip com dois clientes conectados ao socket do compositor.
4. Vizinhos: `f1i_protocols.py`, `headless_render_damage.py`, `frame_buttons.py`, `frame_window_id.py` → OK.
5. Gate: `bash scripts/check.sh` verde; clippy `--features bench -D warnings` EXIT=0; `--no-default-features` EXIT=0.

## Fora de escopo
- Primary-selection (já fechada), drag-and-drop, xdg-activation, streaming de eventos (F2), protocolos/ADR/deps novos.
- Reabrir a lacuna do `wl-copy` como se fosse regressão do produto.

## Limites
- Tempo: 2 h sem round-trip funcional → PARAR e reportar. Dependências: nenhuma.

## Entrega
Relatório curto + STATE §3/§7/§9 + commit local. PARAR antes do cartão 3.
