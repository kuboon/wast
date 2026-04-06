import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile } from "../index.js";

export function extract(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast extract <component-dir> <uid> [uid...]", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);

  if (options.json) {
    console.log(JSON.stringify({ ok: false, command: "extract", errors: [{ code: "not_ready", message: "extract requires wasm runtime (file-manager, partial-manager, syntax-plugin) which is not yet integrated" }] }));
  } else {
    console.error("error: extract requires wasm runtime (file-manager, partial-manager, syntax-plugin) which is not yet integrated");
  }
  process.exit(2);
}
