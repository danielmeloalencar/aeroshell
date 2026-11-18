import { expect, test } from "bun:test";

test("S1: socket próprio e isolamento do WAYLAND_DISPLAY do host", async () => {
  const hostDisplay = process.env.WAYLAND_DISPLAY ?? "";
  const proc = Bun.spawn(["bash", "tests/integration/s1_nested.sh"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);

  expect(exitCode).toBe(0);
  expect(stdout).toContain("OK: socket próprio");
  expect(process.env.WAYLAND_DISPLAY ?? "").toBe(hostDisplay);
  expect(stderr).not.toContain("FALHA");
}, 120_000);
