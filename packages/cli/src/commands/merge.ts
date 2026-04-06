import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";
import { loadFileManager, loadTsLikePlugin } from "../wasm-plugin.js";

function readStdin(): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    process.stdin.on("data", (chunk) => chunks.push(chunk));
    process.stdin.on("end", () => resolve(Buffer.concat(chunks).toString("utf-8")));
    process.stdin.on("error", reject);
  });
}

export async function merge(positionals: string[], options: GlobalOptions): Promise<void> {
  requireArgs(positionals, 1, "wast merge <component-dir>", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);

  const input = await readStdin();
  if (input.trim().length === 0) {
    errorExit("no input received on stdin", options.json);
  }

  const fm = await loadFileManager();
  const plugin = await loadTsLikePlugin();

  // Read existing component via file-manager
  const { db: existingDb, syms: existingSyms } = fm.read(dir);

  // Parse text via syntax-plugin (uses existing component for UID resolution)
  let partialDb, partialSyms;
  try {
    const result = plugin.fromText(input, existingDb, existingSyms);
    partialDb = result.db;
    partialSyms = result.syms;
  } catch (err: any) {
    const msg = err?.message ?? String(err);
    errorExit(`parse error: ${msg}`, options.json);
  }

  if (options.dryRun) {
    // Compare to find what would change
    const existingUids = new Set(existingDb.funcs.map(([uid]) => uid));
    const newUids = partialDb.funcs.map(([uid]) => uid);
    const wouldAdd = newUids.filter((uid) => !existingUids.has(uid));
    const wouldUpdate = newUids.filter((uid) => existingUids.has(uid));

    if (options.json) {
      console.log(JSON.stringify({
        ok: true,
        command: "merge",
        dry_run: true,
        would_add: wouldAdd,
        would_update: wouldUpdate,
      }));
    } else {
      console.log("dry-run: no changes written");
      if (wouldAdd.length > 0) console.log(`  would add: ${wouldAdd.join(", ")}`);
      if (wouldUpdate.length > 0) console.log(`  would update: ${wouldUpdate.join(", ")}`);
      if (wouldAdd.length === 0 && wouldUpdate.length === 0) console.log("  no changes detected");
    }
    return;
  }

  // Merge via file-manager (validates against world.wit and writes to disk)
  try {
    fm.merge(dir, partialDb, partialSyms);
  } catch (err: any) {
    const msg = err?.message ?? String(err);
    errorExit(`merge failed: ${msg}`, options.json);
  }

  if (options.json) {
    console.log(JSON.stringify({ ok: true, command: "merge" }));
  } else {
    console.log(`merged into ${join(dir, "wast.db")}`);
  }
}
