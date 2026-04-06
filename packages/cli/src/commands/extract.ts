import { join } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import type { GlobalOptions } from "../index.js";
import { requireArgs, requireDir, requireFile, errorExit } from "../index.js";
import { readWastDb, formatFunc } from "../wast-db.js";
import { parseSyms, type SymsData } from "../syms-io.js";

export function extract(positionals: string[], options: GlobalOptions): void {
  requireArgs(positionals, 2, "wast extract <component-dir> <uid> [uid...]", options.json);
  const dir = requireDir(positionals[0], options.json);
  requireFile(join(dir, "wast.db"), "wast.db", options.json);

  const uids = positionals.slice(1);
  const db = readWastDb(dir);

  // Build func lookup
  const funcMap = new Map(db.funcs);

  // Load syms if available
  const symsFile = join(dir, `syms.${options.symsLang}.yaml`);
  let symsLookup = new Map<string, string>();
  if (existsSync(symsFile)) {
    const symsData: SymsData = parseSyms(readFileSync(symsFile, "utf-8"));
    for (const [k, v] of symsData.wit) symsLookup.set(k, v);
    for (const [k, v] of symsData.internal) symsLookup.set(k, v);
    for (const [k, v] of symsData.local) symsLookup.set(k, v);
  }

  // Resolve requested UIDs
  const found: { uid: string; text: string }[] = [];
  const missing: string[] = [];

  for (const uid of uids) {
    const func = funcMap.get(uid);
    if (func) {
      found.push({ uid, text: formatFunc(uid, func, symsLookup) });
    } else {
      missing.push(uid);
    }
  }

  // If --include-caller, scan all func bodies for calls to target UIDs
  // (basic heuristic: check if uid bytes appear in body — real implementation
  //  would decode instructions via wasm runtime)
  if (options.includeCaller) {
    const targetSet = new Set(uids);
    const alreadyIncluded = new Set(found.map((f) => f.uid));

    for (const [fuid, func] of db.funcs) {
      if (alreadyIncluded.has(fuid)) continue;
      if (!func.body) continue;

      // Check if any target UID appears as a call reference in the body
      // Body format is opaque bytes — we do a simple string scan for the UID
      // This is a best-effort heuristic; real implementation uses partial-manager
      const bodyStr = String.fromCharCode(...func.body);
      for (const target of targetSet) {
        if (bodyStr.includes(target)) {
          found.push({ uid: fuid, text: formatFunc(fuid, func, symsLookup) });
          alreadyIncluded.add(fuid);
          break;
        }
      }
    }
  }

  if (options.json) {
    console.log(
      JSON.stringify({
        ok: true,
        command: "extract",
        found: found.map((f) => f.uid),
        missing,
        text: found.map((f) => f.text).join("\n\n"),
      }),
    );
  } else {
    if (missing.length > 0) {
      console.error(`warning: UIDs not found: ${missing.join(", ")}`);
    }
    if (found.length === 0) {
      errorExit("no matching functions found", options.json);
    }
    console.log(`# Extracted functions: ${found.map((f) => f.uid).join(", ")}`);
    console.log();
    for (let i = 0; i < found.length; i++) {
      if (i > 0) console.log();
      console.log(found[i].text);
    }
  }
}
