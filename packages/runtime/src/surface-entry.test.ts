import { expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { materializeProductionSurface } from "./surface-entry";

test("produção embute o bundle do dev_entry e remove a inicialização inline duplicada", async () => {
  const root = await mkdtemp(join("/tmp/opencode", "aero-production-entry-"));
  const entry = join(root, "panel.ts");
  await writeFile(entry, "document.body.dataset.fromProductionBundle = 'presente';\n");
  try {
    const result = await materializeProductionSurface({
      kind: "Panel",
      id: "panel",
      html: "<!doctype html><html><head></head><body><div id='root'></div><script>window.duplicado = true;</script></body></html>",
      dev_entry: entry,
    });
    expect(result.dev_entry).toBeUndefined();
    expect(result.html).toContain("fromProductionBundle");
    expect(result.html).toContain("id='root'");
    expect(result.html).not.toContain("window.duplicado");
    expect(result.html).not.toContain("http://127.0.0.1");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
