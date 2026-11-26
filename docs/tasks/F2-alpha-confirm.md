# Tarefa F2-alpha-confirm — input por alfa

> Cartão fornecido e autorizado pelo humano em 2026-11-26. Base: `f913cd8`
> (F2-windows-completo fechado). Uma tarefa por sessão; PARAR para o juiz visual.
> FECHADO em 2026-11-26: baseline reproduzida, implementação 1×1 + cache, gates/vizinhos,
> 3 perfs e 3 execuções release verdes. Juiz visual: **“Agora sim funcionou”**.

## Objetivo (uma frase, verificável)
Confirmar ou implementar hit-test por alfa: pixel transparente DENTRO de input region declarada
NÃO-vazia repassa input aos clientes; região restringe e alfa decide dentro dela.

## Contexto mínimo (arquivos que importam)
- `docs/plano-mestre.md` §3 e S3 — contrato de input e gate p99/readbacks.
- `crates/aeroshell-compositor/src/state.rs` e `src/input.rs` — hit-test e entrega de input.
- `crates/aeroshell-compositor/src/render/` e `src/backend/` — buffers/texturas das WebViews.
- `crates/aeroshell-shell/src/surfaces/` — desktop/panel/overlay/frame e input regions.
- `tests/integration/` — harness test-first e vizinhos.
- Referência do pedido: ADR 0006. Divergência constatada: o ADR 0006 versionado trata de resize;
  o contrato técnico deste cartão é o plano §3/S3, sem editar ADR aceito.

## Critério de aceite (comandos exatos + resultado esperado)
1. Discriminador primeiro: baseline do harness com widget opaco + área transparente dentro da
   região FALHA se o transparente não repassa; depois OK.
2. Se hoje só a região decide, implementar readback 1×1 lazy + cache por frame OU máscara web-side.
   ZERO readback de frame inteiro. Ordem: região restringe; alfa decide dentro dela.
3. Cobrir desktop/panel/overlay/frame, caps do 9-slice e overlay fullscreen.
4. Medição S3 sob ~1000 eventos de motion/s: p99 por motion <1 ms; contagem de readbacks e
   0 full-frame readbacks.
5. Gates: `bash scripts/check.sh`; `cargo clippy --workspace --all-targets --features bench --
   -D warnings`; `cargo check --workspace --no-default-features`; vizinhos frame/policy/hotreload/
   workspaces/launcher/host/sdk/hmr/outputs-binds/windows + `f2_surface_perf.py`.
6. Juiz visual: clique transparente chega à janela, opaco captura; 60 s responsivo; aplicação
   deixada aberta e aprovação explícita do humano.

## Fora de escopo (NÃO faça, mesmo que pareça relacionado)
- Adapters React/Solid/Vue e exemplos blank/win11-like/Three.js.
- Layouts (F3), idle/lock (F5), thumbnail/screencopy (F7), snap.
- Otimização de move, hotplug e outras dívidas não bloqueantes.

## Limites
- Sem dependências novas sem aprovação; sem alterar protocolos ou ADRs aceitos.
- Sem push. Atualizar `docs/STATE.md` ao final; commits locais somente com autorização.
- Regra das duas tentativas e teste-first obrigatórios.

## Entrega (relatório curto)
Baseline e depois com o mesmo comando · p99/readbacks crus · gates/vizinhos · o que não foi testado ·
confirmação visual pendente/OK · próximo passo proposto, não executado.

## Depois (somente com novo cartão e ok)
Adapters React/Solid/Vue + exemplos (blank/win11-like/Three.js) → fechamento F2.
