//! Component WAT text assembly.

use wast_pattern_analyzer::deserialize_body;
use wast_types::{FuncSource, WastDb, WastFuncRow};

use crate::core_emit::{emit_body, wit_abi_name, wit_to_core};
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
/// v0 smoke test keeps working). Otherwise every `FuncSource::Exported` func
/// becomes a top-level component export backed by a lifted core function.
pub fn compile_component(db: &WastDb, _world_wit: &str) -> Result<String, CompileError> {
    let exports: Vec<&WastFuncRow> = db
        .funcs
        .iter()
        .filter(|row| matches!(row.func.source, FuncSource::Exported(_)))
        .collect();

    if exports.is_empty() && db.types.is_empty() {
        return Ok(wasi_cli_empty_run_wat().to_string());
    }

    let mut core_funcs = String::new();
    let mut lifted_funcs = String::new();
    let mut exports_section = String::new();

    for row in &exports {
        let export_name = match &row.func.source {
            FuncSource::Exported(n) => n.clone(),
            _ => unreachable!(),
        };

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

        let body_wat = emit_body(&body_instr, &row.func.params, row.func.result.as_deref())?;

        core_funcs.push_str(&format!(
            "  (core module $Mod_{name}\n    (func (export \"{name}\") {params} {result}\n{body}    ))\n  (core instance $m_{name} (instantiate $Mod_{name}))\n",
            name = mangle(&export_name),
            params = core_params,
            result = core_result,
            body = body_wat,
        ));

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
            "  (func ${m}_lifted {params} {result}\n    (canon lift (core func $m_{m} \"{name}\")))\n",
            m = mangle(&export_name),
            name = mangle(&export_name),
            params = lifted_params,
            result = lifted_result,
        ));

        exports_section.push_str(&format!(
            "  (export \"{name}\" (func ${m}_lifted))\n",
            name = export_name,
            m = mangle(&export_name),
        ));
    }

    let wat = format!("(component\n{core_funcs}{lifted_funcs}{exports_section})\n");
    Ok(wat)
}

/// Turn an arbitrary WIT export name (which may contain `-` or other chars
/// WAT identifiers disallow) into a WAT-safe identifier.
fn mangle(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}
