import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";
import { readWastDb, writeWastDb, sourceKind, type WastFunc, type FuncSource } from "../wast-db.js";

/**
 * Parse a basic func definition from text.
 *
 * Expected format:
 *   func <uid>
 *     source: internal(<id>) | imported(<id>) | exported(<id>)
 *     params: <pid>: <type>, ...
 *     result: <type>
 *     body: <N> bytes
 *
 * Returns parsed funcs or null on parse failure.
 */
function parseFuncBlocks(text: string): { uid: string; func: WastFunc }[] | null {
  const blocks = text.split(/^(?=func\s)/m).filter((b) => b.trim().length > 0);
  const results: { uid: string; func: WastFunc }[] = [];

  for (const block of blocks) {
    const lines = block.split("\n").map((l) => l.trim()).filter((l) => l.length > 0);

    // Skip comment lines
    const nonComment = lines.filter((l) => !l.startsWith("#"));
    if (nonComment.length === 0) continue;

    const headerMatch = nonComment[0].match(/^func\s+(\S+)/);
    if (!headerMatch) continue;

    const uid = headerMatch[1];
    let source: FuncSource = { Internal: uid };
    let params: [string, string][] = [];
    let result: string | null = null;

    for (const line of nonComment.slice(1)) {
      const srcMatch = line.match(/^source:\s+(internal|imported|exported)\(([^)]*)\)/);
      if (srcMatch) {
        const kind = srcMatch[1];
        const id = srcMatch[2];
        if (kind === "internal") source = { Internal: id };
        else if (kind === "imported") source = { Imported: id };
        else if (kind === "exported") source = { Exported: id };
      }

      const paramMatch = line.match(/^params:\s+(.+)$/);
      if (paramMatch && paramMatch[1] !== "(none)") {
        const paramStr = paramMatch[1];
        for (const p of paramStr.split(",")) {
          const pm = p.trim().match(/^(\S+?)(?:\([^)]*\))?:\s*(\S+)$/);
          if (pm) {
            params.push([pm[1], pm[2]]);
          }
        }
      }

      const resMatch = line.match(/^result:\s+(\S+)$/);
      if (resMatch) {
        result = resMatch[1];
      }
    }

    results.push({
      uid,
      func: { source, params, result, body: null },
    });
  }

  return results.length > 0 ? results : null;
}

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

  const db = readWastDb(dir);
  const input = await readStdin();

  if (input.trim().length === 0) {
    errorExit("no input received on stdin", options.json);
  }

  const parsed = parseFuncBlocks(input);

  if (!parsed) {
    errorExit("could not parse any func definitions from stdin", options.json);
  }

  // Build existing func map
  const funcMap = new Map(db.funcs);
  const added: string[] = [];
  const updated: string[] = [];

  for (const { uid, func } of parsed) {
    if (funcMap.has(uid)) {
      // Merge: preserve existing body if new one is null
      const existing = funcMap.get(uid)!;
      const merged: WastFunc = {
        source: func.source,
        params: func.params,
        result: func.result,
        body: func.body ?? existing.body,
      };
      funcMap.set(uid, merged);
      updated.push(uid);
    } else {
      funcMap.set(uid, func);
      added.push(uid);
    }
  }

  if (options.dryRun) {
    if (options.json) {
      console.log(
        JSON.stringify({
          ok: true,
          command: "merge",
          dry_run: true,
          would_add: added,
          would_update: updated,
        }),
      );
    } else {
      console.log("dry-run: no changes written");
      if (added.length > 0) console.log(`  would add: ${added.join(", ")}`);
      if (updated.length > 0) console.log(`  would update: ${updated.join(", ")}`);
      if (added.length === 0 && updated.length === 0) console.log("  no changes detected");
    }
    return;
  }

  // Write back
  db.funcs = Array.from(funcMap.entries());
  writeWastDb(dir, db);

  if (options.json) {
    console.log(
      JSON.stringify({
        ok: true,
        command: "merge",
        added,
        updated,
      }),
    );
  } else {
    if (added.length > 0) console.log(`added: ${added.join(", ")}`);
    if (updated.length > 0) console.log(`updated: ${updated.join(", ")}`);
    if (added.length === 0 && updated.length === 0) console.log("no changes");
    console.log(`wrote ${join(dir, "wast.db")}`);
  }
}
