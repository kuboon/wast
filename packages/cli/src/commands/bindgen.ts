import { join } from "node:path";
import { existsSync, writeFileSync } from "node:fs";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";

export function bindgen(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 1, "wast bindgen <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "world.wit"), "world.wit", options.json);

  const dbPath = join(dir, "wast.db");
  if (existsSync(dbPath)) {
    errorExit("wast.db already exists — remove it first to re-generate", options.json);
  }

  // Create initial empty wast.db in JSON format
  // TODO: parse world.wit for exported/imported functions
  const initial = {
    funcs: [],
    types: [],
    syms: {
      wit_syms: [],
      internal: [],
      local: [],
    },
  };

  writeFileSync(dbPath, JSON.stringify(initial, null, 2) + "\n");

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "bindgen", path: dbPath }));
  } else {
    console.log(`created ${dbPath}`);
    console.log("note: world.wit parsing not yet implemented — wast.db is empty scaffold");
  }
}
