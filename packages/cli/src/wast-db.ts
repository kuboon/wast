/**
 * Read/write wast.db JSON files directly.
 *
 * This mirrors the Rust serde_types in crates/file-manager/src/serde_types.rs.
 * The JSON format uses Rust-style tagged enums, e.g.:
 *   { "Internal": "f3a9" }
 *   { "Primitive": "U32" }
 */

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// ── Types matching Rust serde_types ──

export type PrimitiveType = "U32" | "U64" | "I32" | "I64" | "F32" | "F64" | "Bool" | "Char" | "String";

export type WitType =
  | { Primitive: PrimitiveType }
  | { Option: string }
  | { Result: [string, string] }
  | { List: string }
  | { Record: [string, string][] }
  | { Variant: [string, string | null][] }
  | { Tuple: string[] };

export type FuncSource =
  | { Internal: string }
  | { Imported: string }
  | { Exported: string };

export type TypeSource =
  | { Internal: string }
  | { Imported: string }
  | { Exported: string };

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

export interface WastDb {
  funcs: [string, WastFunc][];
  types: [string, WastTypeDef][];
}

// ── Read / Write ──

export function readWastDb(dir: string): WastDb {
  const dbPath = join(dir, "wast.db");
  const raw = readFileSync(dbPath, "utf-8");
  return JSON.parse(raw) as WastDb;
}

export function writeWastDb(dir: string, db: WastDb): void {
  const dbPath = join(dir, "wast.db");
  writeFileSync(dbPath, JSON.stringify(db, null, 2) + "\n");
}

// ── Formatting helpers ──

export function formatSource(source: FuncSource | TypeSource): string {
  if ("Internal" in source) return `internal(${source.Internal})`;
  if ("Imported" in source) return `imported(${source.Imported})`;
  if ("Exported" in source) return `exported(${source.Exported})`;
  return "unknown";
}

export function sourceKind(source: FuncSource | TypeSource): string {
  if ("Internal" in source) return "internal";
  if ("Imported" in source) return "imported";
  if ("Exported" in source) return "exported";
  return "unknown";
}

export function formatWitType(t: WitType): string {
  if ("Primitive" in t) return t.Primitive.toLowerCase();
  if ("Option" in t) return `option<${t.Option}>`;
  if ("Result" in t) return `result<${t.Result[0]}, ${t.Result[1]}>`;
  if ("List" in t) return `list<${t.List}>`;
  if ("Record" in t) return `record { ${t.Record.map(([k, v]) => `${k}: ${v}`).join(", ")} }`;
  if ("Variant" in t) return `variant { ${t.Variant.map(([k, v]) => v ? `${k}(${v})` : k).join(", ")} }`;
  if ("Tuple" in t) return `tuple<${t.Tuple.join(", ")}>`;
  return "unknown";
}

export function formatFunc(uid: string, func: WastFunc, symsLookup?: Map<string, string>): string {
  const lines: string[] = [];
  const displayName = symsLookup?.get(uid);
  const header = displayName ? `func ${uid} (${displayName})` : `func ${uid}`;
  lines.push(header);
  lines.push(`  source: ${formatSource(func.source)}`);

  if (func.params.length > 0) {
    const paramStrs = func.params.map(([pid, type_ref]) => {
      const pName = symsLookup?.get(pid);
      return pName ? `${pid}(${pName}): ${type_ref}` : `${pid}: ${type_ref}`;
    });
    lines.push(`  params: ${paramStrs.join(", ")}`);
  } else {
    lines.push("  params: (none)");
  }

  if (func.result) {
    lines.push(`  result: ${func.result}`);
  }

  if (func.body) {
    lines.push(`  body: ${func.body.length} bytes`);
  }

  return lines.join("\n");
}

export function formatTypeDef(uid: string, td: WastTypeDef): string {
  const lines: string[] = [];
  lines.push(`type ${uid}`);
  lines.push(`  source: ${formatSource(td.source)}`);
  lines.push(`  definition: ${formatWitType(td.definition)}`);
  return lines.join("\n");
}
