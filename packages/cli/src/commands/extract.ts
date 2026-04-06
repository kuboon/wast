import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";
import { join } from "node:path";
import { loadFileManager, loadPartialManager, loadTsLikePlugin } from "../wasm-plugin.js";

export async function extract(positionals: string[], options: GlobalOptions): Promise<void> {
  requireArgs(positionals, 2, "wast extract <component-dir> <uid> [uid...]", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);

  const uids = positionals.slice(1);

  const fm = await loadFileManager();
  const pm = await loadPartialManager();
  const plugin = await loadTsLikePlugin();

  // Read the full component via file-manager (reads wast.db + syms)
  const { db: fullDb, syms: fullSyms } = fm.read(dir);

  // Build extract targets
  const targets = uids.map((uid) => ({
    sym: uid,
    includeCaller: options.includeCaller,
  }));

  // Use partial-manager to extract (handles call-graph analysis, type refs, etc.)
  const { db: partialDb, syms: partialSyms } = pm.extract(fullDb, fullSyms, targets);

  if (partialDb.funcs.length === 0) {
    errorExit("no matching functions found", options.json);
  }

  // Render extracted component as text via syntax-plugin
  const text = plugin.toText(partialDb, partialSyms);

  if (options.json) {
    console.log(
      JSON.stringify({
        ok: true,
        command: "extract",
        found: partialDb.funcs.map(([uid]) => uid),
        text,
      }),
    );
  } else {
    process.stdout.write(text);
    if (!text.endsWith("\n")) process.stdout.write("\n");
  }
}
