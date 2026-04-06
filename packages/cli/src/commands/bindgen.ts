import { join } from "node:path";
import { existsSync } from "node:fs";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";
import { loadFileManager } from "../wasm-plugin.js";

export async function bindgen(positionals: string[], options: GlobalOptions): Promise<void> {
  requireArgs(positionals, 1, "wast bindgen <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "world.wit"), "world.wit", options.json);

  const dbPath = join(dir, "wast.db");
  if (existsSync(dbPath)) {
    errorExit("wast.db already exists — remove it first to re-generate", options.json);
  }

  try {
    const fm = await loadFileManager();
    fm.bindgen(dir);
  } catch (err: any) {
    const msg = err?.message ?? String(err);
    errorExit(`bindgen failed: ${msg}`, options.json);
  }

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "bindgen", path: dbPath }));
  } else {
    console.log(`created ${dbPath}`);
  }
}
