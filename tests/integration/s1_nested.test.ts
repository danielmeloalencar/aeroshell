import { expect, test } from "bun:test";

test("S1: socket próprio e isolamento do WAYLAND_DISPLAY do host", async () => {
  const hostDisplay = process.env.WAYLAND_DISPLAY ?? "";
  const proc = Bun.spawn(["bash", "tests/integration/s1_nested.sh"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  try {
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);

    // Sem o stderr aqui, uma falha aparece só como "expected 0, received 1"
    // e o erro real do compositor fica invisível (STATE.md §6, flake do S1).
    if (exitCode !== 0) {
      throw new Error(
        `S1 falhou (exit ${exitCode}).\n--- stderr do compositor ---\n${stderr}\n--- stdout ---\n${stdout}`,
      );
    }
    expect(stdout).toContain("OK: socket próprio");
    expect(process.env.WAYLAND_DISPLAY ?? "").toBe(hostDisplay);
    expect(stderr).not.toContain("FALHA");
  } finally {
    // Encerra SOMENTE o processo que este teste iniciou (AGENTS.md §0.5):
    // nunca matar por nome. O trap do script repassa o sinal ao compositor.
    if (proc.exitCode === null) {
      proc.kill();
      await proc.exited;
    }
  }
}, 120_000);
