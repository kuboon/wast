//! Core WASM instruction emission from WAST IR.
//!
//! Scope: numeric primitives (i32/i64/u32/u64/f32/f64/bool/char) with
//! `LocalGet`/`LocalSet`, `Const`, `Arithmetic`, `Compare`, `Call`,
//! `If`/`Block`/`Loop`/`Br`/`BrIf`, `Return`, `Nop`, `IsErr` on result
//! params, and option/result-in-param via the Canonical ABI flat layout.
//! Later stages will add option/result returns (needs `cabi_realloc`),
//! strings, lists, records, variants.

use std::collections::HashMap;

use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};
use wast_types::{WastFunc, WitType};

use crate::emit::mangle;
use crate::error::CompileError;

/// Map from a callable func's uid to its definition. Used by `emit_body` to
/// resolve `Instruction::Call` targets (param order + return type).
pub type FuncMap<'a> = HashMap<String, &'a WastFunc>;

/// Map from a type uid to its structural definition (for resolving
/// `option<…>`, `result<…,…>`, etc. that appear as param or result refs).
pub type TypeMap<'a> = HashMap<String, &'a WitType>;

/// Structural view of a WIT type reference (resolved through a `TypeMap`).
/// v0.6 scope: primitives + option<prim> + result<prim, prim>.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    Primitive(String),
    Option(String),
    Result(String, String),
}

pub fn resolve_type(ty_ref: &str, type_map: &TypeMap) -> Result<ResolvedType, CompileError> {
    if is_known_primitive(ty_ref) {
        return Ok(ResolvedType::Primitive(ty_ref.to_string()));
    }
    let def = type_map
        .get(ty_ref)
        .ok_or_else(|| CompileError::InvalidInput(format!("unknown type reference {ty_ref:?}")))?;
    match def {
        WitType::Primitive(p) => Ok(ResolvedType::Primitive(primitive_name(p).to_string())),
        WitType::Option(inner) => Ok(ResolvedType::Option(inner.clone())),
        WitType::Result(ok, err) => Ok(ResolvedType::Result(ok.clone(), err.clone())),
        other => Err(CompileError::Unsupported(format!(
            "type {ty_ref:?} with definition {other:?} is not supported yet"
        ))),
    }
}

fn is_known_primitive(name: &str) -> bool {
    matches!(
        name,
        "u32" | "u64" | "i32" | "i64" | "f32" | "f64" | "bool" | "char" | "string"
    )
}

fn primitive_name(p: &wast_types::PrimitiveType) -> &'static str {
    use wast_types::PrimitiveType as P;
    match p {
        P::U32 => "u32",
        P::U64 => "u64",
        P::I32 => "i32",
        P::I64 => "i64",
        P::F32 => "f32",
        P::F64 => "f64",
        P::Bool => "bool",
        P::Char => "char",
        P::String => "string",
    }
}

/// Core-flat slot types for a WIT type reference (one core type per slot).
/// primitive → 1 slot; option<P>/result<A,B> → 2 slots (i32 disc + joined payload).
pub fn flat_slots(ty_ref: &str, type_map: &TypeMap) -> Result<Vec<&'static str>, CompileError> {
    match resolve_type(ty_ref, type_map)? {
        ResolvedType::Primitive(p) => Ok(vec![wit_to_core(&p)?]),
        ResolvedType::Option(inner) => {
            let payload = wit_to_core(&inner)?;
            Ok(vec!["i32", payload])
        }
        ResolvedType::Result(ok, err) => {
            // v0.6: both payloads required. Pick the wider/float-compatible
            // join — restricted to matching core types for simplicity.
            let ok_core = wit_to_core(&ok)?;
            let err_core = wit_to_core(&err)?;
            let payload = join_core_type(ok_core, err_core)?;
            Ok(vec!["i32", payload])
        }
    }
}

fn join_core_type(a: &'static str, b: &'static str) -> Result<&'static str, CompileError> {
    // Canonical ABI flat-join, restricted to v0.6 primitive payloads:
    if a == b {
        return Ok(a);
    }
    // i32/i64 widening
    match (a, b) {
        ("i32", "i64") | ("i64", "i32") => Ok("i64"),
        ("f32", "f64") | ("f64", "f32") => Ok("f64"),
        // i32↔f32 reinterpret as i32
        ("i32", "f32") | ("f32", "i32") => Ok("i32"),
        // i64↔f64 reinterpret as i64
        ("i64", "f64") | ("f64", "i64") => Ok("i64"),
        _ => Err(CompileError::Unsupported(format!(
            "flat-join of core types {a:?} and {b:?} not supported yet"
        ))),
    }
}

/// Component-Model-level WAT type string for a WIT type reference.
/// E.g. `u32` → `u32`, `i32` → `s32`, `option<u32>` → `(option u32)`,
/// `result<u32, u32>` → `(result u32 (error u32))`.
pub fn lifted_type_wat(ty_ref: &str, type_map: &TypeMap) -> Result<String, CompileError> {
    match resolve_type(ty_ref, type_map)? {
        ResolvedType::Primitive(p) => Ok(wit_abi_name(&p)?.to_string()),
        ResolvedType::Option(inner) => Ok(format!("(option {})", wit_abi_name(&inner)?)),
        ResolvedType::Result(ok, err) => Ok(format!(
            "(result {} (error {}))",
            wit_abi_name(&ok)?,
            wit_abi_name(&err)?
        )),
    }
}

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

/// Locals introduced by `LocalSet` instructions (in first-assignment order),
/// paired with their inferred WIT type.
pub fn collect_locals(
    body: &[Instruction],
    params: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
) -> Vec<(String, String)> {
    let mut locals: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> =
        params.iter().map(|(n, _)| n.clone()).collect();
    collect_locals_rec(body, params, func_map, type_map, &mut locals, &mut seen);
    locals
}

fn collect_locals_rec(
    body: &[Instruction],
    params: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    locals: &mut Vec<(String, String)>,
    seen: &mut std::collections::HashSet<String>,
) {
    for instr in body {
        match instr {
            Instruction::LocalSet { uid, value } => {
                if !seen.contains(uid) {
                    let ty = infer_wit_type(value, params, func_map, type_map)
                        .unwrap_or_else(|| "i32".to_string());
                    seen.insert(uid.clone());
                    locals.push((uid.clone(), ty));
                }
                collect_locals_rec(
                    std::slice::from_ref(value.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
            }
            Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
                collect_locals_rec(body, params, func_map, type_map, locals, seen);
            }
            Instruction::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_locals_rec(
                    std::slice::from_ref(condition.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
                collect_locals_rec(then_body, params, func_map, type_map, locals, seen);
                collect_locals_rec(else_body, params, func_map, type_map, locals, seen);
            }
            Instruction::BrIf { condition, .. } => {
                collect_locals_rec(
                    std::slice::from_ref(condition.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
            }
            Instruction::Call { args, .. } => {
                for (_, arg) in args {
                    collect_locals_rec(
                        std::slice::from_ref(arg),
                        params,
                        func_map,
                        type_map,
                        locals,
                        seen,
                    );
                }
            }
            Instruction::Arithmetic { lhs, rhs, .. } | Instruction::Compare { lhs, rhs, .. } => {
                collect_locals_rec(
                    std::slice::from_ref(lhs.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
                collect_locals_rec(
                    std::slice::from_ref(rhs.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
            }
            _ => {}
        }
    }
}

/// Look up the core-slot layout of a named local in a scope that mixes
/// params and locals. Returns `(first_slot_idx, flat_core_types, wit_type_ref)`.
fn slot_info<'a>(
    uid: &str,
    scope: &'a [(String, String)],
    type_map: &TypeMap,
) -> Result<(usize, Vec<&'static str>, &'a str), CompileError> {
    let mut cur = 0usize;
    for (name, ty) in scope {
        let slots = flat_slots(ty, type_map)?;
        if name == uid {
            return Ok((cur, slots, ty.as_str()));
        }
        cur += slots.len();
    }
    Err(CompileError::InvalidInput(format!("unknown local {uid:?}")))
}

/// Emit the core WAT body text for a function.
///
/// `params` is the ordered list of `(name, wit_type_ref)` pairs matching
/// `LocalGet { uid }` lookups. `result_ty` is the function's return type
/// (used as the default inference hint for ambiguous top-level `Const`).
/// `func_map` resolves `Instruction::Call` target uids to their signatures.
/// `locals` are declared locals (after params) that `LocalSet` can assign.
/// `type_map` resolves compound types (option/result) referenced from
/// params/locals so compound slot layouts can be computed.
pub fn emit_body(
    instructions: &[Instruction],
    params: &[(String, String)],
    locals: &[(String, String)],
    result_ty: Option<&str>,
    func_map: &FuncMap,
    type_map: &TypeMap,
) -> Result<String, CompileError> {
    let scope: Vec<(String, String)> = params.iter().chain(locals.iter()).cloned().collect();
    let mut out = String::new();
    for instr in instructions {
        emit_instr(instr, result_ty, &scope, func_map, type_map, &mut out)?;
    }
    Ok(out)
}

/// Emit a single instruction. `expected` is the WIT type the consumer wants
/// on the stack — used only to disambiguate free-floating `Const` values.
fn emit_instr(
    instr: &Instruction,
    expected: Option<&str>,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    out: &mut String,
) -> Result<(), CompileError> {
    match instr {
        Instruction::Nop => out.push_str("      nop\n"),
        Instruction::Return => out.push_str("      return\n"),
        Instruction::LocalGet { uid } => {
            let (first_idx, slots, _) = slot_info(uid, scope, type_map)?;
            // For compound locals (option/result) this pushes disc then payload.
            for i in 0..slots.len() {
                out.push_str(&format!("      local.get {}\n", first_idx + i));
            }
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
                .or_else(|| infer_wit_type(lhs, scope, func_map, type_map))
                .or_else(|| infer_wit_type(rhs, scope, func_map, type_map))
                .unwrap_or_else(|| "i32".to_string());
            emit_instr(lhs, Some(&ty), scope, func_map, type_map, out)?;
            emit_instr(rhs, Some(&ty), scope, func_map, type_map, out)?;
            out.push_str(&format!("      {}\n", arith_op(op.clone(), &ty)?));
        }
        Instruction::Compare { op, lhs, rhs } => {
            // Compare pushes i32 regardless of operand type, so `expected`
            // only tells us the final boolean width (always i32) — operand
            // type must be inferred from the operands themselves.
            let operand_ty = infer_wit_type(lhs, scope, func_map, type_map)
                .or_else(|| infer_wit_type(rhs, scope, func_map, type_map))
                .unwrap_or_else(|| "i32".to_string());
            emit_instr(lhs, Some(&operand_ty), scope, func_map, type_map, out)?;
            emit_instr(rhs, Some(&operand_ty), scope, func_map, type_map, out)?;
            out.push_str(&format!("      {}\n", compare_op(op.clone(), &operand_ty)?));
        }
        Instruction::Call { func_uid, args } => {
            let target = func_map.get(func_uid).ok_or_else(|| {
                CompileError::InvalidInput(format!("Call references unknown func {func_uid:?}"))
            })?;
            for (pname, pty) in &target.params {
                let (_, arg_instr) = args.iter().find(|(n, _)| n == pname).ok_or_else(|| {
                    CompileError::InvalidInput(format!(
                        "Call to {func_uid:?} missing arg for param {pname:?}"
                    ))
                })?;
                emit_instr(arg_instr, Some(pty), scope, func_map, type_map, out)?;
            }
            out.push_str(&format!("      call ${}\n", mangle(func_uid)));
        }
        Instruction::LocalSet { uid, value } => {
            let (first_idx, slots, ty) = slot_info(uid, scope, type_map)?;
            if slots.len() != 1 {
                return Err(CompileError::Unsupported(format!(
                    "LocalSet on compound local {uid:?} not supported yet"
                )));
            }
            let ty_owned = ty.to_string();
            emit_instr(value, Some(&ty_owned), scope, func_map, type_map, out)?;
            out.push_str(&format!("      local.set {first_idx}\n"));
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            emit_instr(condition, None, scope, func_map, type_map, out)?;
            let emit_typed = expected.is_some() && !else_body.is_empty();
            let result_clause = if emit_typed {
                format!(" (result {})", wit_to_core(expected.unwrap())?)
            } else {
                String::new()
            };
            out.push_str(&format!("      if{result_clause}\n"));
            let child_expected = if emit_typed { expected } else { None };
            for i in then_body {
                emit_instr(i, child_expected, scope, func_map, type_map, out)?;
            }
            if !else_body.is_empty() {
                out.push_str("      else\n");
                for i in else_body {
                    emit_instr(i, child_expected, scope, func_map, type_map, out)?;
                }
            }
            out.push_str("      end\n");
        }
        Instruction::Block { label, body } => {
            let lbl = label_clause(label.as_deref());
            out.push_str(&format!("      block{lbl}\n"));
            for i in body {
                emit_instr(i, None, scope, func_map, type_map, out)?;
            }
            out.push_str("      end\n");
        }
        Instruction::Loop { label, body } => {
            let lbl = label_clause(label.as_deref());
            out.push_str(&format!("      loop{lbl}\n"));
            for i in body {
                emit_instr(i, None, scope, func_map, type_map, out)?;
            }
            out.push_str("      end\n");
        }
        Instruction::Br { label } => {
            out.push_str(&format!("      br ${}\n", mangle(label)));
        }
        Instruction::BrIf { label, condition } => {
            emit_instr(condition, None, scope, func_map, type_map, out)?;
            out.push_str(&format!("      br_if ${}\n", mangle(label)));
        }
        Instruction::IsErr { value } => {
            // v0.6: restrict to `LocalGet(result_param)`; generalizing requires
            // knowing the static type of arbitrary sub-expressions.
            let uid = match value.as_ref() {
                Instruction::LocalGet { uid } => uid,
                _ => {
                    return Err(CompileError::Unsupported(
                        "IsErr only supports LocalGet of a result local for now".into(),
                    ));
                }
            };
            let (first_idx, _, ty) = slot_info(uid, scope, type_map)?;
            match resolve_type(ty, type_map)? {
                ResolvedType::Result(_, _) => {
                    // disc is at first_idx; a disc != 0 means `err`.
                    out.push_str(&format!("      local.get {first_idx}\n"));
                }
                other => {
                    return Err(CompileError::InvalidInput(format!(
                        "IsErr applied to non-result local {uid:?} (resolved as {other:?})"
                    )));
                }
            }
        }
        other => {
            return Err(CompileError::Unsupported(format!(
                "instruction {other:?} not yet supported"
            )));
        }
    }
    Ok(())
}

fn label_clause(label: Option<&str>) -> String {
    match label {
        Some(l) if !l.is_empty() => format!(" ${}", mangle(l)),
        _ => String::new(),
    }
}

/// Best-effort WIT-type inference for an instruction's stack result.
/// Returns `None` when the type is ambiguous (e.g. bare `Const`).
fn infer_wit_type(
    instr: &Instruction,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
) -> Option<String> {
    match instr {
        Instruction::LocalGet { uid } => scope
            .iter()
            .find(|(n, _)| n == uid)
            .map(|(_, ty)| ty.clone()),
        Instruction::Arithmetic { lhs, rhs, .. } => infer_wit_type(lhs, scope, func_map, type_map)
            .or_else(|| infer_wit_type(rhs, scope, func_map, type_map)),
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
