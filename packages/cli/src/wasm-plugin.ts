/**
 * Load a transpiled WASM syntax-plugin component and expose toText/fromText.
 *
 * The jco-transpiled output uses tagged-union types (e.g. {tag:'internal', val:'f1'})
 * while our wast-db.ts uses Rust-serde style ({Internal:'f1'}).  The bridge
 * functions below convert between the two representations.
 */

import type { WastDb, WastFunc, WastTypeDef, FuncSource, TypeSource, WitType } from "./wast-db.js";
import type {
  WastComponent as WasmComponent,
  WastFunc as WasmFunc,
  WastTypeDef as WasmTypeDef,
  FuncSource as WasmFuncSource,
  TypeSource as WasmTypeSource,
  WitType as WasmWitType,
  Syms as WasmSyms,
  SymEntry as WasmSymEntry,
  WastError,
} from "./generated/ts-like/interfaces/wast-core-types.js";

// Re-export the error type
export type { WastError };

export interface SymsData {
  wit: [string, string][];
  internal: [string, string][];
  local: [string, string][];
}

// ---------------------------------------------------------------------------
// Conversion: wast-db format → WASM component format
// ---------------------------------------------------------------------------

function dbFuncSourceToWasm(source: FuncSource): WasmFuncSource {
  if ("Internal" in source) return { tag: "internal", val: source.Internal };
  if ("Imported" in source) return { tag: "imported", val: source.Imported };
  if ("Exported" in source) return { tag: "exported", val: source.Exported };
  throw new Error(`unknown FuncSource: ${JSON.stringify(source)}`);
}

function dbTypeSourceToWasm(source: TypeSource): WasmTypeSource {
  if ("Internal" in source) return { tag: "internal", val: source.Internal };
  if ("Imported" in source) return { tag: "imported", val: source.Imported };
  if ("Exported" in source) return { tag: "exported", val: source.Exported };
  throw new Error(`unknown TypeSource: ${JSON.stringify(source)}`);
}

function dbWitTypeToWasm(def: WitType): WasmWitType {
  if ("Primitive" in def) return { tag: "primitive", val: def.Primitive.toLowerCase() as any };
  if ("Option" in def) return { tag: "option", val: def.Option };
  if ("Result" in def) return { tag: "result", val: def.Result as [string, string] };
  if ("List" in def) return { tag: "list", val: def.List };
  if ("Record" in def) return { tag: "record", val: def.Record };
  if ("Variant" in def) return { tag: "variant", val: def.Variant.map(([k, v]) => [k, v ?? undefined] as [string, string | undefined]) };
  if ("Tuple" in def) return { tag: "tuple", val: def.Tuple };
  throw new Error(`unknown WitType: ${JSON.stringify(def)}`);
}

function dbFuncToWasm(func: WastFunc): WasmFunc {
  return {
    source: dbFuncSourceToWasm(func.source),
    params: func.params,
    result: func.result ?? undefined,
    body: func.body ? new Uint8Array(func.body) : undefined,
  };
}

function dbTypeDefToWasm(td: WastTypeDef): WasmTypeDef {
  return {
    source: dbTypeSourceToWasm(td.source),
    definition: dbWitTypeToWasm(td.definition),
  };
}

export function dbToWasmComponent(db: WastDb, syms: SymsData): WasmComponent {
  return {
    funcs: db.funcs.map(([uid, func]) => [uid, dbFuncToWasm(func)] as [string, WasmFunc]),
    types: db.types.map(([uid, td]) => [uid, dbTypeDefToWasm(td)] as [string, WasmTypeDef]),
    syms: {
      witSyms: syms.wit,
      internal: syms.internal.map(([uid, name]) => ({ uid, displayName: name })),
      local: syms.local.map(([uid, name]) => ({ uid, displayName: name })),
    },
  };
}

// ---------------------------------------------------------------------------
// Conversion: WASM component format → wast-db format
// ---------------------------------------------------------------------------

function wasmFuncSourceToDb(source: WasmFuncSource): FuncSource {
  switch (source.tag) {
    case "internal": return { Internal: source.val };
    case "imported": return { Imported: source.val };
    case "exported": return { Exported: source.val };
  }
}

function wasmTypeSourceToDb(source: WasmTypeSource): TypeSource {
  switch (source.tag) {
    case "internal": return { Internal: source.val };
    case "imported": return { Imported: source.val };
    case "exported": return { Exported: source.val };
  }
}

function wasmWitTypeToDb(def: WasmWitType): WitType {
  switch (def.tag) {
    case "primitive": {
      const p = def.val.charAt(0).toUpperCase() + def.val.slice(1);
      return { Primitive: p as any };
    }
    case "option": return { Option: def.val };
    case "result": return { Result: def.val };
    case "list": return { List: def.val };
    case "record": return { Record: def.val as [string, string][] };
    case "variant": return { Variant: def.val.map(([k, v]) => [k, v ?? null] as [string, string | null]) };
    case "tuple": return { Tuple: def.val };
  }
}

function wasmFuncToDb(func: WasmFunc): WastFunc {
  return {
    source: wasmFuncSourceToDb(func.source),
    params: func.params as [string, string][],
    result: func.result ?? null,
    body: func.body ? Array.from(func.body) : null,
  };
}

function wasmTypeDefToDb(td: WasmTypeDef): WastTypeDef {
  return {
    source: wasmTypeSourceToDb(td.source),
    definition: wasmWitTypeToDb(td.definition),
  };
}

export function wasmComponentToDb(comp: WasmComponent): { db: WastDb; syms: SymsData } {
  return {
    db: {
      funcs: comp.funcs.map(([uid, func]) => [uid, wasmFuncToDb(func)] as [string, WastFunc]),
      types: comp.types.map(([uid, td]) => [uid, wasmTypeDefToDb(td)] as [string, WastTypeDef]),
    },
    syms: {
      wit: comp.syms.witSyms,
      internal: comp.syms.internal.map((e) => [e.uid, e.displayName] as [string, string]),
      local: comp.syms.local.map((e) => [e.uid, e.displayName] as [string, string]),
    },
  };
}

// ---------------------------------------------------------------------------
// Plugin loading
// ---------------------------------------------------------------------------

export interface SyntaxPlugin {
  toText(db: WastDb, syms: SymsData): string;
  fromText(text: string, existingDb: WastDb, syms: SymsData): { db: WastDb; syms: SymsData };
}

export async function loadTsLikePlugin(): Promise<SyntaxPlugin> {
  const mod = await import("./generated/ts-like/ts-like.js");
  const plugin = mod.syntaxPlugin;

  return {
    toText(db: WastDb, syms: SymsData): string {
      const comp = dbToWasmComponent(db, syms);
      return plugin.toText(comp);
    },

    fromText(text: string, existingDb: WastDb, syms: SymsData): { db: WastDb; syms: SymsData } {
      const existing = dbToWasmComponent(existingDb, syms);
      const result = plugin.fromText(text, existing);
      return wasmComponentToDb(result);
    },
  };
}

// ---------------------------------------------------------------------------
// File-manager WASM loader
// ---------------------------------------------------------------------------

export interface FileManager {
  /** Parse world.wit, populate funcs/types, and write wast.db + syms to disk. */
  bindgen(dirPath: string): void;
  /** Read a WastComponent from disk (wast.db + syms). */
  read(dirPath: string, targets?: Array<{ sym: string; includeCaller: boolean }>): { db: WastDb; syms: SymsData };
  /** Validate against world.wit and write a WastComponent to disk. */
  write(dirPath: string, db: WastDb, syms: SymsData): void;
  /** Validate partial component and merge into existing wast.db on disk. */
  merge(dirPath: string, partialDb: WastDb, partialSyms: SymsData): void;
}

export async function loadFileManager(): Promise<FileManager> {
  const mod = await import("./generated/file-manager/file-manager.js");
  const fm = mod.fileManager;

  return {
    bindgen(dirPath: string): void {
      fm.bindgen(dirPath);
    },

    read(dirPath: string, targets?: Array<{ sym: string; includeCaller: boolean }>): { db: WastDb; syms: SymsData } {
      const wasmTargets = targets?.map((t) => ({ sym: t.sym, includeCaller: t.includeCaller }));
      const comp = fm.read(dirPath, wasmTargets);
      return wasmComponentToDb(comp);
    },

    write(dirPath: string, db: WastDb, syms: SymsData): void {
      const comp = dbToWasmComponent(db, syms);
      fm.write(dirPath, comp);
    },

    merge(dirPath: string, partialDb: WastDb, partialSyms: SymsData): void {
      const partial = dbToWasmComponent(partialDb, partialSyms);
      fm.merge(dirPath, partial);
    },
  };
}

// ---------------------------------------------------------------------------
// Partial-manager WASM loader
// ---------------------------------------------------------------------------

export interface PartialManager {
  /** Extract a subset of functions from a full component. */
  extract(
    fullDb: WastDb,
    fullSyms: SymsData,
    targets: Array<{ sym: string; includeCaller: boolean }>,
  ): { db: WastDb; syms: SymsData };
  /** Merge a partial component into a full component. */
  merge(
    partialDb: WastDb,
    partialSyms: SymsData,
    fullDb: WastDb,
    fullSyms: SymsData,
  ): { db: WastDb; syms: SymsData };
}

export async function loadPartialManager(): Promise<PartialManager> {
  const mod = await import("./generated/partial-manager/partial-manager.js");
  const pm = mod.partialManager;

  return {
    extract(
      fullDb: WastDb,
      fullSyms: SymsData,
      targets: Array<{ sym: string; includeCaller: boolean }>,
    ): { db: WastDb; syms: SymsData } {
      const full = dbToWasmComponent(fullDb, fullSyms);
      const wasmTargets = targets.map((t) => ({ sym: t.sym, includeCaller: t.includeCaller }));
      const result = pm.extract(full, wasmTargets);
      return wasmComponentToDb(result);
    },

    merge(
      partialDb: WastDb,
      partialSyms: SymsData,
      fullDb: WastDb,
      fullSyms: SymsData,
    ): { db: WastDb; syms: SymsData } {
      const partial = dbToWasmComponent(partialDb, partialSyms);
      const full = dbToWasmComponent(fullDb, fullSyms);
      const result = pm.merge(partial, full);
      return wasmComponentToDb(result);
    },
  };
}
