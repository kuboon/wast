import { join } from "node:path";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile } from "../index.js";
import { loadFileManager, loadTsLikePlugin } from "../wasm-plugin.js";

export async function diff(positionals: string[], options: GlobalOptions): Promise<void> {
  requireArgs(positionals, 2, "wast diff <dir-a> <dir-b>", options.json);
  const dirA = requireDir(positionals[0], options.json);
  const dirB = requireDir(positionals[1], options.json);
  requireFile(join(dirA, "wast.db"), "wast.db (dir-a)", options.json);
  requireFile(join(dirB, "wast.db"), "wast.db (dir-b)", options.json);

  const fm = await loadFileManager();
  const plugin = await loadTsLikePlugin();

  // Read both components via file-manager
  const { db: dbA, syms: symsA } = fm.read(dirA);
  const { db: dbB, syms: symsB } = fm.read(dirB);

  // Render both as text via syntax-plugin for human-readable diff
  const textA = plugin.toText(dbA, symsA);
  const textB = plugin.toText(dbB, symsB);

  if (options.json) {
    console.log(JSON.stringify({
      ok: true,
      command: "diff",
      identical: textA === textB,
      a: { funcs: dbA.funcs.length, types: dbA.types.length },
      b: { funcs: dbB.funcs.length, types: dbB.types.length },
      textA,
      textB,
    }));
    return;
  }

  if (textA === textB) {
    console.log("identical");
    return;
  }

  // Simple line-by-line diff
  const linesA = textA.split("\n");
  const linesB = textB.split("\n");

  // Build per-function blocks for A and B
  const blocksA = splitFunctionBlocks(linesA);
  const blocksB = splitFunctionBlocks(linesB);

  const allNames = new Set([...blocksA.keys(), ...blocksB.keys()]);
  let changeCount = 0;

  for (const name of allNames) {
    const a = blocksA.get(name);
    const b = blocksB.get(name);

    if (a && !b) {
      changeCount++;
      for (const line of a) console.log(`- ${line}`);
      console.log();
    } else if (!a && b) {
      changeCount++;
      for (const line of b) console.log(`+ ${line}`);
      console.log();
    } else if (a && b && a.join("\n") !== b.join("\n")) {
      changeCount++;
      for (const line of a) console.log(`- ${line}`);
      for (const line of b) console.log(`+ ${line}`);
      console.log();
    }
  }

  if (changeCount === 0) {
    console.log("identical");
  } else {
    console.log(`${changeCount} function(s) differ`);
  }
}

/** Split text lines into blocks keyed by the first line (function signature). */
function splitFunctionBlocks(lines: string[]): Map<string, string[]> {
  const blocks = new Map<string, string[]>();
  let current: string[] = [];
  let name = "";

  for (const line of lines) {
    // Function declarations start at column 0 (no indent)
    if (line.length > 0 && line[0] !== " " && line[0] !== "\t" && line !== "}") {
      if (current.length > 0 && name) {
        blocks.set(name, current);
      }
      name = line;
      current = [line];
    } else if (name) {
      current.push(line);
    }
  }
  if (current.length > 0 && name) {
    blocks.set(name, current);
  }
  return blocks;
}
