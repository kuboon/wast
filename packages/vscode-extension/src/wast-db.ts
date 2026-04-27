/**
 * Types and reader for wast.json and syms.*.yaml files.
 *
 * Mirrors the Rust serde types in `wast-types`, so the VS Code extension
 * can read component data directly without a wasm runtime.
 *
 * On-disk format today is `wast.json` (row-oriented JSON). Future migration
 * target is `wast.db` (SQLite with the same logical schema).
 */

import * as fs from "node:fs";
import * as path from "node:path";

// ---------------------------------------------------------------------------
// wast.json types (matches crates/wast-types/src/lib.rs)
//
// Row-oriented: each func/type row inlines its uid alongside its payload
// so the JSON maps 1:1 to SQLite rows when the `wast.db` migration lands.
// ---------------------------------------------------------------------------

export type FuncSource =
  | { Internal: string }
  | { Imported: string }
  | { Exported: string };

export interface WastFunc {
  source: FuncSource;
  params: [string, string][];
  result: string | null;
  body: number[] | null;
}

/** A row in the on-disk `funcs` table: `uid` flattened alongside WastFunc fields. */
export type WastFuncRow = { uid: string } & WastFunc;

export interface WastDb {
  funcs: WastFuncRow[];
  types: ({ uid: string } & Record<string, unknown>)[];
}

// ---------------------------------------------------------------------------
// Syms YAML types (simple YAML-like format, same parser as CLI)
// ---------------------------------------------------------------------------

export interface SymsData {
  wit: Map<string, string>;
  internal: Map<string, string>;
  local: Map<string, string>;
}

const SECTIONS = ["wit", "internal", "local"] as const;
type Section = (typeof SECTIONS)[number];

export function parseSyms(text: string): SymsData {
  const data: SymsData = {
    wit: new Map(),
    internal: new Map(),
    local: new Map(),
  };

  let currentSection: Section | null = null;

  for (const line of text.split("\n")) {
    const trimmed = line.trimEnd();
    if (trimmed === "" || trimmed.startsWith("#")) continue;

    const sectionMatch = trimmed.match(/^(wit|internal|local):$/);
    if (sectionMatch) {
      currentSection = sectionMatch[1] as Section;
      continue;
    }

    if (currentSection !== null) {
      const entryMatch = trimmed.match(/^\s+([^:]+):\s+(.+)$/);
      if (entryMatch) {
        data[currentSection].set(entryMatch[1].trim(), entryMatch[2].trim());
      }
    }
  }

  return data;
}

// ---------------------------------------------------------------------------
// Source helpers
// ---------------------------------------------------------------------------

export function funcSourceType(source: FuncSource): "internal" | "imported" | "exported" {
  if ("Internal" in source) return "internal";
  if ("Imported" in source) return "imported";
  return "exported";
}

export function funcSourceId(source: FuncSource): string {
  if ("Internal" in source) return source.Internal;
  if ("Imported" in source) return source.Imported;
  return source.Exported;
}

// ---------------------------------------------------------------------------
// Loaded component (wast.json + syms merged)
// ---------------------------------------------------------------------------

export interface LoadedFunc {
  uid: string;
  source: FuncSource;
  sourceType: "internal" | "imported" | "exported";
  displayName: string | undefined;
  params: [string, string][];
  result: string | null;
}

export interface LoadedComponent {
  dir: string;
  name: string;
  funcs: LoadedFunc[];
}

/**
 * Read a component directory's wast.json and syms file.
 */
export function readComponent(dir: string, lang: string): LoadedComponent | null {
  const dbPath = path.join(dir, "wast.json");
  if (!fs.existsSync(dbPath)) return null;

  let db: WastDb;
  try {
    const raw = fs.readFileSync(dbPath, "utf-8");
    db = JSON.parse(raw) as WastDb;
  } catch {
    return null;
  }

  // Read syms
  const symsPath = path.join(dir, `syms.${lang}.yaml`);
  let syms: SymsData = { wit: new Map(), internal: new Map(), local: new Map() };
  if (fs.existsSync(symsPath)) {
    try {
      const raw = fs.readFileSync(symsPath, "utf-8");
      syms = parseSyms(raw);
    } catch {
      // ignore — proceed without syms
    }
  }

  const funcs: LoadedFunc[] = (db.funcs ?? []).map((row) => {
    const sourceType = funcSourceType(row.source);
    const sourceId = funcSourceId(row.source);

    // Look up display name from syms
    let displayName: string | undefined;
    if (sourceType === "internal") {
      displayName = syms.internal.get(row.uid) ?? syms.internal.get(sourceId);
    } else {
      // wit (imported/exported) — look in wit syms
      displayName = syms.wit.get(row.uid) ?? syms.wit.get(sourceId);
    }

    return {
      uid: row.uid,
      source: row.source,
      sourceType,
      displayName,
      params: row.params,
      result: row.result,
    };
  });

  return {
    dir,
    name: path.basename(dir),
    funcs,
  };
}
