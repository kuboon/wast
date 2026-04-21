//! Core WASM instruction emission from WAST IR.
//!
//! Scope: numeric primitives (i32/i64/u32/u64/f32/f64/bool/char) with
//! `LocalGet`, `Const`, `Arithmetic`, `Compare`, `Return`, `Nop`. Later
//! stages will grow this module (calls, control flow, WIT types…).

use std::collections::HashMap;

use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};
use wast_types::WastFunc;

use crate::emit::mangle;
use crate::error::CompileError;

/// Map from a callable func's uid to its definition. Used by `emit_body` to
/// resolve `Instruction::Call` targets (param order + return type).
pub type FuncMap<'a> = HashMap<String, &'a WastFunc>;

/// Map a project-WIT primitive type name (`u32`/`i32`/…) to the WIT ABI
/// token accepted in a Component's lifted function signature. The project
/// uses `i32`/`i64` for signed integers; Component Model WIT spells them
/// `s32`/`s64`.
pub fn wit_abi_name(ty: &str) -> Result<&'static str, CompileError> {
    match ty {
        "i32" => Ok("s32"),
        "i64" => Ok("s64"),
        "u32" | "u64" | "f32" | "f64" | "bool" | "char" | "string" => Ok(match ty {
            "u32" => "u32",
            "u64" => "u64",
            "f32" => "f32",
            "f64" => "f64",
            "bool" => "bool",
            "char" => "char",
            "string" => "string",
            _ => unreachable!(),
        }),
        _ => Err(CompileError::Unsupported(format!(
            "WIT type {ty} is not supported yet"
        ))),
    }
}

/// Map a WIT primitive type name (e.g. `"u32"`) to the core WASM type token
/// used in WAT text (`"i32"`, `"i64"`, `"f32"`, `"f64"`).
pub fn wit_to_core(ty: &str) -> Result<&'static str, CompileError> {
    match ty {
        "u32" | "i32" | "bool" | "char" => Ok("i32"),
        "u64" | "i64" => Ok("i64"),
        "f32" => Ok("f32"),
        "f64" => Ok("f64"),
        _ => Err(CompileError::Unsupported(format!(
            "WIT type {ty} is not supported yet"
        ))),
    }
}

fn is_float(ty: &str) -> bool {
    matches!(ty, "f32" | "f64")
}

fn is_signed(ty: &str) -> bool {
    matches!(ty, "i32" | "i64")
}

/// Emit the core WAT body text for a function.
///
/// `params` is the ordered list of `(name, wit_type)` pairs matching
/// `LocalGet { uid }` lookups. `result_ty` is the function's return type
/// (used as the default inference hint for ambiguous top-level `Const`).
/// `func_map` resolves `Instruction::Call` target uids to their signatures.
pub fn emit_body(
    instructions: &[Instruction],
    params: &[(String, String)],
    result_ty: Option<&str>,
    func_map: &FuncMap,
) -> Result<String, CompileError> {
    let mut out = String::new();
    for instr in instructions {
        emit_instr(instr, result_ty, params, func_map, &mut out)?;
    }
    Ok(out)
}

/// Emit a single instruction. `expected` is the WIT type the consumer wants
/// on the stack — used only to disambiguate free-floating `Const` values.
fn emit_instr(
    instr: &Instruction,
    expected: Option<&str>,
    params: &[(String, String)],
    func_map: &FuncMap,
    out: &mut String,
) -> Result<(), CompileError> {
    match instr {
        Instruction::Nop => out.push_str("      nop\n"),
        Instruction::Return => out.push_str("      return\n"),
        Instruction::LocalGet { uid } => {
            let idx = params.iter().position(|(n, _)| n == uid).ok_or_else(|| {
                CompileError::InvalidInput(format!(
                    "LocalGet references unknown local {uid:?}; only params are supported"
                ))
            })?;
            out.push_str(&format!("      local.get {idx}\n"));
        }
        Instruction::Const { value } => {
            let ty = expected.unwrap_or("i32");
            let core = wit_to_core(ty)?;
            if is_float(ty) {
                // `value` is i64 in the IR — cast for float const. Lossy for
                // values beyond f64 mantissa, but the IR currently has no
                // richer float literal representation.
                out.push_str(&format!("      {core}.const {}\n", *value as f64));
            } else {
                out.push_str(&format!("      {core}.const {value}\n"));
            }
        }
        Instruction::Arithmetic { op, lhs, rhs } => {
            let ty = expected
                .map(str::to_string)
                .or_else(|| infer_wit_type(lhs, params, func_map))
                .or_else(|| infer_wit_type(rhs, params, func_map))
                .unwrap_or_else(|| "i32".to_string());
            emit_instr(lhs, Some(&ty), params, func_map, out)?;
            emit_instr(rhs, Some(&ty), params, func_map, out)?;
            out.push_str(&format!("      {}\n", arith_op(op.clone(), &ty)?));
        }
        Instruction::Compare { op, lhs, rhs } => {
            // Compare pushes i32 regardless of operand type, so `expected`
            // only tells us the final boolean width (always i32) — operand
            // type must be inferred from the operands themselves.
            let operand_ty = infer_wit_type(lhs, params, func_map)
                .or_else(|| infer_wit_type(rhs, params, func_map))
                .unwrap_or_else(|| "i32".to_string());
            emit_instr(lhs, Some(&operand_ty), params, func_map, out)?;
            emit_instr(rhs, Some(&operand_ty), params, func_map, out)?;
            out.push_str(&format!("      {}\n", compare_op(op.clone(), &operand_ty)?));
        }
        Instruction::Call { func_uid, args } => {
            let target = func_map.get(func_uid).ok_or_else(|| {
                CompileError::InvalidInput(format!("Call references unknown func {func_uid:?}"))
            })?;
            // Push args in the target's declared param order (callers can
            // supply them in any order since args are name-keyed pairs).
            for (pname, pty) in &target.params {
                let (_, arg_instr) = args.iter().find(|(n, _)| n == pname).ok_or_else(|| {
                    CompileError::InvalidInput(format!(
                        "Call to {func_uid:?} missing arg for param {pname:?}"
                    ))
                })?;
                emit_instr(arg_instr, Some(pty), params, func_map, out)?;
            }
            out.push_str(&format!("      call ${}\n", mangle(func_uid)));
        }
        other => {
            return Err(CompileError::Unsupported(format!(
                "instruction {other:?} not yet supported"
            )));
        }
    }
    Ok(())
}

/// Best-effort WIT-type inference for an instruction's stack result.
/// Returns `None` when the type is ambiguous (e.g. bare `Const`).
fn infer_wit_type(
    instr: &Instruction,
    params: &[(String, String)],
    func_map: &FuncMap,
) -> Option<String> {
    match instr {
        Instruction::LocalGet { uid } => params
            .iter()
            .find(|(n, _)| n == uid)
            .map(|(_, ty)| ty.clone()),
        Instruction::Arithmetic { lhs, rhs, .. } => {
            infer_wit_type(lhs, params, func_map).or_else(|| infer_wit_type(rhs, params, func_map))
        }
        Instruction::Compare { .. } => Some("bool".to_string()),
        Instruction::IsErr { .. } => Some("bool".to_string()),
        Instruction::Call { func_uid, .. } => func_map.get(func_uid).and_then(|f| f.result.clone()),
        _ => None,
    }
}

fn arith_op(op: ArithOp, ty: &str) -> Result<String, CompileError> {
    let core = wit_to_core(ty)?;
    let name = match op {
        ArithOp::Add => "add",
        ArithOp::Sub => "sub",
        ArithOp::Mul => "mul",
        ArithOp::Div => {
            if is_float(ty) {
                "div"
            } else if is_signed(ty) {
                "div_s"
            } else {
                "div_u"
            }
        }
    };
    Ok(format!("{core}.{name}"))
}

fn compare_op(op: CompareOp, ty: &str) -> Result<String, CompileError> {
    let core = wit_to_core(ty)?;
    let float = is_float(ty);
    let signed = is_signed(ty);
    let name = match op {
        CompareOp::Eq => "eq",
        CompareOp::Ne => "ne",
        CompareOp::Lt => {
            if float {
                "lt"
            } else if signed {
                "lt_s"
            } else {
                "lt_u"
            }
        }
        CompareOp::Le => {
            if float {
                "le"
            } else if signed {
                "le_s"
            } else {
                "le_u"
            }
        }
        CompareOp::Gt => {
            if float {
                "gt"
            } else if signed {
                "gt_s"
            } else {
                "gt_u"
            }
        }
        CompareOp::Ge => {
            if float {
                "ge"
            } else if signed {
                "ge_s"
            } else {
                "ge_u"
            }
        }
    };
    Ok(format!("{core}.{name}"))
}
