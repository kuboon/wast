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

/// Memory + `cabi_realloc` infrastructure injected into every non-empty core
/// module. Bump allocator over a single memory page starting at offset 1024;
/// `memory.copy` handles realloc-grow.
const CABI_REALLOC_WAT: &str = r#"  (memory (export "memory") 1)
  (global $heap_end (mut i32) (i32.const 1024))
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
"#;

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

    let core_wat = emit_core_module(db, &func_map, &type_map)?;
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
/// internal + exported funcs. Exports use the unmangled WIT name so
/// `wit-component` matches them to the WIT world.
fn emit_core_module(
    db: &WastDb,
    func_map: &FuncMap,
    type_map: &TypeMap,
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
        body_funcs.push_str(&emit_core_func(row, func_map, type_map)?);
    }

    Ok(format!(
        "(module\n{imports}{CABI_REALLOC_WAT}{body_funcs})\n"
    ))
}

/// Emit a single core function definition. Exported rows carry `(export
/// "name")` with the **unmangled** WIT name so wit-component can bind it.
/// The `$mangled` WAT identifier is used only for intra-module references.
fn emit_core_func(
    row: &WastFuncRow,
    func_map: &FuncMap,
    type_map: &TypeMap,
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
    let needs_ret_ptr = body_needs_ret_ptr(&body_instr);
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

/// Synthesize a WIT world string from the db's exports + imports. Type refs
/// are inlined (no `type` declarations) — `option<u32>` / `result<u32, u32>`
/// are emitted directly at each use site.
fn synthesize_world(db: &WastDb, type_map: &TypeMap) -> Result<String, CompileError> {
    let mut out = String::new();
    out.push_str("package wast:generated;\n\nworld generated {\n");

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
        ResolvedType::Option(inner) => {
            Ok(format!("option<{}>", format_wit_type(&inner, type_map)?))
        }
        ResolvedType::Result(ok, err) => Ok(format!(
            "result<{}, {}>",
            format_wit_type(&ok, type_map)?,
            format_wit_type(&err, type_map)?
        )),
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
