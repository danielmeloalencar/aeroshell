#!/usr/bin/env python3
"""F1d3-diagnóstico: mede se o cliente (foot) está de fato composto no backend udev.

NÃO julga a imagem. Só lê as sondas `FOOT_COMMIT` e `UDEV_FRAME_CB` (sob `bench`)
e distingue dois cenários:

  CALLBACK_NAO_CHEGA      - poucos commits e/ou `pending` nunca volta a zero:
                            o cliente parou de desenhar porque não recebe o
                            `done()` do frame callback.
  SEM_DAMAGE_FORA_DA_CENA - commits contínuos, `pending` circula (volta a zero)
                            e o output segue is_empty=true: o elemento existe
                            mas não chega ao scanout/cena.

A tendência do `pending` é o discriminador: no headless que funciona, mediu-se
0,1,1,0,1,1,0 (7 commits em 0,76 s) — ou seja, os callbacks circulam.

Uso:
  python3 tests/integration/udev_client_compose_probe.py <arquivo-de-log>
  python3 tests/integration/udev_client_compose_probe.py --min-span 0 <log>  # validação local
"""

import re
import sys

COMMIT = re.compile(r"FOOT_COMMIT t=(\d+) surface=(\S+) pending=(\d+)")
CALLBACK = re.compile(r"UDEV_FRAME_CB t=(\d+) surface=(\S+) pending=(\d+)")
MIN_SPAN_NS = 10_000_000_000  # o pedido: medir ao longo de 10 s


def parse(raw):
    commits = []
    callbacks = []
    for line in raw.splitlines():
        m = COMMIT.search(line)
        if m:
            commits.append((int(m.group(1)), m.group(2), int(m.group(3))))
        m = CALLBACK.search(line)
        if m:
            callbacks.append((int(m.group(1)), m.group(2), int(m.group(3))))
    return commits, callbacks


def analyze(commits, callbacks, min_span_ns):
    reasons = []
    if not commits:
        reasons.append("sem FOOT_COMMIT (sonda de commit do cliente ausente)")
    if not callbacks:
        reasons.append("sem UDEV_FRAME_CB (sonda de frame-callback pendente ausente)")

    span = 0
    if len(callbacks) >= 2:
        span = callbacks[-1][0] - callbacks[0][0]
    elif callbacks:
        reasons.append("UDEV_FRAME_CB aparece uma única vez (medição < 10 s)")
    if 0 < span < min_span_ns:
        reasons.append(f"span de UDEV_FRAME_CB = {span / 1e9:.2f} s < {min_span_ns / 1e9:.0f} s")

    if reasons:
        return "FALHA", reasons, span, "INDETERMINADO"

    pendings = [p for _, _, p in commits]
    seen_positive = False
    zeroed_after_positive = False
    for pending in pendings:
        if pending > 0:
            seen_positive = True
        elif seen_positive:
            zeroed_after_positive = True

    if len(commits) <= 2:
        diagnosis = "CALLBACK_NAO_CHEGA"
    elif seen_positive and not zeroed_after_positive:
        diagnosis = "PENDENTE_ACUMULANDO"
    elif zeroed_after_positive or max(pendings) == 0:
        diagnosis = "SEM_DAMAGE_FORA_DA_CENA"
    else:
        diagnosis = "INDETERMINADO"
    return "OK", [], span, diagnosis


def main():
    args = sys.argv[1:]
    min_span_ns = MIN_SPAN_NS
    if args and args[0] == "--min-span":
        if len(args) != 3:
            print("Uso: --min-span SEGUNDOS <log>", file=sys.stderr)
            return 2
        try:
            min_span_ns = int(float(args[1]) * 1e9)
        except ValueError:
            print("--min-span exige número", file=sys.stderr)
            return 2
        args = args[2:]
    if len(args) != 1:
        print("Uso: udev_client_compose_probe.py [--min-span SEGUNDOS] <log>", file=sys.stderr)
        return 2
    path = args[0]
    try:
        with open(path, "rb") as handle:
            raw = handle.read().decode("utf-8", errors="replace")
    except OSError as err:
        print(f"FALHA: não consegui ler {path}: {err}")
        return 2

    commits, callbacks = parse(raw)
    result, reasons, span, diagnosis = analyze(commits, callbacks, min_span_ns)
    all_pending = [p for _, _, p in commits] + [p for _, _, p in callbacks]
    print(f"log: {path}")
    print(
        f"contagens: foot_commits={len(commits)} frame_cb_samples={len(callbacks)} "
        f"span={span / 1e9:.2f}s pending_max={max(all_pending) if all_pending else 0}"
    )
    if commits:
        print(f"  1o commit: t={commits[0][0]} pending={commits[0][2]}")
        print(f"  ult. commit: t={commits[-1][0]} pending={commits[-1][2]}")
        print("  pending_seq: " + ",".join(str(p) for _, _, p in commits[:16]))
    if callbacks:
        print(f"  1a amostra CB: t={callbacks[0][0]} pending={callbacks[0][2]}")
        print(f"  ult. amostra CB: t={callbacks[-1][0]} pending={callbacks[-1][2]}")
    if reasons:
        print("faltam:")
        for reason in reasons:
            print(f"  - {reason}")
    print(f"diagnostico: {diagnosis}")
    print(f"VEREDITO: {result}")
    return 0 if result == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
