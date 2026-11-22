#!/usr/bin/env python3
"""F1d3-diagnóstico: verifica se o log do tty3 contém a evidência do caminho de apresentação.

NÃO mede pixels nem substitui o juiz humano: só prova que sessão, CRTC, queue_frame e
vblank/frame_submitted foram exercitados no backend udev.

Uso:
  python3 tests/integration/udev_frame_probe.py <arquivo-de-log>
  python3 tests/integration/udev_frame_probe.py --self-test   # prova do parser (fixture sintética)
"""

import re
import sys

ANSI = re.compile(r"\x1b\[[0-9;]*m")

PATTERNS = {
    "session": re.compile(r"UDEV_SESSION active=(true|false) seat=(\S+)"),
    "output": re.compile(r"UDEV_OUTPUT crtc=(\S+) active=(true|false) connectors=(\d+)"),
    # O tracing imprime campos string COM aspas (queued="ok"); aceitar as duas formas.
    "frame": re.compile(
        r'UDEV_FRAME seq=(\d+) is_empty=(true|false) needs_sync=(true|false) queued="?(ok|empty|err)"?'
    ),
    "vblank": re.compile(r'UDEV_VBLANK crtc=(\S+) n=(\d+) submitted="?(ok|err)"?'),
    "session_event": re.compile(r"UDEV_SESSION_EVENT event=(\S+) active=(true|false)"),
    "vt": re.compile(r'UDEV_VT key=F(\d+) vt=(\d+) result="?(ok|err)"?'),
}
ERROR_MARKERS = (
    "render_frame falhou",
    "queue_frame falhou",
    "frame_submitted falhou",
    "erro DRM",
)


def parse(raw):
    lines = [ANSI.sub("", line) for line in raw.splitlines()]
    found = {key: [] for key in PATTERNS}
    for line in lines:
        for key, pattern in PATTERNS.items():
            match = pattern.search(line)
            if match:
                found[key].append(match.groups())
    errors = {marker: sum(marker in line for line in lines) for marker in ERROR_MARKERS}
    return found, errors


def verdict(found, errors):
    reasons = []
    active_session = any(active == "true" for active, _ in found["session"])
    activated = any(event == "ActivateSession" and active == "true" for event, active in found["session_event"])
    if not (active_session or activated):
        reasons.append("sem UDEV_SESSION active=true nem UDEV_SESSION_EVENT ActivateSession active=true")

    active_output = any(active == "true" for _, active, _ in found["output"])
    if not active_output:
        reasons.append("sem UDEV_OUTPUT active=true")

    queued = [m for m in found["frame"] if m[1] == "false" and m[3] == "ok"]
    if not queued:
        reasons.append("sem UDEV_FRAME is_empty=false queued=ok")

    submitted = [m for m in found["vblank"] if m[2] == "ok"]
    if not submitted:
        reasons.append("sem UDEV_VBLANK submitted=ok")

    for marker, count in errors.items():
        if count:
            reasons.append(f"{count}× '{marker}' no log")

    return ("OK" if not reasons else "FALHA"), reasons


def report(path, raw, found, result, reasons):
    print(f"log: {path}")
    print(
        "contagens: session={} output={} frame={} (queued=ok={}) vblank={} (submitted=ok={}) "
        "session_event={} vt={}".format(
            len(found["session"]),
            len(found["output"]),
            len(found["frame"]),
            sum(m[3] == "ok" for m in found["frame"]),
            len(found["vblank"]),
            sum(m[2] == "ok" for m in found["vblank"]),
            len(found["session_event"]),
            len(found["vt"]),
        )
    )
    if found["session"]:
        print("  sessao: active=" + found["session"][-1][0])
    if found["output"]:
        print("  output: active=" + found["output"][-1][1])
    if found["frame"]:
        last = found["frame"][-1]
        print(f"  frame final: seq={last[0]} is_empty={last[1]} needs_sync={last[2]} queued={last[3]}")
    if found["vblank"]:
        last = found["vblank"][-1]
        print(f"  vblank final: n={last[1]} submitted={last[2]}")
    if found["vt"]:
        print("  atalhos VT: " + ", ".join(f"F{k}->{v}:{r}" for k, v, r in found["vt"]))
    if reasons:
        print("faltam/erros:")
        for reason in reasons:
            print(f"  - {reason}")
    print(f"VEREDITO: {result}")
    return 0 if result == "OK" else 1


def self_test():
    good = "\n".join(
        [
            "UDEV_SESSION active=true seat=seat0",
            "UDEV_OUTPUT crtc=crtc::Handle(45) active=true connectors=1",
            "UDEV_FRAME seq=1 is_empty=false needs_sync=true queued=\"ok\"",
            "UDEV_VBLANK crtc=crtc::Handle(45) n=1 submitted=\"ok\"",
            "UDEV_SESSION_EVENT event=ActivateSession active=true",
            "UDEV_VT key=F2 vt=2 result=\"ok\"",
        ]
    )
    bad = "FRAMEWEB: pool_ready\nsessão libseat aberta seat=seat0\nudev: loop de render vivo tick=1500\n"
    result_good, reasons_good = verdict(*parse(good))
    result_bad, reasons_bad = verdict(*parse(bad))
    if result_good != "OK" or result_bad != "FALHA":
        print(f"FALHA no self-test: good={result_good} {reasons_good} bad={result_bad} {reasons_bad}")
        return 1
    print("SELF-TEST: OK (parser aceita evidência completa e rejeita log sem evidência)")
    print("AVISO: fixture é sintética; não é evidência de execução do backend udev.")
    return 0


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--self-test":
        return self_test()
    if len(sys.argv) != 2:
        print("Uso: udev_frame_probe.py <log> | --self-test", file=sys.stderr)
        return 2
    path = sys.argv[1]
    try:
        with open(path, "rb") as handle:
            raw = handle.read().decode("utf-8", errors="replace")
    except OSError as err:
        print(f"FALHA: não consegui ler {path}: {err}")
        return 2
    found, errors = parse(raw)
    result, reasons = verdict(found, errors)
    return report(path, raw, found, result, reasons)


if __name__ == "__main__":
    sys.exit(main())
