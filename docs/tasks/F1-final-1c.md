# Tarefa F1-final-1c — diagnóstico: frame WebView não pinta com Kate/Kwrite no udev (APROVADA 2026-11-23)

> Cartão de diagnóstico aberto com dados crus dos logs `~/aeroshell-udev-20261123-005015.log` (kate) e
> `-005123.log` (kwrite). Um cartão por sessão; PARE para ok visual antes do cartão 2 (clipboard).

## Objetivo (uma frase verificável)
Identificar com dado cru por que o frame WebView não pinta (`0 frame_painted`, `FRAME_BUFFER_COMMIT buffer=None`)
quando Kate/Kwrite rodam no udev, aplicar a correção mínima e fechar com juiz visual.

## Contexto (dados já medidos nos logs)
- Ambos conectam, mapeiam toplevel e recebem `INITIAL_CONFIGURE 800x600`; frame associado
  (`frame_requested reapresentado`, `associate(window_id=0) ... client_frame=false`).
- `FRAMEWEB: canvas inicial window_id=0 1932x991` e `configure serial=1 conteudo=1920x955 target=1932x991` —
  o canvas JÁ cobre o conteúdo que o app pediu.
- 0 `frame_painted` e único `FRAME_BUFFER_COMMIT buffer=None expected=None ok=false` nos dois logs.
- `IPC de janelas ativo path=…`, mas `painel React command="list" lista=` vazio sempre.
- kwrite: `The Wayland connection broke` após `ICON_SET`, sem protocol error no compositor.
- Nenhuma linha de portal/GApplication/`already associated` nos logs do compositor.

## Hipóteses ranqueadas (investigar nesta ordem)
1. CSD vs SSD: o app pede decoração? `CSD_DECO`/`CSD_DETECT`? O frame é associado? (dados parciais já coletados)
2. Canvas inicial (ADR 0007): tamanho pedido vs canvas (dado já coletado: 1920x955 → 1932x991, cobre).
3. `frame_requested` não enviado/recebido (dado já coletado: enviado e recebido).
4. Específico do udev: o frame pinta no nested? Se sim, bug do backend; se não, bug do compositor/shell.

## Abordagem
1. Confirmar 1–3 nos logs crus existentes (sem nova execução).
2. Rodar Kate no **nested** com `RUST_LOG=debug` + binário `bench` e comparar `frame_painted`, `FRAME_BUFFER_COMMIT`,
   `xdp`/decoração com o controle `foot`.
3. Se divergir só no udev: instrumentar o caminho do frame no udev (`FSCALE_FRAME`, `UDEV_FRAME`, damage).
4. Cliente sintético (CSD + tamanho grande) se necessário, escrito ANTES da correção.

## Critério de aceite
1. Causa com `arquivo:linha` ou linha de log crua citada, hipóteses descartadas registradas.
2. Correção mínima, uma coisa por vez, teste-first visto falhando.
3. Regressão: foot/gedit/qterminal e `f1_subsurfaces.py`, `f1_popup_click.py` → OK.
4. Gate: `check.sh`, clippy bench, `--no-default-features`.
5. Juiz visual: Kate abre com decoração visível no tty3.

## Fora de escopo
Portal/xdg-desktop-portal, ADR 0007 (salvo prova), protocolos além de xdg-decoration/xdg_toplevel, cartão 2, F2.

## Limites
- Tempo: 2 h sem causa → PARAR e reportar. Nenhuma amostra descartada; sondas só sob `bench`.

## Entrega
Relatório curto + STATE §3/§6/§7/§9 + commit local. PARAR no ok visual.
