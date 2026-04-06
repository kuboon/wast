import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir } from "../index.js";

export function diff(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast diff <dir-a> <dir-b>", options.json);
  requireDir(positionals[0], options.json);
  requireDir(positionals[1], options.json);

  if (options.json) {
    console.log(JSON.stringify({ ok: false, command: "diff", errors: [{ code: "not_ready", message: "diff requires wasm runtime (syntax-plugin) and difftastic which are not yet integrated" }] }));
  } else {
    console.error("error: diff requires wasm runtime (syntax-plugin) and difftastic which are not yet integrated");
  }
  process.exit(2);
}
