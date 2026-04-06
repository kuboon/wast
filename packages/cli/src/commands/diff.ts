import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, notImplemented } from "../index.js";

export function diff(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast diff <dir-a> <dir-b>", options.json);
  requireDir(positionals[0], options.json);
  requireDir(positionals[1], options.json);
  notImplemented("diff", options.json);
}
