import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, notImplemented } from "../index.js";

export function bindgen(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 1, "wast bindgen <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "world.wit"), "world.wit", options.json);
  notImplemented("bindgen", options.json);
}
