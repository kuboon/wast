import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, notImplemented } from "../index.js";

export function syms(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 3, "wast syms <component-dir> <uid> <display-name>", options.json);
  requireDir(positionals[0], options.json);
  notImplemented("syms", options.json);
}
