//! Core WASM instruction emission from WAST IR.
//!
//! Scope: numeric primitives (i32/i64/u32/u64/f32/f64/bool/char) with
//! `LocalGet`/`LocalSet`, `Const`, `Arithmetic`, `Compare`, `Call`,
//! `If`/`Block`/`Loop`/`Br`/`BrIf`, `Return`, `Nop`, `IsErr` on result
//! params, option/result-in-param via the Canonical ABI flat layout,
//! option/result **return** with primitive payload via indirect return
//! through the bump allocator (`cabi_realloc`), and `MatchOption` /
//! `MatchResult` for destructuring compound params.
//! Later stages will add strings, lists, records, variants.

use std::collections::HashMap;

use wast_pattern_analyzer::{ArithOp, CompareOp, Instruction};
use wast_types::{WastFunc, WitType};

use crate::emit::{LiteralTable, mangle};
use crate::error::CompileError;

/// Map from a callable func's uid to its definition. Used by `emit_body` to
/// resolve `Instruction::Call` targets (param order + return type).
pub type FuncMap<'a> = HashMap<String, &'a WastFunc>;

/// Map from a type uid to its structural definition (for resolving
/// `option<…>`, `result<…,…>`, etc. that appear as param or result refs).
pub type TypeMap<'a> = HashMap<String, &'a WitType>;

/// Structural view of a WIT type reference (resolved through a `TypeMap`).
/// Scope: primitives (numeric + bool/char), string (ptr+len pair),
/// list<T> (ptr+len pair where len is element count), and option/result
/// with primitive payload.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    Primitive(String),
    String,
    List(String),
    Option(String),
    Result(String, String),
}

pub fn resolve_type(ty_ref: &str, type_map: &TypeMap) -> Result<ResolvedType, CompileError> {
    if ty_ref == "string" {
        return Ok(ResolvedType::String);
    }
    if is_known_primitive(ty_ref) {
        return Ok(ResolvedType::Primitive(ty_ref.to_string()));
    }
    let def = type_map
        .get(ty_ref)
        .ok_or_else(|| CompileError::InvalidInput(format!("unknown type reference {ty_ref:?}")))?;
    match def {
        WitType::Primitive(p) => {
            let name = primitive_name(p);
            if name == "string" {
                Ok(ResolvedType::String)
            } else {
                Ok(ResolvedType::Primitive(name.to_string()))
            }
        }
        WitType::List(inner) => Ok(ResolvedType::List(inner.clone())),
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
        "u32" | "u64" | "i32" | "i64" | "f32" | "f64" | "bool" | "char"
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
/// primitive → 1 slot; option<P>/result<A,B>/string/list<T> → 2 slots.
pub fn flat_slots(ty_ref: &str, type_map: &TypeMap) -> Result<Vec<&'static str>, CompileError> {
    match resolve_type(ty_ref, type_map)? {
        ResolvedType::Primitive(p) => Ok(vec![wit_to_core(&p)?]),
        ResolvedType::String => Ok(vec!["i32", "i32"]),
        ResolvedType::List(_) => Ok(vec!["i32", "i32"]),
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

/// Canonical-ABI byte size and alignment for a WIT type. Driven by the
/// compile-time layout; see <https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md#alignment>.
pub fn size_align(ty_ref: &str, type_map: &TypeMap) -> Result<(usize, usize), CompileError> {
    match resolve_type(ty_ref, type_map)? {
        ResolvedType::Primitive(p) => Ok(match p.as_str() {
            "bool" => (1, 1),
            "u32" | "i32" | "f32" | "char" => (4, 4),
            "u64" | "i64" | "f64" => (8, 8),
            other => {
                return Err(CompileError::Unsupported(format!(
                    "size_align for primitive {other}"
                )));
            }
        }),
        ResolvedType::String => Ok((8, 4)),
        ResolvedType::List(_) => Ok((8, 4)),
        ResolvedType::Option(inner) => variant_layout(&[Some(inner)], type_map),
        ResolvedType::Result(ok, err) => variant_layout(&[Some(ok), Some(err)], type_map),
    }
}

/// Layout of a variant with the given case payload types (`None` = no payload).
/// For option/result the discriminant fits in `u8` (only two cases).
fn variant_layout(
    cases: &[Option<String>],
    type_map: &TypeMap,
) -> Result<(usize, usize), CompileError> {
    // Discriminant: u8 for ≤256 cases (all our current compounds).
    let disc_size = 1usize;
    let disc_align = 1usize;
    let mut case_align = disc_align;
    let mut case_size = 0usize;
    for case in cases {
        if let Some(ty) = case {
            let (s, a) = size_align(ty, type_map)?;
            case_align = case_align.max(a);
            case_size = case_size.max(s);
        }
    }
    let case_start = align_up(disc_size, case_align);
    let align = case_align;
    let size = align_up(case_start + case_size, align);
    Ok((size, align))
}

pub fn align_up(offset: usize, align: usize) -> usize {
    if align == 0 {
        offset
    } else {
        (offset + align - 1) & !(align - 1)
    }
}

/// Number of core flat result slots: primitives → 1; option/result → 2.
pub const MAX_FLAT_RESULTS: usize = 1;

/// True when the WIT return type exceeds `MAX_FLAT_RESULTS` and therefore
/// requires indirect return (core func returns a single `i32` pointer).
pub fn return_is_indirect(ty: &str, type_map: &TypeMap) -> Result<bool, CompileError> {
    Ok(flat_slots(ty, type_map)?.len() > MAX_FLAT_RESULTS)
}

/// Core store instruction + natural-alignment power-of-two for a primitive.
/// `bool` stores a single byte via `i32.store8`. Integers and floats use
/// their natural width.
pub fn store_op(ty: &str) -> Result<(&'static str, u32), CompileError> {
    Ok(match ty {
        "bool" => ("i32.store8", 0),
        "u32" | "i32" | "char" => ("i32.store", 2),
        "u64" | "i64" => ("i64.store", 3),
        "f32" => ("f32.store", 2),
        "f64" => ("f64.store", 3),
        other => {
            return Err(CompileError::Unsupported(format!("store op for {other}")));
        }
    })
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
            Instruction::MatchOption {
                value,
                some_binding,
                some_body,
                none_body,
            } => {
                collect_locals_rec(
                    std::slice::from_ref(value.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
                if !seen.contains(some_binding) {
                    if let Some(opt_ty) = infer_wit_type(value, params, func_map, type_map)
                        && let Ok(ResolvedType::Option(inner)) = resolve_type(&opt_ty, type_map)
                    {
                        seen.insert(some_binding.clone());
                        locals.push((some_binding.clone(), inner));
                    }
                }
                collect_locals_rec(some_body, params, func_map, type_map, locals, seen);
                collect_locals_rec(none_body, params, func_map, type_map, locals, seen);
            }
            Instruction::MatchResult {
                value,
                ok_binding,
                ok_body,
                err_binding,
                err_body,
            } => {
                collect_locals_rec(
                    std::slice::from_ref(value.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
                if let Some(res_ty) = infer_wit_type(value, params, func_map, type_map)
                    && let Ok(ResolvedType::Result(ok, err)) = resolve_type(&res_ty, type_map)
                {
                    if !seen.contains(ok_binding) {
                        seen.insert(ok_binding.clone());
                        locals.push((ok_binding.clone(), ok));
                    }
                    if !seen.contains(err_binding) {
                        seen.insert(err_binding.clone());
                        locals.push((err_binding.clone(), err));
                    }
                }
                collect_locals_rec(ok_body, params, func_map, type_map, locals, seen);
                collect_locals_rec(err_body, params, func_map, type_map, locals, seen);
            }
            Instruction::Some { value }
            | Instruction::Ok { value }
            | Instruction::Err { value } => {
                collect_locals_rec(
                    std::slice::from_ref(value.as_ref()),
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
/// `ret_ptr_slot` is the core-local index of the synthesized `i32` that
/// holds the return buffer pointer for indirect-return functions; `None`
/// when the function doesn't need it.
pub fn emit_body(
    instructions: &[Instruction],
    params: &[(String, String)],
    locals: &[(String, String)],
    result_ty: Option<&str>,
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
) -> Result<String, CompileError> {
    let scope: Vec<(String, String)> = params.iter().chain(locals.iter()).cloned().collect();
    let mut out = String::new();

    // If the function returns `string` or `list<T>`, the last body
    // instruction is expected to produce that value (LocalGet of the
    // corresponding local, or a literal). Wrap it in an indirect-return
    // sequence — allocate 8 bytes, store (ptr, len), push the buffer ptr.
    let needs_ptrlen_wrap = result_ty
        .map(|ty| {
            matches!(
                resolve_type(ty, type_map),
                Ok(ResolvedType::String) | Ok(ResolvedType::List(_))
            )
        })
        .unwrap_or(false);

    let (init, last) = if needs_ptrlen_wrap {
        instructions
            .split_last()
            .map(|(last, init)| (init, Some(last)))
            .unwrap_or((instructions, None))
    } else {
        (instructions, None)
    };

    for instr in init {
        emit_instr(
            instr,
            result_ty,
            &scope,
            func_map,
            type_map,
            literal_table,
            ret_ptr_slot,
            &mut out,
        )?;
    }

    if let Some(last) = last {
        emit_ptrlen_return_wrap(
            last,
            &scope,
            type_map,
            literal_table,
            ret_ptr_slot,
            &mut out,
        )?;
    }

    Ok(out)
}

/// Wrap a (ptr, len)-producing instruction (for string or list return) into
/// an indirect-return sequence: allocate an 8-byte buffer via `cabi_realloc`,
/// write (data_ptr, len) to offsets 0 and 4, then push the buffer pointer.
fn emit_ptrlen_return_wrap(
    instr: &Instruction,
    scope: &[(String, String)],
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    let ret_ptr = ret_ptr_slot.ok_or_else(|| {
        CompileError::Unsupported(
            "ret_ptr_slot missing for indirect (ptr,len) return (collect_locals should have reserved one)".into(),
        )
    })?;

    // Allocate 8-byte return area for the (ptr, len) struct.
    out.push_str(&format!(
        "      i32.const 0\n      i32.const 0\n      i32.const 4\n      i32.const 8\n      call $cabi_realloc\n      local.set {ret_ptr}\n"
    ));

    match instr {
        Instruction::LocalGet { uid } => {
            let (first_idx, _, ty) = slot_info(uid, scope, type_map)?;
            match resolve_type(ty, type_map)? {
                ResolvedType::String | ResolvedType::List(_) => {}
                other => {
                    return Err(CompileError::InvalidInput(format!(
                        "(ptr,len) return wrap applied to non-string/list local {uid:?} \
                         (resolved as {other:?})"
                    )));
                }
            }
            out.push_str(&format!(
                "      local.get {ret_ptr}\n      local.get {first_idx}\n      i32.store offset=0 align=2\n"
            ));
            out.push_str(&format!(
                "      local.get {ret_ptr}\n      local.get {}\n      i32.store offset=4 align=2\n",
                first_idx + 1
            ));
        }
        Instruction::StringLiteral { bytes } => {
            let offset = literal_table.get(bytes).ok_or_else(|| {
                CompileError::InvalidInput(
                    "StringLiteral missing from literal table (collector bug)".into(),
                )
            })?;
            out.push_str(&format!(
                "      local.get {ret_ptr}\n      i32.const {offset}\n      i32.store offset=0 align=2\n"
            ));
            out.push_str(&format!(
                "      local.get {ret_ptr}\n      i32.const {}\n      i32.store offset=4 align=2\n",
                bytes.len()
            ));
        }
        other => {
            return Err(CompileError::Unsupported(format!(
                "(ptr,len) return from {other:?} not supported yet \
                 (v0.14/v0.15 handles LocalGet or StringLiteral)"
            )));
        }
    }

    // Push the buffer pointer as the core function's return value.
    out.push_str(&format!("      local.get {ret_ptr}\n"));
    Ok(())
}

/// Scan a body for any compound constructor (`Some`/`None`/`Ok`/`Err`) that
/// would need the synthesized return-pointer local.
pub fn body_needs_ret_ptr(body: &[Instruction]) -> bool {
    body.iter().any(instr_has_compound_ctor)
}

fn instr_has_compound_ctor(i: &Instruction) -> bool {
    match i {
        Instruction::Some { .. }
        | Instruction::None
        | Instruction::Ok { .. }
        | Instruction::Err { .. } => true,
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            instr_has_compound_ctor(condition)
                || then_body.iter().any(instr_has_compound_ctor)
                || else_body.iter().any(instr_has_compound_ctor)
        }
        Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
            body.iter().any(instr_has_compound_ctor)
        }
        Instruction::BrIf { condition, .. } => instr_has_compound_ctor(condition),
        Instruction::Call { args, .. } => args.iter().any(|(_, a)| instr_has_compound_ctor(a)),
        Instruction::Arithmetic { lhs, rhs, .. } | Instruction::Compare { lhs, rhs, .. } => {
            instr_has_compound_ctor(lhs) || instr_has_compound_ctor(rhs)
        }
        Instruction::LocalSet { value, .. } => instr_has_compound_ctor(value),
        _ => false,
    }
}

/// Emit a single instruction. `expected` is the WIT type the consumer wants
/// on the stack — used only to disambiguate free-floating `Const` values and
/// to resolve compound constructors (`Some`/`None`/`Ok`/`Err`) that write to
/// the return-pointer slot.
fn emit_instr(
    instr: &Instruction,
    expected: Option<&str>,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    match instr {
        Instruction::Nop => out.push_str("      nop\n"),
        Instruction::Return => out.push_str("      return\n"),
        Instruction::LocalGet { uid } => {
            let (first_idx, slots, _) = slot_info(uid, scope, type_map)?;
            for i in 0..slots.len() {
                out.push_str(&format!("      local.get {}\n", first_idx + i));
            }
        }
        Instruction::Const { value } => {
            let ty = expected.unwrap_or("i32");
            let core = wit_to_core(ty)?;
            if is_float(ty) {
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
            emit_instr(
                lhs,
                Some(&ty),
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            emit_instr(
                rhs,
                Some(&ty),
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            out.push_str(&format!("      {}\n", arith_op(op.clone(), &ty)?));
        }
        Instruction::Compare { op, lhs, rhs } => {
            let operand_ty = infer_wit_type(lhs, scope, func_map, type_map)
                .or_else(|| infer_wit_type(rhs, scope, func_map, type_map))
                .unwrap_or_else(|| "i32".to_string());
            emit_instr(
                lhs,
                Some(&operand_ty),
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            emit_instr(
                rhs,
                Some(&operand_ty),
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
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
                emit_instr(
                    arg_instr,
                    Some(pty),
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
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
            emit_instr(
                value,
                Some(&ty_owned),
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            out.push_str(&format!("      local.set {first_idx}\n"));
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            emit_instr(
                condition,
                None,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            let emit_typed = expected.is_some() && !else_body.is_empty();
            let result_clause = if emit_typed {
                format!(" (result {})", wit_to_core(expected.unwrap())?)
            } else {
                String::new()
            };
            out.push_str(&format!("      if{result_clause}\n"));
            let child_expected = if emit_typed { expected } else { None };
            for i in then_body {
                emit_instr(
                    i,
                    child_expected,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            if !else_body.is_empty() {
                out.push_str("      else\n");
                for i in else_body {
                    emit_instr(
                        i,
                        child_expected,
                        scope,
                        func_map,
                        type_map,
                        literal_table,
                        ret_ptr_slot,
                        out,
                    )?;
                }
            }
            out.push_str("      end\n");
        }
        Instruction::Block { label, body } => {
            let lbl = label_clause(label.as_deref());
            out.push_str(&format!("      block{lbl}\n"));
            for i in body {
                emit_instr(
                    i,
                    None,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      end\n");
        }
        Instruction::Loop { label, body } => {
            let lbl = label_clause(label.as_deref());
            out.push_str(&format!("      loop{lbl}\n"));
            for i in body {
                emit_instr(
                    i,
                    None,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      end\n");
        }
        Instruction::Br { label } => {
            out.push_str(&format!("      br ${}\n", mangle(label)));
        }
        Instruction::BrIf { label, condition } => {
            emit_instr(
                condition,
                None,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            out.push_str(&format!("      br_if ${}\n", mangle(label)));
        }
        Instruction::IsErr { value } => {
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
                    out.push_str(&format!("      local.get {first_idx}\n"));
                }
                other => {
                    return Err(CompileError::InvalidInput(format!(
                        "IsErr applied to non-result local {uid:?} (resolved as {other:?})"
                    )));
                }
            }
        }
        Instruction::StringLen { value } => {
            match value.as_ref() {
                // Compile-time folding: byte length of a literal is known now.
                Instruction::StringLiteral { bytes } => {
                    out.push_str(&format!("      i32.const {}\n", bytes.len()));
                }
                // v0.12: inline read of a string local's `len` slot.
                Instruction::LocalGet { uid } => {
                    let (first_idx, _, ty) = slot_info(uid, scope, type_map)?;
                    match resolve_type(ty, type_map)? {
                        ResolvedType::String => {
                            out.push_str(&format!("      local.get {}\n", first_idx + 1));
                        }
                        other => {
                            return Err(CompileError::InvalidInput(format!(
                                "StringLen applied to non-string local {uid:?} \
                                 (resolved as {other:?})"
                            )));
                        }
                    }
                }
                _ => {
                    return Err(CompileError::Unsupported(
                        "StringLen only supports LocalGet of a string local or a \
                         StringLiteral value for now"
                            .into(),
                    ));
                }
            }
        }
        Instruction::ListLen { value } => {
            // v0.15: read the element count from the list local's `len` slot.
            let uid = match value.as_ref() {
                Instruction::LocalGet { uid } => uid,
                _ => {
                    return Err(CompileError::Unsupported(
                        "ListLen only supports LocalGet of a list local for now".into(),
                    ));
                }
            };
            let (first_idx, _, ty) = slot_info(uid, scope, type_map)?;
            match resolve_type(ty, type_map)? {
                ResolvedType::List(_) => {
                    out.push_str(&format!("      local.get {}\n", first_idx + 1));
                }
                other => {
                    return Err(CompileError::InvalidInput(format!(
                        "ListLen applied to non-list local {uid:?} (resolved as {other:?})"
                    )));
                }
            }
        }
        Instruction::Some { value } => {
            emit_variant_ctor(
                expected,
                1,
                Some(value.as_ref()),
                ExpectedKind::Option,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
        }
        Instruction::None => {
            emit_variant_ctor(
                expected,
                0,
                None,
                ExpectedKind::Option,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
        }
        Instruction::Ok { value } => {
            emit_variant_ctor(
                expected,
                0,
                Some(value.as_ref()),
                ExpectedKind::ResultOk,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
        }
        Instruction::Err { value } => {
            emit_variant_ctor(
                expected,
                1,
                Some(value.as_ref()),
                ExpectedKind::ResultErr,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
        }
        Instruction::MatchOption {
            value,
            some_binding,
            some_body,
            none_body,
        } => {
            // Emit value → pushes [disc, payload]. Save payload into
            // `some_binding`'s local slot so LocalGet(some_binding) in
            // some_body reads it. `none_body` inherits a zero (or prior)
            // value there — caller responsibility to respect scoping.
            emit_instr(
                value,
                None,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;

            let (bind_idx, bind_slots, _) = slot_info(some_binding, scope, type_map)?;
            if bind_slots.len() != 1 {
                return Err(CompileError::Unsupported(format!(
                    "MatchOption binding {some_binding:?} of compound type not supported yet"
                )));
            }
            out.push_str(&format!("      local.set {bind_idx}\n"));

            let (result_clause, child_expected) = branch_result_clause(expected, type_map)?;
            out.push_str(&format!("      if{result_clause}\n"));
            for i in some_body {
                emit_instr(
                    i,
                    child_expected,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      else\n");
            for i in none_body {
                emit_instr(
                    i,
                    child_expected,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      end\n");
        }
        Instruction::MatchResult {
            value,
            ok_binding,
            ok_body,
            err_binding,
            err_body,
        } => {
            // Emit value → pushes [disc, payload]. Canonical-ABI flat-join
            // makes the payload a single core type; if ok/err have matching
            // core types we can `tee` payload into both bindings in one go.
            let (ok_idx, ok_slots, ok_ty) = slot_info(ok_binding, scope, type_map)?;
            let (err_idx, err_slots, err_ty) = slot_info(err_binding, scope, type_map)?;
            if ok_slots.len() != 1 || err_slots.len() != 1 {
                return Err(CompileError::Unsupported(
                    "MatchResult binding of compound type not supported yet".into(),
                ));
            }
            if wit_to_core(ok_ty)? != wit_to_core(err_ty)? {
                return Err(CompileError::Unsupported(format!(
                    "MatchResult with heterogeneous ok/err core types \
                     (ok={ok_ty}, err={err_ty}) not supported yet"
                )));
            }
            emit_instr(
                value,
                None,
                scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                out,
            )?;
            // Seed both bindings: tee copies to one while keeping on stack.
            out.push_str(&format!("      local.tee {err_idx}\n"));
            out.push_str(&format!("      local.set {ok_idx}\n"));

            // disc != 0 → err_body, disc == 0 → ok_body.
            let (result_clause, child_expected) = branch_result_clause(expected, type_map)?;
            out.push_str(&format!("      if{result_clause}\n"));
            for i in err_body {
                emit_instr(
                    i,
                    child_expected,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      else\n");
            for i in ok_body {
                emit_instr(
                    i,
                    child_expected,
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
            }
            out.push_str("      end\n");
        }
        Instruction::StringLiteral { bytes } => {
            // Literal bytes live in a pre-allocated data segment assigned by
            // `collect_literal_table`. Push (ptr, len) as two i32 consts.
            let offset = literal_table.get(bytes).ok_or_else(|| {
                CompileError::InvalidInput(
                    "StringLiteral missing from literal table (collector bug)".into(),
                )
            })?;
            out.push_str(&format!(
                "      i32.const {offset}\n      i32.const {}\n",
                bytes.len()
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ExpectedKind {
    Option,
    ResultOk,
    ResultErr,
}

#[allow(clippy::too_many_arguments)]
fn emit_variant_ctor(
    expected: Option<&str>,
    disc: u8,
    value: Option<&Instruction>,
    kind: ExpectedKind,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    let expected_ty = expected.ok_or_else(|| {
        CompileError::Unsupported(
            "compound constructors (Some/None/Ok/Err) are only supported at return position".into(),
        )
    })?;
    let payload_ty = match (resolve_type(expected_ty, type_map)?, kind) {
        (ResolvedType::Option(inner), ExpectedKind::Option) => Some(inner),
        (ResolvedType::Result(ok, _), ExpectedKind::ResultOk) => Some(ok),
        (ResolvedType::Result(_, err), ExpectedKind::ResultErr) => Some(err),
        (rt, _) => {
            return Err(CompileError::InvalidInput(format!(
                "variant ctor {kind:?} in context of type {expected_ty:?} (resolved {rt:?})"
            )));
        }
    };
    let (size, align) = size_align(expected_ty, type_map)?;
    let ret_ptr = ret_ptr_slot.ok_or_else(|| {
        CompileError::Unsupported(
            "ret_ptr_slot missing; collect_locals should have reserved one".into(),
        )
    })?;

    // Allocate return area: realloc(0, 0, align, size) → ptr, stash in ret_ptr.
    out.push_str(&format!(
        "      i32.const 0\n      i32.const 0\n      i32.const {align}\n      i32.const {size}\n      call $cabi_realloc\n      local.set {ret_ptr}\n"
    ));

    // Store disc at offset 0 (u8).
    out.push_str(&format!(
        "      local.get {ret_ptr}\n      i32.const {disc}\n      i32.store8 offset=0\n"
    ));

    // Store payload if present. Some/None's payload-less case skips entirely;
    // Result cases always have a payload in our IR.
    if let (Some(v), Some(pty)) = (value, payload_ty.as_deref()) {
        let (_, pay_align) = size_align(pty, type_map)?;
        let payload_offset = align_up(1, pay_align);
        let (store, align_pow2) = store_op(pty)?;
        out.push_str(&format!("      local.get {ret_ptr}\n"));
        emit_instr(
            v,
            Some(pty),
            scope,
            func_map,
            type_map,
            literal_table,
            ret_ptr_slot,
            out,
        )?;
        out.push_str(&format!(
            "      {store} offset={payload_offset} align={align_pow2}\n"
        ));
    }

    // Return area pointer — this is the function's return value.
    out.push_str(&format!("      local.get {ret_ptr}\n"));

    Ok(())
}

impl std::fmt::Debug for ExpectedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpectedKind::Option => f.write_str("Option"),
            ExpectedKind::ResultOk => f.write_str("ResultOk"),
            ExpectedKind::ResultErr => f.write_str("ResultErr"),
        }
    }
}

fn label_clause(label: Option<&str>) -> String {
    match label {
        Some(l) if !l.is_empty() => format!(" ${}", mangle(l)),
        _ => String::new(),
    }
}

/// Helper for `if …` clauses inside `MatchOption`/`MatchResult`. Returns
/// `(result_clause, child_expected)` — a typed form with propagated
/// expected type when the consumer wants a value, plain statement form
/// otherwise. Compound consumer types use `i32` (indirect-return ptr).
fn branch_result_clause<'a>(
    expected: Option<&'a str>,
    type_map: &TypeMap,
) -> Result<(String, Option<&'a str>), CompileError> {
    match expected {
        None => Ok((String::new(), None)),
        Some(ty) => {
            let core = match resolve_type(ty, type_map)? {
                ResolvedType::Primitive(p) => wit_to_core(&p)?.to_string(),
                // Compound & string both use indirect return (single i32 ptr).
                ResolvedType::String
                | ResolvedType::List(_)
                | ResolvedType::Option(_)
                | ResolvedType::Result(_, _) => "i32".to_string(),
            };
            Ok((format!(" (result {core})"), Some(ty)))
        }
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
        Instruction::StringLen { .. } => Some("u32".to_string()),
        Instruction::StringLiteral { .. } => Some("string".to_string()),
        Instruction::ListLen { .. } => Some("u32".to_string()),
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
