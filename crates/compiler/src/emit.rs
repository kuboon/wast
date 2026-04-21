//! Component WAT text assembly.

use std::collections::HashMap;

use wast_pattern_analyzer::deserialize_body;
use wast_types::{FuncSource, WastDb, WastFuncRow};

use crate::core_emit::{FuncMap, emit_body, wit_abi_name, wit_to_core};
use crate::error::CompileError;

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

    if exports.is_empty() && internals.is_empty() && db.types.is_empty() {
        return Ok(wasi_cli_empty_run_wat().to_string());
    }

    // Func map for Call resolution — keyed on the source uid so
    // `Instruction::Call { func_uid }` matches whatever FuncSource stored.
    let func_map: FuncMap = db
        .funcs
        .iter()
        .map(|r| (source_key(&r.func.source).to_string(), &r.func))
        .collect::<HashMap<_, _>>();

    let mut core_body = String::new();
    for row in internals.iter().chain(exports.iter()) {
        core_body.push_str(&emit_core_func(row, &func_map)?);
    }

    let mut lifted_funcs = String::new();
    let mut component_exports = String::new();
    for row in &exports {
        let export_name = source_key(&row.func.source).to_string();
        let mangled = mangle(&export_name);

        let lifted_params = row
            .func
            .params
            .iter()
            .map(|(n, ty)| wit_abi_name(ty).map(|t| format!("(param \"{n}\" {t})")))
            .collect::<Result<Vec<_>, _>>()?
            .join(" ");
        let lifted_result = match &row.func.result {
            Some(ty) => format!("(result {})", wit_abi_name(ty)?),
            None => String::new(),
        };

        lifted_funcs.push_str(&format!(
            "  (func ${mangled}_lifted {lifted_params} {lifted_result}\n    (canon lift (core func $m \"{mangled}\")))\n",
        ));

        component_exports.push_str(&format!(
            "  (export \"{export_name}\" (func ${mangled}_lifted))\n",
        ));
    }

    let wat = format!(
        "(component\n  (core module $Mod\n{core_body}  )\n  (core instance $m (instantiate $Mod))\n{lifted_funcs}{component_exports})\n"
    );
    Ok(wat)
}

/// Emit a single core function definition for a WastDb row. Exported rows also
/// get `(export "name")` so the host component can alias them.
fn emit_core_func(row: &WastFuncRow, func_map: &FuncMap) -> Result<String, CompileError> {
    let name = source_key(&row.func.source);
    let mangled = mangle(name);

    let core_params = row
        .func
        .params
        .iter()
        .map(|(_, ty)| wit_to_core(ty).map(|t| format!("(param {t})")))
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    let core_result = match &row.func.result {
        Some(ty) => format!("(result {})", wit_to_core(ty)?),
        None => String::new(),
    };

    let body_instr = match &row.func.body {
        Some(bytes) if !bytes.is_empty() => deserialize_body(bytes)
            .map_err(|e| CompileError::InvalidInput(format!("body decode failed: {e}")))?,
        _ => Vec::new(),
    };
    let body_wat = emit_body(
        &body_instr,
        &row.func.params,
        row.func.result.as_deref(),
        func_map,
    )?;

    let export_clause = match row.func.source {
        FuncSource::Exported(_) => format!(" (export \"{mangled}\")"),
        _ => String::new(),
    };

    Ok(format!(
        "    (func ${mangled}{export_clause} {core_params} {core_result}\n{body_wat}    )\n"
    ))
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
