import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile } from "../index.js";

export function merge(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 1, "wast merge <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);

  if (options.json) {
    console.log(JSON.stringify({ ok: false, command: "merge", errors: [{ code: "not_ready", message: "merge requires wasm runtime (syntax-plugin, file-manager) which is not yet integrated" }] }));
  } else {
    console.error("error: merge requires wasm runtime (syntax-plugin, file-manager) which is not yet integrated");
  }
  process.exit(2);
}
