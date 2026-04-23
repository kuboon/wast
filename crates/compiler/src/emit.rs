//! Core module WAT assembly + component wrap via `wit-component`.
//!
//! v0.11 dropped hand-rolled `(component …)` outer shells, `canon lift`,
//! `canon lower`, and memory-option threading in favor of letting
//! `wit-component::ComponentEncoder` synthesize all of that from a core
//! module + embedded `component-type` custom section. We now only emit a
//! single `(module …)` and a generated WIT world string; wit-component
//! handles the Canonical ABI wiring for primitives, option/result, and
//! (future) string/list/record/variant.

use std::collections::HashMap;

use wast_pattern_analyzer::deserialize_body;
use wast_types::{FuncSource, WastDb, WastFuncRow};
use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::Resolve;

use crate::core_emit::{
    FuncMap, ResolvedType, TypeMap, body_needs_ret_ptr, collect_locals, emit_body, flat_slots,
    resolve_type, return_is_indirect,
};
use crate::error::CompileError;

/// Base offset for static data (string literals). Bump allocator starts its
/// heap at or past the end of the collected literals.
const STATIC_DATA_BASE: usize = 1024;

/// Memory + `cabi_realloc` infrastructure injected into every non-empty core
/// module. Bump allocator over a single memory page; `memory.copy` handles
/// realloc-grow. `heap_end` initial value is set past any string literals.
fn cabi_realloc_wat(heap_end_init: usize) -> String {
    format!(
        r#"  (memory (export "memory") 1)
  (global $heap_end (mut i32) (i32.const {heap_end_init}))
  (func $cabi_realloc (export "cabi_realloc")
    (param $orig_ptr i32) (param $orig_size i32) (param $align i32) (param $new_size i32)
    (result i32)
    (local $aligned i32)
    global.get $heap_end
    local.get $align
    i32.const 1
    i32.sub
    i32.add
    local.get $align
    i32.const 1
    i32.sub
    i32.const -1
    i32.xor
    i32.and
    local.tee $aligned
    local.get $new_size
    i32.add
    global.set $heap_end
    local.get $orig_size
    if
      local.get $aligned
      local.get $orig_ptr
      local.get $orig_size
      memory.copy
    end
    local.get $aligned
  )
"#
    )
}

/// Fixed Component binary for the v0 WASI CLI empty-run case. Kept as a
/// verbatim Component WAT (not synthesized via wit-component) because the
/// WASI CLI world exports `wasi:cli/run@0.2.0` with an inner-component
/// wrapping pattern that predates this compiler and is not worth generating.
pub fn wasi_cli_empty_run_wat() -> &'static str {
    r#"(component
  (core module $Mod
    (func (export "mod-main") (result i32)
      i32.const 0))
  (core instance $m (instantiate $Mod))
  (func $main_lifted (result (result))
    (canon lift (core func $m "mod-main")))
  (component $Comp
    (import "main" (func $g (result (result))))
    (export "run" (func $g)))
  (instance $c (instantiate $Comp
      (with "main" (func $main_lifted))))
  (export "wasi:cli/run@0.2.0" (instance $c)))
"#
}

/// Compile a `WastDb` into a WASM Component binary.
///
/// Empty input returns the fixed WASI CLI empty-run component (verbatim WAT,
/// then parsed to bytes). Otherwise:
///  1. Emit a core-only `(module …)` with memory + cabi_realloc + funcs
///  2. Synthesize a WIT world string from `db`'s exports/imports
///  3. Embed the `component-type` custom section
///  4. Wrap via `wit_component::ComponentEncoder`
pub fn compile_component(db: &WastDb, _world_wit: &str) -> Result<Vec<u8>, CompileError> {
    if db.funcs.is_empty() && db.types.is_empty() {
        return wat::parse_str(wasi_cli_empty_run_wat())
            .map_err(|e| CompileError::WatParse(e.to_string()));
    }

    let func_map: FuncMap = db
        .funcs
        .iter()
        .map(|r| (source_key(&r.func.source).to_string(), &r.func))
        .collect::<HashMap<_, _>>();
    let type_map: TypeMap = db
        .types
        .iter()
        .map(|r| (r.uid.clone(), &r.def.definition))
        .collect::<HashMap<_, _>>();

    let literal_table = collect_literal_table(db)?;
    let core_wat = emit_core_module(db, &func_map, &type_map, &literal_table)?;
    let mut core_bytes = wat::parse_str(&core_wat)
        .map_err(|e| CompileError::WatParse(format!("{e}\n--- WAT ---\n{core_wat}")))?;

    let wit_src = synthesize_world(db, &type_map)?;
    let mut resolve = Resolve::default();
    let pkg = resolve.push_str("generated.wit", &wit_src).map_err(|e| {
        CompileError::InvalidInput(format!("wit parse failed: {e}\n--- WIT ---\n{wit_src}"))
    })?;
    let world = resolve.select_world(pkg, None).map_err(|e| {
        CompileError::InvalidInput(format!(
            "wit select_world failed: {e}\n--- WIT ---\n{wit_src}"
        ))
    })?;

    embed_component_metadata(&mut core_bytes, &resolve, world, StringEncoding::UTF8)
        .map_err(|e| CompileError::InvalidInput(format!("embed_component_metadata failed: {e}")))?;

    ComponentEncoder::default()
        .validate(true)
        .module(&core_bytes)
        .map_err(|e| CompileError::InvalidInput(format!("ComponentEncoder::module failed: {e}")))?
        .encode()
        .map_err(|e| CompileError::InvalidInput(format!("ComponentEncoder::encode failed: {e}")))
}

/// Emit a single `(module …)` containing memory + realloc + imports +
/// data segments (string literals) + internal + exported funcs. Exports
/// use the unmangled WIT name so `wit-component` matches them to the WIT
/// world.
fn emit_core_module(
    db: &WastDb,
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
) -> Result<String, CompileError> {
    let mut imports = String::new();
    for row in db
        .funcs
        .iter()
        .filter(|r| matches!(r.func.source, FuncSource::Imported(_)))
    {
        let name = source_key(&row.func.source);
        let mangled = mangle(name);
        let core_params = flat_param_clauses(&row.func.params, type_map)?;
        let core_result = flat_result_clause(row.func.result.as_deref(), type_map)?;
        imports.push_str(&format!(
            "  (import \"$root\" \"{name}\" (func ${mangled} {core_params} {core_result}))\n"
        ));
    }

    let mut body_funcs = String::new();
    for row in db.funcs.iter().filter(|r| {
        matches!(
            r.func.source,
            FuncSource::Internal(_) | FuncSource::Exported(_)
        )
    }) {
        body_funcs.push_str(&emit_core_func(row, func_map, type_map, literal_table)?);
    }

    let heap_end_init = literal_table.heap_start.max(STATIC_DATA_BASE);
    let infra = cabi_realloc_wat(heap_end_init);
    let data_segments = emit_data_segments(literal_table);

    Ok(format!(
        "(module\n{imports}{infra}{data_segments}{body_funcs})\n"
    ))
}

/// Emit a single core function definition. Exported rows carry `(export
/// "name")` with the **unmangled** WIT name so wit-component can bind it.
/// The `$mangled` WAT identifier is used only for intra-module references.
fn emit_core_func(
    row: &WastFuncRow,
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
) -> Result<String, CompileError> {
    let name = source_key(&row.func.source);
    let mangled = mangle(name);

    let core_params = flat_param_clauses(&row.func.params, type_map)?;
    let core_result = flat_result_clause(row.func.result.as_deref(), type_map)?;

    let body_instr = match &row.func.body {
        Some(bytes) if !bytes.is_empty() => deserialize_body(bytes)
            .map_err(|e| CompileError::InvalidInput(format!("body decode failed: {e}")))?,
        _ => Vec::new(),
    };

    let locals = collect_locals(&body_instr, &row.func.params, func_map, type_map);

    let param_slot_count: usize = row
        .func
        .params
        .iter()
        .map(|(_, ty)| flat_slots(ty, type_map).map(|s| s.len()).unwrap_or(0))
        .sum();
    let local_slot_count: usize = locals
        .iter()
        .map(|(_, ty)| flat_slots(ty, type_map).map(|s| s.len()).unwrap_or(0))
        .sum();
    // Reserve a ret_ptr slot for either:
    //   (1) a body that contains Some/None/Ok/Err — variant ctor writes to it
    //   (2) a function whose return type is indirect (string/option/result) —
    //       the return-area wrap in emit_body uses it
    let return_indirect = row
        .func
        .result
        .as_deref()
        .map(|ty| return_is_indirect(ty, type_map).unwrap_or(false))
        .unwrap_or(false);
    let needs_ret_ptr = body_needs_ret_ptr(&body_instr) || return_indirect;
    let ret_ptr_slot = if needs_ret_ptr {
        Some(param_slot_count + local_slot_count)
    } else {
        None
    };

    let mut locals_parts: Vec<String> = locals
        .iter()
        .map(|(_, ty)| {
            flat_slots(ty, type_map).map(|ts| {
                ts.iter()
                    .map(|t| format!("(local {t})"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if needs_ret_ptr {
        locals_parts.push("(local i32)".to_string());
    }
    let locals_decl = locals_parts.join(" ");

    let body_wat = emit_body(
        &body_instr,
        &row.func.params,
        &locals,
        row.func.result.as_deref(),
        func_map,
        type_map,
        literal_table,
        ret_ptr_slot,
    )?;

    let export_clause = match &row.func.source {
        FuncSource::Exported(n) => format!(" (export \"{n}\")"),
        _ => String::new(),
    };

    let locals_clause = if locals_decl.is_empty() {
        String::new()
    } else {
        format!(" {locals_decl}")
    };

    Ok(format!(
        "  (func ${mangled}{export_clause} {core_params} {core_result}{locals_clause}\n{body_wat}  )\n"
    ))
}

/// Flatten params into core WAT `(param T)` clauses (one per flat slot).
fn flat_param_clauses(
    params: &[(String, String)],
    type_map: &TypeMap,
) -> Result<String, CompileError> {
    let mut parts = Vec::new();
    for (_, ty) in params {
        for slot in flat_slots(ty, type_map)? {
            parts.push(format!("(param {slot})"));
        }
    }
    Ok(parts.join(" "))
}

/// Flatten result into a core WAT `(result …)` clause. Indirect-return
/// types (flat > `MAX_FLAT_RESULTS`) collapse to a single `i32` pointer.
fn flat_result_clause(result: Option<&str>, type_map: &TypeMap) -> Result<String, CompileError> {
    match result {
        None => Ok(String::new()),
        Some(ty) => {
            if return_is_indirect(ty, type_map)? {
                Ok("(result i32)".to_string())
            } else {
                let slots = flat_slots(ty, type_map)?;
                Ok(format!("(result {})", slots.join(" ")))
            }
        }
    }
}

/// Synthesize a WIT world string from the db's exports + imports. Records
/// are emitted as named `type` declarations (WIT doesn't allow inline
/// anonymous records at use sites). Other compounds (option/result/list) are
/// inlined at the use site.
fn synthesize_world(db: &WastDb, type_map: &TypeMap) -> Result<String, CompileError> {
    let mut out = String::new();
    out.push_str("package wast:generated;\n\nworld generated {\n");

    // Type declarations: records must be named in WIT. Syntax:
    //   record NAME { field: ty, … }
    // (no `type NAME = record { … }` alias form — that's not valid WIT).
    for row in &db.types {
        if let wast_types::WitType::Record(fields) = &row.def.definition {
            let fields_wit = fields
                .iter()
                .map(|(fname, ftype)| {
                    format_wit_type(ftype, type_map).map(|t| format!("{fname}: {t}"))
                })
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            out.push_str(&format!(
                "  record {name} {{ {fields_wit} }}\n",
                name = wit_name(&row.uid)
            ));
        }
    }

    for row in &db.funcs {
        match &row.func.source {
            FuncSource::Exported(name) => {
                let sig = format_wit_sig(&row.func.params, row.func.result.as_deref(), type_map)?;
                out.push_str(&format!("  export {name}: {sig};\n"));
            }
            FuncSource::Imported(name) => {
                let sig = format_wit_sig(&row.func.params, row.func.result.as_deref(), type_map)?;
                out.push_str(&format!("  import {name}: {sig};\n"));
            }
            FuncSource::Internal(_) => {}
        }
    }

    out.push_str("}\n");
    Ok(out)
}

/// Normalize a WIT identifier — `_` → `-` (WIT is kebab-case, not snake).
fn wit_name(name: &str) -> String {
    name.replace('_', "-")
}

fn format_wit_sig(
    params: &[(String, String)],
    result: Option<&str>,
    type_map: &TypeMap,
) -> Result<String, CompileError> {
    let params_wit = params
        .iter()
        .map(|(n, ty)| format_wit_type(ty, type_map).map(|t| format!("{n}: {t}")))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let result_wit = match result {
        Some(ty) => format!(" -> {}", format_wit_type(ty, type_map)?),
        None => String::new(),
    };
    Ok(format!("func({params_wit}){result_wit}"))
}

/// Render a WIT type reference as WIT source syntax (as opposed to WAT's
/// parenthesized form). `i32` → `s32`, `i64` → `s64`, compound types use
/// the `name<args>` brackets.
fn format_wit_type(ty: &str, type_map: &TypeMap) -> Result<String, CompileError> {
    match resolve_type(ty, type_map)? {
        ResolvedType::Primitive(p) => Ok(match p.as_str() {
            "i32" => "s32".to_string(),
            "i64" => "s64".to_string(),
            other => other.to_string(),
        }),
        ResolvedType::String => Ok("string".to_string()),
        ResolvedType::List(inner) => Ok(format!("list<{}>", format_wit_type(&inner, type_map)?)),
        ResolvedType::Option(inner) => {
            Ok(format!("option<{}>", format_wit_type(&inner, type_map)?))
        }
        ResolvedType::Result(ok, err) => Ok(format!(
            "result<{}, {}>",
            format_wit_type(&ok, type_map)?,
            format_wit_type(&err, type_map)?
        )),
        // Records must be named (WIT disallows anonymous inline records at
        // use sites). `ty` is the uid declared via `type foo = record { … }`.
        ResolvedType::Record(_) => Ok(wit_name(ty)),
    }
}

/// Retrieve the uid-ish string from any FuncSource variant.
fn source_key(s: &FuncSource) -> &str {
    match s {
        FuncSource::Internal(n) | FuncSource::Imported(n) | FuncSource::Exported(n) => n,
    }
}

/// Turn an arbitrary WIT name (which may contain `-` or other chars WAT
/// identifiers disallow) into a WAT-safe identifier for internal references.
/// Export *strings* keep the original kebab form so WIT matching works.
pub(crate) fn mangle(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

// ---------------------------------------------------------------------------
// String literal collection & data segment emission
// ---------------------------------------------------------------------------

/// Static layout of every `Instruction::StringLiteral` encountered across all
/// function bodies. `offsets` maps byte-string → memory offset (dedup'd);
/// `heap_start` is the offset the bump allocator should begin at.
pub(crate) struct LiteralTable {
    pub offsets: std::collections::HashMap<Vec<u8>, usize>,
    pub heap_start: usize,
}

impl LiteralTable {
    pub fn get(&self, bytes: &[u8]) -> Option<usize> {
        self.offsets.get(bytes).copied()
    }
}

fn collect_literal_table(db: &WastDb) -> Result<LiteralTable, CompileError> {
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut seen_set: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

    for row in &db.funcs {
        let Some(bytes) = row.func.body.as_ref() else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let instrs = deserialize_body(bytes)
            .map_err(|e| CompileError::InvalidInput(format!("body decode failed: {e}")))?;
        collect_literals_rec(&instrs, &mut seen, &mut seen_set);
    }

    let mut offsets = std::collections::HashMap::new();
    let mut cur = STATIC_DATA_BASE;
    for lit in &seen {
        offsets.insert(lit.clone(), cur);
        cur += lit.len();
    }

    Ok(LiteralTable {
        offsets,
        heap_start: cur,
    })
}

fn collect_literals_rec(
    instrs: &[wast_pattern_analyzer::Instruction],
    out: &mut Vec<Vec<u8>>,
    seen: &mut std::collections::HashSet<Vec<u8>>,
) {
    use wast_pattern_analyzer::Instruction;
    for i in instrs {
        match i {
            Instruction::StringLiteral { bytes } => {
                if seen.insert(bytes.clone()) {
                    out.push(bytes.clone());
                }
            }
            Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
                collect_literals_rec(body, out, seen);
            }
            Instruction::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_literals_rec(std::slice::from_ref(condition.as_ref()), out, seen);
                collect_literals_rec(then_body, out, seen);
                collect_literals_rec(else_body, out, seen);
            }
            Instruction::BrIf { condition, .. } => {
                collect_literals_rec(std::slice::from_ref(condition.as_ref()), out, seen);
            }
            Instruction::Call { args, .. } => {
                for (_, arg) in args {
                    collect_literals_rec(std::slice::from_ref(arg), out, seen);
                }
            }
            Instruction::LocalSet { value, .. }
            | Instruction::Some { value }
            | Instruction::Ok { value }
            | Instruction::Err { value }
            | Instruction::IsErr { value }
            | Instruction::StringLen { value } => {
                collect_literals_rec(std::slice::from_ref(value.as_ref()), out, seen);
            }
            Instruction::Arithmetic { lhs, rhs, .. } | Instruction::Compare { lhs, rhs, .. } => {
                collect_literals_rec(std::slice::from_ref(lhs.as_ref()), out, seen);
                collect_literals_rec(std::slice::from_ref(rhs.as_ref()), out, seen);
            }
            Instruction::MatchOption {
                value,
                some_body,
                none_body,
                ..
            } => {
                collect_literals_rec(std::slice::from_ref(value.as_ref()), out, seen);
                collect_literals_rec(some_body, out, seen);
                collect_literals_rec(none_body, out, seen);
            }
            Instruction::MatchResult {
                value,
                ok_body,
                err_body,
                ..
            } => {
                collect_literals_rec(std::slice::from_ref(value.as_ref()), out, seen);
                collect_literals_rec(ok_body, out, seen);
                collect_literals_rec(err_body, out, seen);
            }
            _ => {}
        }
    }
}

fn emit_data_segments(table: &LiteralTable) -> String {
    let mut entries: Vec<(&Vec<u8>, &usize)> = table.offsets.iter().collect();
    entries.sort_by_key(|(_, off)| **off);
    let mut out = String::new();
    for (bytes, offset) in entries {
        let escaped: String = bytes.iter().map(|b| format!("\\{b:02x}")).collect();
        out.push_str(&format!("  (data (i32.const {offset}) \"{escaped}\")\n"));
    }
    out
}
