//! Component WAT text assembly.

use std::collections::HashMap;

use wast_pattern_analyzer::deserialize_body;
use wast_types::{FuncSource, WastDb, WastFuncRow};

use crate::core_emit::{
    FuncMap, TypeMap, body_needs_ret_ptr, collect_locals, emit_body, flat_slots, lifted_type_wat,
    return_is_indirect,
};
use crate::error::CompileError;

/// Memory + `cabi_realloc` infrastructure injected into every non-empty core
/// module. Exports `memory` and `cabi_realloc`, which `canon lift`/`canon
/// lower` reference by name. The allocator is a simple bump allocator over
/// a single memory page starting at offset 1024 (leaving room for future
/// static data / stack). `memory.copy` (bulk-memory) handles realloc grows.
const CABI_REALLOC_WAT: &str = r#"    (memory (export "memory") 1)
    (global $heap_end (mut i32) (i32.const 1024))
    (func $cabi_realloc (export "cabi_realloc")
      (param $orig_ptr i32) (param $orig_size i32) (param $align i32) (param $new_size i32)
      (result i32)
      (local $aligned i32)
      ;; aligned = (heap_end + align - 1) & ~(align - 1)
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
      ;; heap_end = aligned + new_size
      local.get $new_size
      i32.add
      global.set $heap_end
      ;; Copy old bytes if this is a realloc-grow
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

/// Canon lift/lower options that tie a lifted or lowered func to the core
/// module's memory + realloc. Always emitted when we're past the empty-run
/// special case so strings/lists/compound returns work uniformly.
const CANON_OPTS: &str = "\n    (memory $m \"memory\")\n    (realloc (func $m \"cabi_realloc\"))";

/// Fixed Component WAT for the v0 WASI CLI empty-run case.
///
/// The outer component exposes the `wasi:cli/run@0.2.0` instance; its single
/// export `run` returns `result<_, _>` and we always return `ok` (discriminant
/// `0`) via the core function `mod-main`. No imports, no memory, no realloc.
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

/// Build the full Component WAT for a `WastDb`.
///
/// Empty input falls back to the v0 WASI CLI empty-run fixed component (so the
/// v0 smoke test keeps working). Otherwise all internal + exported funcs land
/// in a single core module (so they can `call` each other by name), and every
/// `FuncSource::Exported` row also becomes a top-level component export.
pub fn compile_component(db: &WastDb, _world_wit: &str) -> Result<String, CompileError> {
    let exports: Vec<&WastFuncRow> = db
        .funcs
        .iter()
        .filter(|r| matches!(r.func.source, FuncSource::Exported(_)))
        .collect();
    let internals: Vec<&WastFuncRow> = db
        .funcs
        .iter()
        .filter(|r| matches!(r.func.source, FuncSource::Internal(_)))
        .collect();
    let imports: Vec<&WastFuncRow> = db
        .funcs
        .iter()
        .filter(|r| matches!(r.func.source, FuncSource::Imported(_)))
        .collect();

    if exports.is_empty() && internals.is_empty() && imports.is_empty() && db.types.is_empty() {
        return Ok(wasi_cli_empty_run_wat().to_string());
    }

    // Func map for Call resolution — keyed on the source uid so
    // `Instruction::Call { func_uid }` matches whatever FuncSource stored.
    let func_map: FuncMap = db
        .funcs
        .iter()
        .map(|r| (source_key(&r.func.source).to_string(), &r.func))
        .collect::<HashMap<_, _>>();

    // Type map for resolving param/result type references (primitive vs
    // option<T>/result<T,E>).
    let type_map: TypeMap = db
        .types
        .iter()
        .map(|r| (r.uid.clone(), &r.def.definition))
        .collect::<HashMap<_, _>>();

    // Component-level imports + canon lower + core module imports.
    // Each imported func appears at three layers:
    //   (1) component-level `(import "name" (func $name_comp …))`
    //   (2) core func `(core func $name (canon lower (func $name_comp)))`
    //   (3) core module `(import "imports" "name" (func $name …))`
    // and the core instantiation plumbs (2) into (3) via `with "imports" …`.
    let mut comp_imports = String::new();
    let mut core_lowers = String::new();
    let mut core_module_imports = String::new();
    let mut with_exports: Vec<String> = Vec::new();

    for row in &imports {
        let name = source_key(&row.func.source).to_string();
        let mangled = mangle(&name);

        let comp_params = row
            .func
            .params
            .iter()
            .map(|(n, ty)| lifted_type_wat(ty, &type_map).map(|t| format!("(param \"{n}\" {t})")))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        let comp_result = match &row.func.result {
            Some(ty) => format!("(result {})", lifted_type_wat(ty, &type_map)?),
            None => String::new(),
        };
        comp_imports.push_str(&format!(
            "  (import \"{name}\" (func ${mangled}_comp {comp_params} {comp_result}))\n"
        ));

        // canon lower can't reference $m (the core instance) because the core
        // instance is created AFTER these lowerings — circular reference.
        // For primitive-only imports no memory is needed; compound imports
        // will need a two-module split (allocator module separate) — deferred.
        core_lowers.push_str(&format!(
            "  (core func ${mangled} (canon lower (func ${mangled}_comp)))\n"
        ));

        let core_params = flat_param_clauses(&row.func.params, &type_map)?;
        let core_result = flat_result_clause(row.func.result.as_deref(), &type_map)?;
        core_module_imports.push_str(&format!(
            "    (import \"imports\" \"{mangled}\" (func ${mangled} {core_params} {core_result}))\n"
        ));

        with_exports.push(format!("      (export \"{mangled}\" (func ${mangled}))"));
    }

    let mut core_body = String::new();
    for row in internals.iter().chain(exports.iter()) {
        core_body.push_str(&emit_core_func(row, &func_map, &type_map)?);
    }

    let instantiate_clause = if with_exports.is_empty() {
        "(instantiate $Mod)".to_string()
    } else {
        format!(
            "(instantiate $Mod\n    (with \"imports\" (instance\n{}\n    )))",
            with_exports.join("\n")
        )
    };

    let mut lifted_funcs = String::new();
    let mut component_exports = String::new();
    for row in &exports {
        let export_name = source_key(&row.func.source).to_string();
        let mangled = mangle(&export_name);

        let lifted_params = row
            .func
            .params
            .iter()
            .map(|(n, ty)| lifted_type_wat(ty, &type_map).map(|t| format!("(param \"{n}\" {t})")))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        let lifted_result = match &row.func.result {
            Some(ty) => format!("(result {})", lifted_type_wat(ty, &type_map)?),
            None => String::new(),
        };

        lifted_funcs.push_str(&format!(
            "  (func ${mangled}_lifted {lifted_params} {lifted_result}\n    (canon lift (core func $m \"{mangled}\"){CANON_OPTS}))\n",
        ));

        component_exports.push_str(&format!(
            "  (export \"{export_name}\" (func ${mangled}_lifted))\n",
        ));
    }

    let wat = format!(
        "(component\n{comp_imports}{core_lowers}  (core module $Mod\n{core_module_imports}{CABI_REALLOC_WAT}{core_body}  )\n  (core instance $m {instantiate_clause})\n{lifted_funcs}{component_exports})\n"
    );
    Ok(wat)
}

/// Emit a single core function definition for a WastDb row. Exported rows also
/// get `(export "name")` so the host component can alias them.
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

    // Compute core slot positions so `ret_ptr_slot` lands right after all
    // user-declared params+locals.
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

    let export_clause = match row.func.source {
        FuncSource::Exported(_) => format!(" (export \"{mangled}\")"),
        _ => String::new(),
    };

    let locals_clause = if locals_decl.is_empty() {
        String::new()
    } else {
        format!(" {locals_decl}")
    };

    Ok(format!(
        "    (func ${mangled}{export_clause} {core_params} {core_result}{locals_clause}\n{body_wat}    )\n"
    ))
}

/// Flatten params into core WAT `(param T)` clauses (one per slot).
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

/// Flatten result into a core WAT `(result …)` clause.
///
/// For types within `MAX_FLAT_RESULTS` the clause is the full flat list; for
/// types that exceed it (e.g. option/result with payload) the core function
/// uses indirect return — a single `i32` pointer to caller-allocated (for
/// lower) or callee-allocated (for lift) memory.
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

/// Retrieve the uid-ish string from any FuncSource variant.
fn source_key(s: &FuncSource) -> &str {
    match s {
        FuncSource::Internal(n) | FuncSource::Imported(n) | FuncSource::Exported(n) => n,
    }
}

/// Turn an arbitrary WIT export name (which may contain `-` or other chars
/// WAT identifiers disallow) into a WAT-safe identifier.
pub(crate) fn mangle(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}
