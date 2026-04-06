import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, notImplemented } from "../index.js";

export function merge(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 1, "wast merge <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);
  // reads stdin; --dry-run is in options.dryRun
  notImplemented("merge", options.json);
}
