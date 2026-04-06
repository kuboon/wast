import { join } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile } from "../index.js";
import { readWastDb, formatFunc, formatTypeDef, formatSource, type WastDb, type WastFunc, type WastTypeDef } from "../wast-db.js";
import { parseSyms, type SymsData } from "../syms-io.js";

interface DiffEntry {
  uid: string;
  kind: "func" | "type";
  status: "added" | "removed" | "changed";
  details?: string[];
}

function loadSyms(dir: string, lang: string): Map<string, string> {
  const symsFile = join(dir, `syms.${lang}.yaml`);
  const lookup = new Map<string, string>();
  if (existsSync(symsFile)) {
    const symsData: SymsData = parseSyms(readFileSync(symsFile, "utf-8"));
    for (const [k, v] of symsData.wit) lookup.set(k, v);
    for (const [k, v] of symsData.internal) lookup.set(k, v);
    for (const [k, v] of symsData.local) lookup.set(k, v);
  }
  return lookup;
}

function diffFuncs(
  aFuncs: Map<string, WastFunc>,
  bFuncs: Map<string, WastFunc>,
): DiffEntry[] {
  const entries: DiffEntry[] = [];

  // Removed: in A but not in B
  for (const [uid] of aFuncs) {
    if (!bFuncs.has(uid)) {
      entries.push({ uid, kind: "func", status: "removed" });
    }
  }

  // Added: in B but not in A
  for (const [uid] of bFuncs) {
    if (!aFuncs.has(uid)) {
      entries.push({ uid, kind: "func", status: "added" });
    }
  }

  // Changed: in both but different
  for (const [uid, aFunc] of aFuncs) {
    const bFunc = bFuncs.get(uid);
    if (!bFunc) continue;

    const details: string[] = [];

    // Compare source
    const aSrc = formatSource(aFunc.source);
    const bSrc = formatSource(bFunc.source);
    if (aSrc !== bSrc) {
      details.push(`source: ${aSrc} -> ${bSrc}`);
    }

    // Compare params
    const aParams = JSON.stringify(aFunc.params);
    const bParams = JSON.stringify(bFunc.params);
    if (aParams !== bParams) {
      details.push(`params changed`);
    }

    // Compare result
    if (aFunc.result !== bFunc.result) {
      details.push(`result: ${aFunc.result ?? "(none)"} -> ${bFunc.result ?? "(none)"}`);
    }

    // Compare body length
    const aBodyLen = aFunc.body?.length ?? 0;
    const bBodyLen = bFunc.body?.length ?? 0;
    if (aBodyLen !== bBodyLen) {
      details.push(`body: ${aBodyLen} bytes -> ${bBodyLen} bytes`);
    } else if (aBodyLen > 0 && JSON.stringify(aFunc.body) !== JSON.stringify(bFunc.body)) {
      details.push(`body: ${aBodyLen} bytes (contents differ)`);
    }

    if (details.length > 0) {
      entries.push({ uid, kind: "func", status: "changed", details });
    }
  }

  return entries;
}

function diffTypes(
  aTypes: Map<string, WastTypeDef>,
  bTypes: Map<string, WastTypeDef>,
): DiffEntry[] {
  const entries: DiffEntry[] = [];

  for (const [uid] of aTypes) {
    if (!bTypes.has(uid)) {
      entries.push({ uid, kind: "type", status: "removed" });
    }
  }

  for (const [uid] of bTypes) {
    if (!aTypes.has(uid)) {
      entries.push({ uid, kind: "type", status: "added" });
    }
  }

  for (const [uid, aDef] of aTypes) {
    const bDef = bTypes.get(uid);
    if (!bDef) continue;

    const details: string[] = [];

    const aSrc = formatSource(aDef.source);
    const bSrc = formatSource(bDef.source);
    if (aSrc !== bSrc) {
      details.push(`source: ${aSrc} -> ${bSrc}`);
    }

    if (JSON.stringify(aDef.definition) !== JSON.stringify(bDef.definition)) {
      details.push("definition changed");
    }

    if (details.length > 0) {
      entries.push({ uid, kind: "type", status: "changed", details });
    }
  }

  return entries;
}

function diffSyms(
  aSyms: Map<string, string>,
  bSyms: Map<string, string>,
): { added: [string, string][]; removed: [string, string][]; changed: [string, string, string][] } {
  const added: [string, string][] = [];
  const removed: [string, string][] = [];
  const changed: [string, string, string][] = [];

  for (const [uid, name] of aSyms) {
    if (!bSyms.has(uid)) {
      removed.push([uid, name]);
    } else if (bSyms.get(uid) !== name) {
      changed.push([uid, name, bSyms.get(uid)!]);
    }
  }

  for (const [uid, name] of bSyms) {
    if (!aSyms.has(uid)) {
      added.push([uid, name]);
    }
  }

  return { added, removed, changed };
}

export function diff(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast diff <dir-a> <dir-b>", options.json);
  const dirA = requireDir(positionals[0], options.json);
  const dirB = requireDir(positionals[1], options.json);
  requireFile(join(dirA, "wast.db"), "wast.db (dir-a)", options.json);
  requireFile(join(dirB, "wast.db"), "wast.db (dir-b)", options.json);

  const dbA = readWastDb(dirA);
  const dbB = readWastDb(dirB);

  const aFuncs = new Map(dbA.funcs);
  const bFuncs = new Map(dbB.funcs);
  const aTypes = new Map(dbA.types);
  const bTypes = new Map(dbB.types);

  const funcDiffs = diffFuncs(aFuncs, bFuncs);
  const typeDiffs = diffTypes(aTypes, bTypes);

  // Syms diff
  const aSyms = loadSyms(dirA, options.symsLang);
  const bSyms = loadSyms(dirB, options.symsLang);
  const symsDiff = diffSyms(aSyms, bSyms);
  const hasSymsChanges = symsDiff.added.length + symsDiff.removed.length + symsDiff.changed.length > 0;

  const totalChanges = funcDiffs.length + typeDiffs.length + (hasSymsChanges ? 1 : 0);

  if (options.json) {
    console.log(
      JSON.stringify({
        ok: true,
        command: "diff",
        identical: totalChanges === 0,
        funcs: funcDiffs,
        types: typeDiffs,
        syms: hasSymsChanges ? symsDiff : undefined,
      }),
    );
    return;
  }

  if (totalChanges === 0) {
    console.log("identical");
    return;
  }

  // Print func diffs
  if (funcDiffs.length > 0) {
    console.log("## Functions");
    console.log();
    for (const entry of funcDiffs) {
      const prefix = entry.status === "added" ? "+" : entry.status === "removed" ? "-" : "~";
      const sym = aSyms.get(entry.uid) ?? bSyms.get(entry.uid);
      const label = sym ? `${entry.uid} (${sym})` : entry.uid;
      console.log(`  ${prefix} func ${label}`);
      if (entry.details) {
        for (const d of entry.details) {
          console.log(`      ${d}`);
        }
      }
    }
    console.log();
  }

  // Print type diffs
  if (typeDiffs.length > 0) {
    console.log("## Types");
    console.log();
    for (const entry of typeDiffs) {
      const prefix = entry.status === "added" ? "+" : entry.status === "removed" ? "-" : "~";
      console.log(`  ${prefix} type ${entry.uid}`);
      if (entry.details) {
        for (const d of entry.details) {
          console.log(`      ${d}`);
        }
      }
    }
    console.log();
  }

  // Print syms diffs
  if (hasSymsChanges) {
    console.log("## Syms");
    console.log();
    for (const [uid, name] of symsDiff.added) {
      console.log(`  + ${uid}: ${name}`);
    }
    for (const [uid, name] of symsDiff.removed) {
      console.log(`  - ${uid}: ${name}`);
    }
    for (const [uid, oldName, newName] of symsDiff.changed) {
      console.log(`  ~ ${uid}: ${oldName} -> ${newName}`);
    }
    console.log();
  }

  // Summary
  const parts: string[] = [];
  const added = funcDiffs.filter((d) => d.status === "added").length + typeDiffs.filter((d) => d.status === "added").length;
  const removed = funcDiffs.filter((d) => d.status === "removed").length + typeDiffs.filter((d) => d.status === "removed").length;
  const changed = funcDiffs.filter((d) => d.status === "changed").length + typeDiffs.filter((d) => d.status === "changed").length;
  if (added > 0) parts.push(`${added} added`);
  if (removed > 0) parts.push(`${removed} removed`);
  if (changed > 0) parts.push(`${changed} changed`);
  if (hasSymsChanges) parts.push("syms differ");
  console.log(parts.join(", "));
}
