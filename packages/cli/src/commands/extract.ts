import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, notImplemented } from "../index.js";

export function extract(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast extract <component-dir> <uid> [uid...]", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);
  // positionals[1..] are UIDs; --include-caller is in options.includeCaller
  notImplemented("extract", options.json);
}
