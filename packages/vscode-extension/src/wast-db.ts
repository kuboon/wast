/**
 * Read wast.json + syms.*.yaml from a component directory via
 * `vscode.workspace.fs` (works in both desktop and web hosts).
 *
 * Provides two views of the same data:
 *  - `LoadedComponent` — a UI-friendly summary used by the tree view.
 *  - `toWastComponent` — converts the raw rows + syms into the WIT-shaped
 *    `WastComponent` record that jco-transpiled wasm components consume.
 */

import * as vscode from "vscode";

const DECODER = new TextDecoder("utf-8");

// ---------------------------------------------------------------------------
// On-disk wast.json shapes — match crates/wast-types/src/lib.rs serde output.
// Row-oriented: each func/type entry inlines its uid alongside its payload
// so the JSON maps 1:1 to SQLite rows when the `wast.db` migration lands.
// ---------------------------------------------------------------------------

export type FuncSource =
  | { Internal: string }
  | { Imported: string }
  | { Exported: string };

export type TypeSource =
  | { Internal: string }
  | { Imported: string }
  | { Exported: string };

/** Match `wast_types::WitType` serde tagged union. */
export type WitType =
  | { Primitive: PrimitiveTag }
  | { Option: string }
  | { Result: [string, string] }
  | { List: string }
  | { Record: [string, string][] }
  | { Variant: [string, string | null][] }
  | { Tuple: string[] }
  | { Enum: string[] }
  | { Flags: string[] }
  | "Resource"
  | { Own: string }
  | { Borrow: string };

export type PrimitiveTag =
  | "U32" | "U64" | "I32" | "I64"
  | "F32" | "F64" | "Bool" | "Char" | "String";

export interface WastFunc {
  source: FuncSource;
  params: [string, string][];
  result: string | null;
  body: number[] | null;
}

export interface WastTypeDef {
  source: TypeSource;
  definition: WitType;
}

export type WastFuncRow = { uid: string } & WastFunc;
export type WastTypeRow = { uid: string } & WastTypeDef;

export interface WastDb {
  funcs: WastFuncRow[];
  types: WastTypeRow[];
}

// ---------------------------------------------------------------------------
// syms.*.yaml
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
// Loaded component (wast.json + syms merged) — view used by the tree.
// Holds the raw `db` + `syms` so the wasm-side converter can build a
// `WastComponent` without re-reading disk.
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
  /** Filesystem URI of the component directory. */
  dirUri: vscode.Uri;
  name: string;
  funcs: LoadedFunc[];
  /** Raw rows + syms — kept so callers can build a `WastComponent` view. */
  db: WastDb;
  syms: SymsData;
}

async function fileExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

async function readUtf8(uri: vscode.Uri): Promise<string | null> {
  try {
    const bytes = await vscode.workspace.fs.readFile(uri);
    return DECODER.decode(bytes);
  } catch {
    return null;
  }
}

/** Read raw bytes of `<dir>/world.wit` if present. */
export async function readWorldWit(dirUri: vscode.Uri): Promise<Uint8Array | null> {
  const uri = vscode.Uri.joinPath(dirUri, "world.wit");
  try {
    return await vscode.workspace.fs.readFile(uri);
  } catch {
    return null;
  }
}

/**
 * Read a component directory's wast.json + syms.<lang>.yaml.
 * Returns null if wast.json is missing or unparseable.
 */
export async function readComponent(
  dirUri: vscode.Uri,
  lang: string,
): Promise<LoadedComponent | null> {
  const dbUri = vscode.Uri.joinPath(dirUri, "wast.json");
  const dbText = await readUtf8(dbUri);
  if (dbText === null) return null;

  let db: WastDb;
  try {
    db = JSON.parse(dbText) as WastDb;
  } catch {
    return null;
  }

  let syms: SymsData = { wit: new Map(), internal: new Map(), local: new Map() };
  const symsUri = vscode.Uri.joinPath(dirUri, `syms.${lang}.yaml`);
  if (await fileExists(symsUri)) {
    const symsText = await readUtf8(symsUri);
    if (symsText !== null) {
      try {
        syms = parseSyms(symsText);
      } catch {
        // ignore — proceed without syms
      }
    }
  }

  const funcs: LoadedFunc[] = (db.funcs ?? []).map((row) => {
    const sourceType = funcSourceType(row.source);
    const sourceId = funcSourceId(row.source);
    const displayName =
      sourceType === "internal"
        ? syms.internal.get(row.uid) ?? syms.internal.get(sourceId)
        : syms.wit.get(row.uid) ?? syms.wit.get(sourceId);
    return {
      uid: row.uid,
      source: row.source,
      sourceType,
      displayName,
      params: row.params,
      result: row.result,
    };
  });

  // Derive the directory's basename from the URI path.
  const segments = dirUri.path.split("/").filter((s) => s !== "");
  const name = segments[segments.length - 1] ?? dirUri.path;

  return { dirUri, name, funcs, db, syms };
}

// ---------------------------------------------------------------------------
// WIT-shaped `WastComponent` — what jco-transpiled wasm components consume.
// Field names are camelCase to match jco's bindgen output, and tagged
// unions are `{ tag, val }` rather than serde's `{ Variant: payload }`.
// ---------------------------------------------------------------------------

export type FuncSourceWasm =
  | { tag: "internal"; val: string }
  | { tag: "imported"; val: string }
  | { tag: "exported"; val: string };

export type TypeSourceWasm = FuncSourceWasm;

export type PrimitiveTypeWasm =
  | "u32" | "u64" | "i32" | "i64"
  | "f32" | "f64" | "bool" | "char" | "string";

export type WitTypeWasm =
  | { tag: "primitive"; val: PrimitiveTypeWasm }
  | { tag: "option"; val: string }
  | { tag: "result"; val: [string, string] }
  | { tag: "list"; val: string }
  | { tag: "record"; val: [string, string][] }
  | { tag: "variant"; val: [string, string | null][] }
  | { tag: "tuple"; val: string[] }
  | { tag: "enum"; val: string[] }
  | { tag: "flags"; val: string[] }
  | { tag: "resource" }
  | { tag: "own"; val: string }
  | { tag: "borrow"; val: string };

export interface WastFuncWasm {
  source: FuncSourceWasm;
  params: [string, string][];
  result: string | null;
  body: Uint8Array | null;
}

export interface WastTypeDefWasm {
  source: TypeSourceWasm;
  definition: WitTypeWasm;
}

export interface SymsWasm {
  witSyms: [string, string][];
  internal: { uid: string; displayName: string }[];
  local: { uid: string; displayName: string }[];
}

export interface WastComponent {
  funcs: [string, WastFuncWasm][];
  types: [string, WastTypeDefWasm][];
  syms: SymsWasm;
}

const PRIMITIVE_TAGS: Record<PrimitiveTag, PrimitiveTypeWasm> = {
  U32: "u32", U64: "u64", I32: "i32", I64: "i64",
  F32: "f32", F64: "f64", Bool: "bool", Char: "char", String: "string",
};

function convertSource(s: FuncSource | TypeSource): FuncSourceWasm {
  if ("Internal" in s) return { tag: "internal", val: s.Internal };
  if ("Imported" in s) return { tag: "imported", val: s.Imported };
  return { tag: "exported", val: s.Exported };
}

function convertWitType(t: WitType): WitTypeWasm {
  if (t === "Resource") return { tag: "resource" };
  if ("Primitive" in t) return { tag: "primitive", val: PRIMITIVE_TAGS[t.Primitive] };
  if ("Option" in t) return { tag: "option", val: t.Option };
  if ("Result" in t) return { tag: "result", val: t.Result };
  if ("List" in t) return { tag: "list", val: t.List };
  if ("Record" in t) return { tag: "record", val: t.Record };
  if ("Variant" in t) return { tag: "variant", val: t.Variant };
  if ("Tuple" in t) return { tag: "tuple", val: t.Tuple };
  if ("Enum" in t) return { tag: "enum", val: t.Enum };
  if ("Flags" in t) return { tag: "flags", val: t.Flags };
  if ("Own" in t) return { tag: "own", val: t.Own };
  return { tag: "borrow", val: t.Borrow };
}

/**
 * Convert a `LoadedComponent` (wast.json rows + syms) into the WIT-shaped
 * `WastComponent` that wasm components expect.
 */
export function toWastComponent(loaded: LoadedComponent): WastComponent {
  const funcs: [string, WastFuncWasm][] = loaded.db.funcs.map((row) => [
    row.uid,
    {
      source: convertSource(row.source),
      params: row.params,
      result: row.result,
      body: row.body !== null ? Uint8Array.from(row.body) : null,
    },
  ]);
  const types: [string, WastTypeDefWasm][] = loaded.db.types.map((row) => [
    row.uid,
    {
      source: convertSource(row.source),
      definition: convertWitType(row.definition),
    },
  ]);
  const syms: SymsWasm = {
    witSyms: [...loaded.syms.wit.entries()],
    internal: [...loaded.syms.internal.entries()].map(([uid, displayName]) => ({
      uid,
      displayName,
    })),
    local: [...loaded.syms.local.entries()].map(([uid, displayName]) => ({
      uid,
      displayName,
    })),
  };
  return { funcs, types, syms };
}
