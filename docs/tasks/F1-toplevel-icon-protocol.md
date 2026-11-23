# Tarefa F1-toplevel-icon-protocol — ProtocolError do xdg_toplevel_icon_v1 desconecta Kate/Kwrite (APROVADA 2026-11-23)

> Plano aprovado pelo humano em 2026-11-23, com escopo e ordem fixos. Bloqueia o juiz visual do F1-final-1c:
> `tests/integration/f1_client_visible.py` → `frame_painted=1 protocol_errors=1 IPC='.' titulos_reais=0 VEREDITO: FALHA`.
> O "Janela" visto na tela é o placeholder do HTML (`frame_webview.rs:864`), não título real.

## Objetivo (uma frase verificável)
Fazer `f1_client_visible.py` dar `VEREDITO: OK` (sem ProtocolError, janela na IPC list, título real), com o global
`xdg_toplevel_icon_manager_v1` anunciado e checagens intactas.

## Ordem obrigatória (aprovada)
1. **A/B diagnóstico:** NÃO anunciar o global e rodar `f1_client_visible.py` no Kate/Kwrite. Reportar log cru.
   Se aparecer outro ProtocolError ou outro bloqueio, RELATAR antes de seguir.
2. **WAYLAND_DEBUG=1 no kwrite:** reportar a ordem exata de `create_buffer`, `add_buffer`, `set_icon`, `destroy`
   (do buffer e do icon).
3. **Leitura:** `xdg_toplevel_icon.rs:360-369` — dizer se o código é do Smithay 0.7.0 ou nosso; se for do Smithay,
   verificar se há correção upstream.
4. **Correção (causa provada):** copiar pixels em vez de reter o `wl_buffer`; emitir `no_buffer` só se o buffer
   morreu antes de ser lido. **Não desligar checagens.**
5. **Teste de regressão sintético:** `add_buffer`, `set_icon`, `destroy` do buffer. Só reanunciar o global depois.

## Critério de aceite
- `python3 tests/integration/f1_client_visible.py` → `VEREDITO: OK` (os três FALHA viram PASSA).
- Global `xdg_toplevel_icon_manager_v1` anunciado no final (não pode ficar suprimido).
- Gate `bash scripts/check.sh` + `cargo clippy --workspace --all-targets --features bench -- -D warnings`.
- Juiz visual do F1-final-1c com Kate/Kwrite (nested e tty3) — aplicação deixada aberta.

## Fora de escopo
Mudança de versão do Smithay (pedir nova autorização), CSD, ADR 0007, cartão 2/clipboard, F2.

## Regras
- Uma mudança por vez; se não produzir o log esperado, REVERTA antes da próxima.
- Commits separados: diagnóstico / correção / teste. **Commit da medição (`402bc16`) já existe e fica separado.**
- Nenhuma amostra descartada; nenhuma checagem desligada; sem push.
