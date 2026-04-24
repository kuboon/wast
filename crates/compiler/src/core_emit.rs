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
/// list<T> (ptr+len pair where len is element count), option/result
/// with primitive payload, and record with primitive fields.
#[derive(Debug, Clone)]
pub enum ResolvedType {
    Primitive(String),
    String,
    List(String),
    Option(String),
    Result(String, String),
    Record(Vec<(String, String)>),
    /// General variant: `Vec<(case_name, optional_payload_type)>`.
    /// `option<T>` and `result<T,E>` remain their own variants for
    /// backward compatibility with existing IR nodes (Some/None/Ok/Err).
    Variant(Vec<(String, Option<String>)>),
    /// Anonymous positional record. Fields indexed by position.
    Tuple(Vec<String>),
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
        WitType::Record(fields) => Ok(ResolvedType::Record(fields.clone())),
        WitType::Variant(cases) => Ok(ResolvedType::Variant(cases.clone())),
        WitType::Tuple(elems) => Ok(ResolvedType::Tuple(elems.clone())),
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
        ResolvedType::Record(fields) => {
            // Flat form of a record is the concatenation of its fields' flats.
            let mut out = Vec::new();
            for (_, ftype) in fields {
                out.extend(flat_slots(&ftype, type_map)?);
            }
            Ok(out)
        }
        ResolvedType::Variant(cases) => {
            // disc slot + payload flat-join across all cases. Each case's
            // payload flattens independently; we merge slot-by-slot, widening
            // per slot using the same rules as Result's flat-join.
            let mut joined: Vec<&'static str> = Vec::new();
            for (_, payload_ty) in &cases {
                if let Some(pty) = payload_ty {
                    let case_slots = flat_slots(pty, type_map)?;
                    for (idx, s) in case_slots.iter().enumerate() {
                        if idx < joined.len() {
                            joined[idx] = join_core_type(joined[idx], s)?;
                        } else {
                            joined.push(s);
                        }
                    }
                }
            }
            let mut out = vec!["i32"]; // disc
            out.extend(joined);
            Ok(out)
        }
        ResolvedType::Tuple(elems) => {
            // Same rule as record: concat of elements' flats.
            let mut out = Vec::new();
            for ty in &elems {
                out.extend(flat_slots(ty, type_map)?);
            }
            Ok(out)
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
        ResolvedType::Record(fields) => record_layout(&fields, type_map),
        ResolvedType::Variant(cases) => {
            // Reuse the option/result layout helper: it already takes
            // per-case payload types and computes the right size+align.
            let payloads: Vec<Option<String>> = cases.iter().map(|(_, p)| p.clone()).collect();
            variant_layout(&payloads, type_map)
        }
        ResolvedType::Tuple(elems) => {
            // Tuple layout is the same as a record with positional field
            // names. Reuse record_layout via a synthesized name list.
            let synthetic: Vec<(String, String)> = elems
                .iter()
                .enumerate()
                .map(|(i, t)| (i.to_string(), t.clone()))
                .collect();
            record_layout(&synthetic, type_map)
        }
    }
}

/// Canonical ABI layout for a record — each field starts at an aligned
/// offset, total size is padded to the record's alignment.
fn record_layout(
    fields: &[(String, String)],
    type_map: &TypeMap,
) -> Result<(usize, usize), CompileError> {
    let mut size = 0usize;
    let mut align = 1usize;
    for (_, ftype) in fields {
        let (fs, fa) = size_align(ftype, type_map)?;
        size = align_up(size, fa) + fs;
        align = align.max(fa);
    }
    size = align_up(size, align);
    Ok((size, align))
}

/// Byte offset of a named field within a record, plus its WIT type ref.
pub fn record_field_info(
    fields: &[(String, String)],
    field_name: &str,
    type_map: &TypeMap,
) -> Result<(usize, String), CompileError> {
    let mut offset = 0usize;
    for (fname, ftype) in fields {
        let (fs, fa) = size_align(ftype, type_map)?;
        offset = align_up(offset, fa);
        if fname == field_name {
            return Ok((offset, ftype.clone()));
        }
        offset += fs;
    }
    Err(CompileError::InvalidInput(format!(
        "field {field_name:?} not found in record"
    )))
}

/// Flat-slot offset of a named field within a record's concatenated flat form.
pub fn record_field_slot_info(
    fields: &[(String, String)],
    field_name: &str,
    type_map: &TypeMap,
) -> Result<(usize, Vec<&'static str>), CompileError> {
    let mut slot_offset = 0usize;
    for (fname, ftype) in fields {
        let slots = flat_slots(ftype, type_map)?;
        if fname == field_name {
            return Ok((slot_offset, slots));
        }
        slot_offset += slots.len();
    }
    Err(CompileError::InvalidInput(format!(
        "field {field_name:?} not found in record"
    )))
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

/// Core store instruction + natural alignment (byte count, always a power of
/// two) for a primitive. `bool` stores a single byte via `i32.store8`.
/// Integers and floats use their natural width. The alignment value is used
/// as the `align=` WAT memarg — it must be a power of 2 in bytes, not the
/// exponent.
pub fn store_op(ty: &str) -> Result<(&'static str, u32), CompileError> {
    Ok(match ty {
        "bool" => ("i32.store8", 1),
        "u32" | "i32" | "char" => ("i32.store", 4),
        "u64" | "i64" => ("i64.store", 8),
        "f32" => ("f32.store", 4),
        "f64" => ("f64.store", 8),
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
            Instruction::VariantCtor { value, .. } => {
                if let Some(v) = value {
                    collect_locals_rec(
                        std::slice::from_ref(v.as_ref()),
                        params,
                        func_map,
                        type_map,
                        locals,
                        seen,
                    );
                }
            }
            Instruction::TupleLiteral { values } => {
                for v in values {
                    collect_locals_rec(
                        std::slice::from_ref(v),
                        params,
                        func_map,
                        type_map,
                        locals,
                        seen,
                    );
                }
            }
            Instruction::MatchVariant { value, arms } => {
                collect_locals_rec(
                    std::slice::from_ref(value.as_ref()),
                    params,
                    func_map,
                    type_map,
                    locals,
                    seen,
                );
                if let Some(variant_ty) = infer_wit_type(value, params, func_map, type_map)
                    && let Ok(ResolvedType::Variant(cases)) = resolve_type(&variant_ty, type_map)
                {
                    for arm in arms {
                        if let Some(bname) = &arm.binding
                            && !seen.contains(bname)
                            && let Some((_, Some(payload_ty))) =
                                cases.iter().find(|(n, _)| n == &arm.case)
                        {
                            seen.insert(bname.clone());
                            locals.push((bname.clone(), payload_ty.clone()));
                        }
                    }
                }
                for arm in arms {
                    collect_locals_rec(&arm.body, params, func_map, type_map, locals, seen);
                }
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

    // If the function returns a (ptr, len) compound (string/list) or a
    // record, the last body instruction is expected to produce that value.
    // Wrap it in the appropriate indirect-return sequence.
    #[derive(Clone, Copy)]
    enum WrapKind {
        PtrLen,
        Record,
        Variant,
        Tuple,
    }
    let wrap_kind = result_ty.and_then(|ty| match resolve_type(ty, type_map) {
        Ok(ResolvedType::String) | Ok(ResolvedType::List(_)) => Some(WrapKind::PtrLen),
        Ok(ResolvedType::Record(_)) => Some(WrapKind::Record),
        Ok(ResolvedType::Variant(_)) => Some(WrapKind::Variant),
        Ok(ResolvedType::Tuple(_)) => Some(WrapKind::Tuple),
        _ => None,
    });

    let (init, last) = if wrap_kind.is_some() {
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
        match wrap_kind {
            Some(WrapKind::PtrLen) => emit_ptrlen_return_wrap(
                last,
                &scope,
                type_map,
                literal_table,
                ret_ptr_slot,
                &mut out,
            )?,
            Some(WrapKind::Record) => emit_record_return_wrap(
                last,
                result_ty.unwrap(),
                &scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                &mut out,
            )?,
            Some(WrapKind::Variant) => emit_variant_return_wrap(
                last,
                result_ty.unwrap(),
                &scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                &mut out,
            )?,
            Some(WrapKind::Tuple) => emit_tuple_return_wrap(
                last,
                result_ty.unwrap(),
                &scope,
                func_map,
                type_map,
                literal_table,
                ret_ptr_slot,
                &mut out,
            )?,
            None => unreachable!(),
        }
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

/// Wrap a record-producing instruction (must be `RecordLiteral`) into an
/// indirect-return sequence: allocate record size bytes, store each field at
/// its Canonical-ABI offset, then push the buffer pointer.
#[allow(clippy::too_many_arguments)]
fn emit_record_return_wrap(
    instr: &Instruction,
    record_ty: &str,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    let ret_ptr = ret_ptr_slot.ok_or_else(|| {
        CompileError::Unsupported(
            "ret_ptr_slot missing for indirect record return (collect_locals should have reserved one)".into(),
        )
    })?;

    let ResolvedType::Record(declared_fields) = resolve_type(record_ty, type_map)? else {
        return Err(CompileError::InvalidInput(format!(
            "record return wrap on non-record type {record_ty:?}"
        )));
    };

    let (size, align) = size_align(record_ty, type_map)?;

    let user_fields = match instr {
        Instruction::RecordLiteral { fields } => fields,
        other => {
            return Err(CompileError::Unsupported(format!(
                "record return from {other:?} not supported yet (v0.16 handles RecordLiteral)"
            )));
        }
    };

    // Allocate record bytes.
    out.push_str(&format!(
        "      i32.const 0\n      i32.const 0\n      i32.const {align}\n      i32.const {size}\n      call $cabi_realloc\n      local.set {ret_ptr}\n"
    ));

    // Write each declared field in order. Caller may supply fields out of
    // order — find by name.
    for (fname, ftype) in &declared_fields {
        let (field_offset, _) = record_field_info(&declared_fields, fname, type_map)?;
        let value = user_fields
            .iter()
            .find(|(n, _)| n == fname)
            .map(|(_, v)| v)
            .ok_or_else(|| {
                CompileError::InvalidInput(format!(
                    "RecordLiteral missing field {fname:?} for record {record_ty:?}"
                ))
            })?;
        // v0.16: primitive fields only.
        match resolve_type(ftype, type_map)? {
            ResolvedType::Primitive(p) => {
                let (store, align_pow2) = store_op(&p)?;
                out.push_str(&format!("      local.get {ret_ptr}\n"));
                emit_instr(
                    value,
                    Some(ftype),
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
                out.push_str(&format!(
                    "      {store} offset={field_offset} align={align_pow2}\n"
                ));
            }
            other => {
                return Err(CompileError::Unsupported(format!(
                    "record field {fname:?} of type {ftype:?} (resolved {other:?}) not supported yet — v0.16 handles primitives only"
                )));
            }
        }
    }

    // Push the buffer pointer as the core function's return value.
    out.push_str(&format!("      local.get {ret_ptr}\n"));
    Ok(())
}

/// Wrap a variant-producing instruction (must be `VariantCtor`) into an
/// indirect-return sequence: allocate variant size bytes, store the u8 disc
/// + the selected case's payload (if any), then push the buffer pointer.
#[allow(clippy::too_many_arguments)]
fn emit_variant_return_wrap(
    instr: &Instruction,
    variant_ty: &str,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    let ret_ptr = ret_ptr_slot.ok_or_else(|| {
        CompileError::Unsupported("ret_ptr_slot missing for indirect variant return".into())
    })?;

    let ResolvedType::Variant(cases) = resolve_type(variant_ty, type_map)? else {
        return Err(CompileError::InvalidInput(format!(
            "variant return wrap on non-variant type {variant_ty:?}"
        )));
    };

    let (case, payload) = match instr {
        Instruction::VariantCtor { case, value } => (case, value),
        other => {
            return Err(CompileError::Unsupported(format!(
                "variant return from {other:?} not supported yet (v0.17 handles VariantCtor)"
            )));
        }
    };

    let disc = cases.iter().position(|(n, _)| n == case).ok_or_else(|| {
        CompileError::InvalidInput(format!("case {case:?} not found in variant {variant_ty:?}"))
    })?;
    let declared_payload = cases[disc].1.clone();
    if payload.is_some() && declared_payload.is_none() {
        return Err(CompileError::InvalidInput(format!(
            "case {case:?} has no payload but ctor supplied one"
        )));
    }
    if payload.is_none() && declared_payload.is_some() {
        return Err(CompileError::InvalidInput(format!(
            "case {case:?} requires payload but ctor omitted it"
        )));
    }

    let (size, align) = size_align(variant_ty, type_map)?;

    // Allocate and stash ptr.
    out.push_str(&format!(
        "      i32.const 0\n      i32.const 0\n      i32.const {align}\n      i32.const {size}\n      call $cabi_realloc\n      local.set {ret_ptr}\n"
    ));

    // Store disc (u8 — valid for ≤256 cases).
    out.push_str(&format!(
        "      local.get {ret_ptr}\n      i32.const {disc}\n      i32.store8 offset=0\n"
    ));

    // Store payload when the case carries one.
    if let (Some(pty), Some(val)) = (declared_payload.as_deref(), payload.as_deref()) {
        match resolve_type(pty, type_map)? {
            ResolvedType::Primitive(p) => {
                let (_, pay_align) = size_align(pty, type_map)?;
                let payload_offset = align_up(1, pay_align);
                let (store, align_pow2) = store_op(&p)?;
                out.push_str(&format!("      local.get {ret_ptr}\n"));
                emit_instr(
                    val,
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
            other => {
                return Err(CompileError::Unsupported(format!(
                    "variant case {case:?} with {other:?} payload not supported yet \
                     (v0.17 handles primitive payloads only)"
                )));
            }
        }
    }

    out.push_str(&format!("      local.get {ret_ptr}\n"));
    Ok(())
}

/// Wrap a tuple-producing instruction (must be `TupleLiteral`) into an
/// indirect-return sequence. Tuples share record's layout — each element
/// lives at the offset a record field would.
#[allow(clippy::too_many_arguments)]
fn emit_tuple_return_wrap(
    instr: &Instruction,
    tuple_ty: &str,
    scope: &[(String, String)],
    func_map: &FuncMap,
    type_map: &TypeMap,
    literal_table: &LiteralTable,
    ret_ptr_slot: Option<usize>,
    out: &mut String,
) -> Result<(), CompileError> {
    let ret_ptr = ret_ptr_slot.ok_or_else(|| {
        CompileError::Unsupported("ret_ptr_slot missing for indirect tuple return".into())
    })?;

    let ResolvedType::Tuple(elem_types) = resolve_type(tuple_ty, type_map)? else {
        return Err(CompileError::InvalidInput(format!(
            "tuple return wrap on non-tuple type {tuple_ty:?}"
        )));
    };

    let values = match instr {
        Instruction::TupleLiteral { values } => values,
        other => {
            return Err(CompileError::Unsupported(format!(
                "tuple return from {other:?} not supported yet (v0.18 handles TupleLiteral)"
            )));
        }
    };

    if values.len() != elem_types.len() {
        return Err(CompileError::InvalidInput(format!(
            "TupleLiteral arity {} does not match tuple type arity {}",
            values.len(),
            elem_types.len(),
        )));
    }

    let (size, align) = size_align(tuple_ty, type_map)?;

    // Allocate + stash buffer pointer.
    out.push_str(&format!(
        "      i32.const 0\n      i32.const 0\n      i32.const {align}\n      i32.const {size}\n      call $cabi_realloc\n      local.set {ret_ptr}\n"
    ));

    // Synthesize positional field names so record_field_info can compute
    // offsets for us (tuple layout == record layout with these names).
    let synthetic_fields: Vec<(String, String)> = elem_types
        .iter()
        .enumerate()
        .map(|(i, t)| (i.to_string(), t.clone()))
        .collect();

    for (i, value) in values.iter().enumerate() {
        let (offset, ftype) = record_field_info(&synthetic_fields, &i.to_string(), type_map)?;
        match resolve_type(&ftype, type_map)? {
            ResolvedType::Primitive(p) => {
                let (store, align_pow2) = store_op(&p)?;
                out.push_str(&format!("      local.get {ret_ptr}\n"));
                emit_instr(
                    value,
                    Some(&ftype),
                    scope,
                    func_map,
                    type_map,
                    literal_table,
                    ret_ptr_slot,
                    out,
                )?;
                out.push_str(&format!(
                    "      {store} offset={offset} align={align_pow2}\n"
                ));
            }
            other => {
                return Err(CompileError::Unsupported(format!(
                    "tuple element {i} of type {ftype:?} (resolved {other:?}) not supported yet — v0.18 handles primitive elements only"
                )));
            }
        }
    }

    out.push_str(&format!("      local.get {ret_ptr}\n"));
    Ok(())
}

/// Scan a body for any compound constructor (`Some`/`None`/`Ok`/`Err`/`VariantCtor`/
/// `RecordLiteral`/`TupleLiteral`) that would need the synthesized return-pointer local.
pub fn body_needs_ret_ptr(body: &[Instruction]) -> bool {
    body.iter().any(instr_has_compound_ctor)
}

fn instr_has_compound_ctor(i: &Instruction) -> bool {
    match i {
        Instruction::Some { .. }
        | Instruction::None
        | Instruction::Ok { .. }
        | Instruction::Err { .. }
        | Instruction::VariantCtor { .. }
        | Instruction::RecordLiteral { .. }
        | Instruction::TupleLiteral { .. } => true,
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
        Instruction::RecordGet { value, field } => {
            // v0.16: restrict to `LocalGet(record_local)`. Read the field's
            // flat slots from the concatenated flat layout.
            let uid = match value.as_ref() {
                Instruction::LocalGet { uid } => uid,
                _ => {
                    return Err(CompileError::Unsupported(
                        "RecordGet only supports LocalGet of a record local for now".into(),
                    ));
                }
            };
            let (base_idx, _, ty) = slot_info(uid, scope, type_map)?;
            let ResolvedType::Record(fields) = resolve_type(ty, type_map)? else {
                return Err(CompileError::InvalidInput(format!(
                    "RecordGet applied to non-record local {uid:?}"
                )));
            };
            let (slot_offset, field_slots) = record_field_slot_info(&fields, field, type_map)?;
            for i in 0..field_slots.len() {
                out.push_str(&format!("      local.get {}\n", base_idx + slot_offset + i));
            }
        }
        Instruction::RecordLiteral { .. } => {
            // RecordLiteral is handled at the return-position wrap. Seeing it
            // in a non-return context means it would be consumed as a flat
            // value stream — not supported yet (need memory alloc + stores).
            return Err(CompileError::Unsupported(
                "RecordLiteral outside of return position not supported yet".into(),
            ));
        }
        Instruction::VariantCtor { .. } => {
            // Same rationale as RecordLiteral: handled only via
            // emit_variant_return_wrap at function-return position.
            return Err(CompileError::Unsupported(
                "VariantCtor outside of return position not supported yet".into(),
            ));
        }
        Instruction::TupleLiteral { .. } => {
            return Err(CompileError::Unsupported(
                "TupleLiteral outside of return position not supported yet".into(),
            ));
        }
        Instruction::TupleGet { value, index } => {
            // Mirror of RecordGet, with positional index lookup.
            let uid = match value.as_ref() {
                Instruction::LocalGet { uid } => uid,
                _ => {
                    return Err(CompileError::Unsupported(
                        "TupleGet only supports LocalGet of a tuple local for now".into(),
                    ));
                }
            };
            let (base_idx, _, ty) = slot_info(uid, scope, type_map)?;
            let ResolvedType::Tuple(elems) = resolve_type(ty, type_map)? else {
                return Err(CompileError::InvalidInput(format!(
                    "TupleGet applied to non-tuple local {uid:?}"
                )));
            };
            let idx = *index as usize;
            if idx >= elems.len() {
                return Err(CompileError::InvalidInput(format!(
                    "tuple index {idx} out of bounds (arity {})",
                    elems.len()
                )));
            }
            // Walk the flat layout, summing slot counts up to `idx`, then
            // emit one local.get per slot of the selected element.
            let mut slot_offset = 0usize;
            for (i, et) in elems.iter().enumerate() {
                let slots = flat_slots(et, type_map)?;
                if i == idx {
                    for s in 0..slots.len() {
                        out.push_str(&format!("      local.get {}\n", base_idx + slot_offset + s));
                    }
                    break;
                }
                slot_offset += slots.len();
            }
        }
        Instruction::MatchVariant { value, arms } => {
            // Restrict to `LocalGet(variant_local)` for the value source —
            // this lets us read the disc + payload slots directly from the
            // local's slot range without needing intermediate temps beyond
            // the arm bindings.
            let uid = match value.as_ref() {
                Instruction::LocalGet { uid } => uid,
                _ => {
                    return Err(CompileError::Unsupported(
                        "MatchVariant only supports LocalGet of a variant local for now".into(),
                    ));
                }
            };
            let (first_idx, slots, ty) = slot_info(uid, scope, type_map)?;
            let cases = match resolve_type(ty, type_map)? {
                ResolvedType::Variant(cs) => cs,
                other => {
                    return Err(CompileError::InvalidInput(format!(
                        "MatchVariant applied to non-variant local {uid:?} (resolved {other:?})"
                    )));
                }
            };

            // disc at first_idx; payload flat slots follow at first_idx+1..
            // For each arm that has a binding, we `local.set` the payload's
            // first flat slot into that binding before running its body.
            // Homogeneous assumption (v0.17): all payload-bearing cases
            // share one flat slot of the same core type.
            if slots.len() > 2 {
                return Err(CompileError::Unsupported(format!(
                    "MatchVariant with multi-slot payloads ({} slots) not supported yet",
                    slots.len()
                )));
            }

            // Pre-populate each arm's binding local with the payload value
            // (shared across all arms — cheap, since only the arm that runs
            // will actually read it).
            for arm in arms {
                if let Some(bname) = &arm.binding {
                    let (bidx, bslots, _) = slot_info(bname, scope, type_map)?;
                    if bslots.len() != 1 {
                        return Err(CompileError::Unsupported(format!(
                            "MatchVariant binding {bname:?} of compound type not supported"
                        )));
                    }
                    // Only copy if the variant has a payload slot.
                    if slots.len() == 2 {
                        out.push_str(&format!(
                            "      local.get {}\n      local.set {}\n",
                            first_idx + 1,
                            bidx,
                        ));
                    }
                }
            }

            // Validate arms vs declared cases + build ordered dispatch list
            // (one entry per declared case, so br_table maps disc → arm).
            let arm_for_case: Vec<Option<&wast_pattern_analyzer::MatchArm>> = cases
                .iter()
                .map(|(cname, _)| arms.iter().find(|a| &a.case == cname))
                .collect();
            for a in arms {
                if !cases.iter().any(|(n, _)| n == &a.case) {
                    return Err(CompileError::InvalidInput(format!(
                        "MatchVariant arm references unknown case {:?}",
                        a.case
                    )));
                }
            }

            // Emit an if-chain over disc. Each iteration compares disc to
            // the case index and runs the matched arm's body; the last else
            // branch runs the final arm (or unreachable for exhaustive).
            let result_clause_info = branch_result_clause(expected, type_map)?;
            let (result_clause, child_expected) = result_clause_info;

            // Build: if (disc == 0) { arm0 } else if (disc == 1) { arm1 } ...
            // Use nested if-else chain so each typed if can return the
            // function's expected value on all paths.
            let emit_case = |idx: usize,
                             arm: Option<&wast_pattern_analyzer::MatchArm>,
                             out: &mut String|
             -> Result<(), CompileError> {
                match arm {
                    Some(a) => {
                        for instr in &a.body {
                            emit_instr(
                                instr,
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
                    None => {
                        return Err(CompileError::InvalidInput(format!(
                            "MatchVariant missing arm for case index {idx}"
                        )));
                    }
                }
                Ok(())
            };

            let n_cases = cases.len();
            for idx in 0..n_cases.saturating_sub(1) {
                out.push_str(&format!(
                    "      local.get {}\n      i32.const {}\n      i32.eq\n      if{}\n",
                    first_idx, idx, result_clause,
                ));
                emit_case(idx, arm_for_case[idx], out)?;
                out.push_str("      else\n");
            }
            // Last case: falls into the innermost else without its own if.
            if n_cases > 0 {
                emit_case(n_cases - 1, arm_for_case[n_cases - 1], out)?;
            }
            for _ in 0..n_cases.saturating_sub(1) {
                out.push_str("      end\n");
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
                // Compound types all use indirect return (single i32 ptr).
                ResolvedType::String
                | ResolvedType::List(_)
                | ResolvedType::Option(_)
                | ResolvedType::Result(_, _)
                | ResolvedType::Record(_)
                | ResolvedType::Variant(_)
                | ResolvedType::Tuple(_) => "i32".to_string(),
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
        Instruction::RecordGet { value, field } => {
            let record_ty = infer_wit_type(value, scope, func_map, type_map)?;
            let resolved = resolve_type(&record_ty, type_map).ok()?;
            if let ResolvedType::Record(fields) = resolved {
                fields
                    .iter()
                    .find(|(n, _)| n == field)
                    .map(|(_, t)| t.clone())
            } else {
                None
            }
        }
        Instruction::TupleGet { value, index } => {
            let tuple_ty = infer_wit_type(value, scope, func_map, type_map)?;
            let resolved = resolve_type(&tuple_ty, type_map).ok()?;
            if let ResolvedType::Tuple(elems) = resolved {
                elems.get(*index as usize).cloned()
            } else {
                None
            }
        }
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
