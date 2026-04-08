#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::wast::core::types::{
    ExtractTarget, FuncSource, SymEntry, Syms, TypeSource, WastComponent, WastError, WastFunc,
    WastTypeDef, WitType,
};
use std::collections::HashSet;
use wast_pattern_analyzer::Instruction;

struct Component;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn err(msg: impl Into<String>, location: Option<String>) -> WastError {
    WastError {
        message: msg.into(),
        location,
    }
}

/// Collect type UIDs directly referenced by a function's params and result.
fn referenced_types(f: &WastFunc) -> Vec<String> {
    let mut refs: Vec<String> = f.params.iter().map(|(_, t)| t.clone()).collect();
    if let Some(ref r) = f.result {
        refs.push(r.clone());
    }
    refs
}

/// Collect type UIDs referenced by a WitType definition.
fn type_refs_from_wit_type(wt: &WitType) -> Vec<String> {
    match wt {
        WitType::Primitive(_) => vec![],
        WitType::Option(r) => vec![r.clone()],
        WitType::Result((ok, err)) => vec![ok.clone(), err.clone()],
        WitType::List(r) => vec![r.clone()],
        WitType::Record(fields) => fields.iter().map(|(_, r)| r.clone()).collect(),
        WitType::Variant(cases) => cases.iter().filter_map(|(_, r)| r.clone()).collect(),
        WitType::Tuple(items) => items.clone(),
    }
}

/// Transitively collect all type UIDs needed, starting from a seed set.
fn collect_types_transitively(
    seeds: &[String],
    all_types: &[(String, WastTypeDef)],
) -> HashSet<String> {
    let mut needed: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = seeds.to_vec();
    while let Some(uid) = stack.pop() {
        if !needed.insert(uid.clone()) {
            continue;
        }
        if let Some((_, td)) = all_types.iter().find(|(id, _)| *id == uid) {
            for dep in type_refs_from_wit_type(&td.definition) {
                if !needed.contains(&dep) {
                    stack.push(dep);
                }
            }
        }
    }
    needed
}

/// Check if two functions have the same signature (params types + result type).
fn signatures_match(a: &WastFunc, b: &WastFunc) -> bool {
    let a_param_types: Vec<&str> = a.params.iter().map(|(_, t)| t.as_str()).collect();
    let b_param_types: Vec<&str> = b.params.iter().map(|(_, t)| t.as_str()).collect();
    a_param_types == b_param_types && a.result == b.result
}

/// Check if two type definitions are equivalent.
fn type_defs_match(a: &WastTypeDef, b: &WastTypeDef) -> bool {
    format!("{:?}", a.definition) == format!("{:?}", b.definition)
}

/// Deserialize a function body and collect all directly-called func UIDs.
fn extract_call_refs(body: &[u8]) -> Vec<String> {
    match wast_pattern_analyzer::deserialize_body(body) {
        Ok(instructions) => {
            let mut refs = Vec::new();
            for instr in &instructions {
                collect_calls(instr, &mut refs);
            }
            refs
        }
        Err(_) => vec![],
    }
}

/// Recursively walk an instruction tree and collect Call func_uid values.
fn collect_calls(instr: &Instruction, out: &mut Vec<String>) {
    match instr {
        Instruction::Call { func_uid, args } => {
            out.push(func_uid.clone());
            for (_, arg) in args {
                collect_calls(arg, out);
            }
        }
        Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
            for child in body {
                collect_calls(child, out);
            }
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            collect_calls(condition, out);
            for child in then_body {
                collect_calls(child, out);
            }
            for child in else_body {
                collect_calls(child, out);
            }
        }
        Instruction::BrIf { condition, .. } => {
            collect_calls(condition, out);
        }
        Instruction::LocalSet { value, .. } => {
            collect_calls(value, out);
        }
        Instruction::Compare { lhs, rhs, .. } | Instruction::Arithmetic { lhs, rhs, .. } => {
            collect_calls(lhs, out);
            collect_calls(rhs, out);
        }
        Instruction::Some { value }
        | Instruction::Ok { value }
        | Instruction::Err { value }
        | Instruction::IsErr { value } => {
            collect_calls(value, out);
        }
        Instruction::MatchOption {
            value,
            some_body,
            none_body,
            ..
        } => {
            collect_calls(value, out);
            for child in some_body {
                collect_calls(child, out);
            }
            for child in none_body {
                collect_calls(child, out);
            }
        }
        Instruction::MatchResult {
            value,
            ok_body,
            err_body,
            ..
        } => {
            collect_calls(value, out);
            for child in ok_body {
                collect_calls(child, out);
            }
            for child in err_body {
                collect_calls(child, out);
            }
        }
        // Leaf nodes: Br, Return, LocalGet, Const, None, Nop
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Extract
// ---------------------------------------------------------------------------

fn extract_impl(full: WastComponent, targets: Vec<ExtractTarget>) -> WastComponent {
    let target_uids: HashSet<&str> = targets.iter().map(|t| t.sym.as_str()).collect();

    // Step 1: Collect target funcs that exist in full
    let mut included_func_uids: HashSet<String> = HashSet::new();
    for uid in &target_uids {
        if full.funcs.iter().any(|(id, _)| id == uid) {
            included_func_uids.insert(uid.to_string());
        }
    }

    // Walk bodies of included funcs to find call references; add called funcs
    // as imported (only direct calls, no recursion).
    let mut called_uids: HashSet<String> = HashSet::new();
    for (uid, func) in &full.funcs {
        if included_func_uids.contains(uid.as_str()) {
            if let Some(ref body) = func.body {
                for called in extract_call_refs(body) {
                    if !included_func_uids.contains(called.as_str()) {
                        called_uids.insert(called);
                    }
                }
            }
        }
    }
    // Add called funcs as imported
    for called in &called_uids {
        if full.funcs.iter().any(|(id, _)| id == called) {
            included_func_uids.insert(called.clone());
        }
    }

    // If include_caller, scan ALL funcs for calls to any target; include callers
    let include_caller_targets: HashSet<&str> = targets
        .iter()
        .filter(|t| t.include_caller)
        .map(|t| t.sym.as_str())
        .collect();
    if !include_caller_targets.is_empty() {
        for (uid, func) in &full.funcs {
            if included_func_uids.contains(uid.as_str()) {
                continue;
            }
            if let Some(ref body) = func.body {
                let calls = extract_call_refs(body);
                if calls
                    .iter()
                    .any(|c| include_caller_targets.contains(c.as_str()))
                {
                    included_func_uids.insert(uid.clone());
                }
            }
        }
    }

    // Step 2: Collect type refs from all included funcs, then transitively
    let mut type_seeds: Vec<String> = Vec::new();
    for (uid, func) in &full.funcs {
        if included_func_uids.contains(uid.as_str()) {
            type_seeds.extend(referenced_types(func));
        }
    }
    let needed_types = collect_types_transitively(&type_seeds, &full.types);

    // Step 3: Build output funcs — targets keep their source as-is,
    // called funcs and callers become imported(uid)
    let out_funcs: Vec<(String, WastFunc)> = full
        .funcs
        .iter()
        .filter(|(uid, _)| included_func_uids.contains(uid.as_str()))
        .map(|(uid, func)| {
            if target_uids.contains(uid.as_str()) {
                (uid.clone(), func.clone())
            } else {
                // This func was pulled in as a callee or caller — mark as imported
                let mut imported_func = func.clone();
                imported_func.source = FuncSource::Imported(uid.clone());
                imported_func.body = None;
                (uid.clone(), imported_func)
            }
        })
        .collect();

    // Step 4: Build output types
    let out_types: Vec<(String, WastTypeDef)> = full
        .types
        .iter()
        .filter(|(uid, _)| needed_types.contains(uid.as_str()))
        .map(|(uid, td)| (uid.clone(), td.clone()))
        .collect();

    // Step 5: Build output syms — include syms whose UID matches an included func or type
    let all_included: HashSet<&str> = included_func_uids
        .iter()
        .map(|s| s.as_str())
        .chain(needed_types.iter().map(|s| s.as_str()))
        .collect();

    let out_wit_syms: Vec<(String, String)> = full
        .syms
        .wit_syms
        .iter()
        .filter(|(k, _)| all_included.contains(k.as_str()))
        .cloned()
        .collect();

    let out_internal: Vec<SymEntry> = full
        .syms
        .internal
        .iter()
        .filter(|e| all_included.contains(e.uid.as_str()))
        .cloned()
        .collect();

    let out_local: Vec<SymEntry> = full
        .syms
        .local
        .iter()
        .filter(|e| all_included.contains(e.uid.as_str()))
        .cloned()
        .collect();

    WastComponent {
        funcs: out_funcs,
        types: out_types,
        syms: Syms {
            wit_syms: out_wit_syms,
            internal: out_internal,
            local: out_local,
        },
    }
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

fn merge_impl(
    partial: WastComponent,
    mut full: WastComponent,
) -> Result<WastComponent, Vec<WastError>> {
    let mut errors: Vec<WastError> = Vec::new();

    // Merge funcs
    for (uid, pfunc) in &partial.funcs {
        match &pfunc.source {
            FuncSource::Imported(_) | FuncSource::Exported(_) => {
                // Must exist in full with matching signature
                if let Some((_, ffunc)) = full.funcs.iter().find(|(fid, _)| fid == uid) {
                    if !signatures_match(pfunc, ffunc) {
                        errors.push(err(
                            format!("signature_mismatch: func '{}'", uid),
                            Some(uid.clone()),
                        ));
                    }
                } else {
                    errors.push(err(
                        format!("signature_mismatch: func '{}' not found in full", uid),
                        Some(uid.clone()),
                    ));
                }
            }
            FuncSource::Internal(_) => {
                // Check for conflict with non-internal
                if let Some((_, ffunc)) = full.funcs.iter().find(|(fid, _)| fid == uid) {
                    if !matches!(&ffunc.source, FuncSource::Internal(_)) {
                        errors.push(err(
                            format!(
                                "uid_conflict: func '{}' exists as non-internal in full",
                                uid
                            ),
                            Some(uid.clone()),
                        ));
                        continue;
                    }
                }
                // Add or update
                if let Some(entry) = full.funcs.iter_mut().find(|(fid, _)| fid == uid) {
                    entry.1 = pfunc.clone();
                } else {
                    full.funcs.push((uid.clone(), pfunc.clone()));
                }
            }
        }
    }

    // Merge types
    for (uid, ptype) in &partial.types {
        match &ptype.source {
            TypeSource::Imported(_) | TypeSource::Exported(_) => {
                if let Some((_, ftype)) = full.types.iter().find(|(fid, _)| fid == uid) {
                    if !type_defs_match(ptype, ftype) {
                        errors.push(err(
                            format!("signature_mismatch: type '{}'", uid),
                            Some(uid.clone()),
                        ));
                    }
                } else {
                    errors.push(err(
                        format!("signature_mismatch: type '{}' not found in full", uid),
                        Some(uid.clone()),
                    ));
                }
            }
            TypeSource::Internal(_) => {
                if let Some((_, ftype)) = full.types.iter().find(|(fid, _)| fid == uid) {
                    if !matches!(&ftype.source, TypeSource::Internal(_)) {
                        errors.push(err(
                            format!(
                                "uid_conflict: type '{}' exists as non-internal in full",
                                uid
                            ),
                            Some(uid.clone()),
                        ));
                        continue;
                    }
                }
                if let Some(entry) = full.types.iter_mut().find(|(fid, _)| fid == uid) {
                    entry.1 = ptype.clone();
                } else {
                    full.types.push((uid.clone(), ptype.clone()));
                }
            }
        }
    }

    // Check that all func references in partial's internal funcs exist
    // in either partial or full (missing_dependency check).
    let all_func_uids: HashSet<&str> = full
        .funcs
        .iter()
        .map(|(uid, _)| uid.as_str())
        .chain(partial.funcs.iter().map(|(uid, _)| uid.as_str()))
        .collect();
    for (uid, pfunc) in &partial.funcs {
        if !matches!(&pfunc.source, FuncSource::Internal(_)) {
            continue;
        }
        if let Some(ref body) = pfunc.body {
            for called in extract_call_refs(body) {
                if !all_func_uids.contains(called.as_str()) {
                    errors.push(err(
                        format!(
                            "missing_dependency: func '{}' calls '{}' which is not found",
                            uid, called
                        ),
                        Some(uid.clone()),
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Merge syms (partial overrides full)
    for (path, name) in &partial.syms.wit_syms {
        if let Some(entry) = full.syms.wit_syms.iter_mut().find(|(p, _)| p == path) {
            entry.1 = name.clone();
        } else {
            full.syms.wit_syms.push((path.clone(), name.clone()));
        }
    }

    for entry in &partial.syms.internal {
        if let Some(existing) = full.syms.internal.iter_mut().find(|e| e.uid == entry.uid) {
            existing.display_name = entry.display_name.clone();
        } else {
            full.syms.internal.push(entry.clone());
        }
    }

    for entry in &partial.syms.local {
        if let Some(existing) = full.syms.local.iter_mut().find(|e| e.uid == entry.uid) {
            existing.display_name = entry.display_name.clone();
        } else {
            full.syms.local.push(entry.clone());
        }
    }

    Ok(full)
}

// ---------------------------------------------------------------------------
// WIT interface implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::partial_manager::Guest for Component {
    fn extract(full: WastComponent, targets: Vec<ExtractTarget>) -> WastComponent {
        extract_impl(full, targets)
    }

    fn merge(partial: WastComponent, full: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        merge_impl(partial, full)
    }
}

bindings::export!(Component with_types_in bindings);

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use bindings::wast::core::types::*;

    fn empty_syms() -> Syms {
        Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        }
    }

    fn mk_func(
        uid: &str,
        source: FuncSource,
        params: &[(&str, &str)],
        result: Option<&str>,
    ) -> (String, WastFunc) {
        (
            uid.to_string(),
            WastFunc {
                source,
                params: params
                    .iter()
                    .map(|(n, t)| (n.to_string(), t.to_string()))
                    .collect(),
                result: result.map(|s| s.to_string()),
                body: None,
            },
        )
    }

    fn mk_type(uid: &str, source: TypeSource, def: WitType) -> (String, WastTypeDef) {
        (
            uid.to_string(),
            WastTypeDef {
                source,
                definition: def,
            },
        )
    }

    // ── extract ──

    #[test]
    fn extract_selects_target_func() {
        let full = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Internal("f1".into()),
                    &[("x", "i32")],
                    Some("i32"),
                ),
                mk_func("f2", FuncSource::Internal("f2".into()), &[], None),
            ],
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![
                    SymEntry {
                        uid: "f1".into(),
                        display_name: "func_one".into(),
                    },
                    SymEntry {
                        uid: "f2".into(),
                        display_name: "func_two".into(),
                    },
                ],
                local: vec![],
            },
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        assert_eq!(result.funcs.len(), 1);
        assert_eq!(result.funcs[0].0, "f1");
        assert_eq!(result.syms.internal.len(), 1);
        assert_eq!(result.syms.internal[0].uid, "f1");
    }

    #[test]
    fn extract_includes_referenced_types() {
        let full = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "my_type")],
                Some("other_type"),
            )],
            types: vec![
                mk_type(
                    "my_type",
                    TypeSource::Internal("my_type".into()),
                    WitType::Primitive(PrimitiveType::U32),
                ),
                mk_type(
                    "other_type",
                    TypeSource::Internal("other_type".into()),
                    WitType::Primitive(PrimitiveType::Bool),
                ),
                mk_type(
                    "unused",
                    TypeSource::Internal("unused".into()),
                    WitType::Primitive(PrimitiveType::String),
                ),
            ],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let type_uids: HashSet<String> = result.types.iter().map(|(u, _)| u.clone()).collect();
        assert!(type_uids.contains("my_type"));
        assert!(type_uids.contains("other_type"));
        assert!(!type_uids.contains("unused"));
    }

    #[test]
    fn extract_includes_transitive_type_refs() {
        let full = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "list_t")],
                None,
            )],
            types: vec![
                mk_type(
                    "list_t",
                    TypeSource::Internal("list_t".into()),
                    WitType::List("rec_t".into()),
                ),
                mk_type(
                    "rec_t",
                    TypeSource::Internal("rec_t".into()),
                    WitType::Record(vec![("field".into(), "inner_t".into())]),
                ),
                mk_type(
                    "inner_t",
                    TypeSource::Internal("inner_t".into()),
                    WitType::Primitive(PrimitiveType::U64),
                ),
            ],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let type_uids: HashSet<String> = result.types.iter().map(|(u, _)| u.clone()).collect();
        assert!(type_uids.contains("list_t"));
        assert!(type_uids.contains("rec_t"));
        assert!(type_uids.contains("inner_t"));
    }

    #[test]
    fn extract_nonexistent_target_gives_empty() {
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Internal("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "nope".into(),
                include_caller: false,
            }],
        );
        assert!(result.funcs.is_empty());
        assert!(result.types.is_empty());
    }

    #[test]
    fn extract_preserves_exported_source() {
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Exported("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        assert!(matches!(&result.funcs[0].1.source, FuncSource::Exported(_)));
    }

    #[test]
    fn extract_multiple_targets() {
        let full = WastComponent {
            funcs: vec![
                mk_func("f1", FuncSource::Internal("f1".into()), &[], None),
                mk_func("f2", FuncSource::Internal("f2".into()), &[], None),
                mk_func("f3", FuncSource::Internal("f3".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![
                ExtractTarget {
                    sym: "f1".into(),
                    include_caller: false,
                },
                ExtractTarget {
                    sym: "f3".into(),
                    include_caller: false,
                },
            ],
        );
        let uids: HashSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
        assert_eq!(uids.len(), 2);
        assert!(uids.contains("f1"));
        assert!(uids.contains("f3"));
        assert!(!uids.contains("f2"));
    }

    // ── merge ──

    #[test]
    fn merge_adds_new_internal_func() {
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f_new",
                FuncSource::Internal("f_new".into()),
                &[],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Internal("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let result = merge_impl(partial, full).unwrap();
        let uids: HashSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
        assert!(uids.contains("f1"));
        assert!(uids.contains("f_new"));
    }

    #[test]
    fn merge_updates_existing_internal_func() {
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("y", "bool")],
                Some("bool"),
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "i32")],
                Some("i32"),
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let result = merge_impl(partial, full).unwrap();
        assert_eq!(result.funcs.len(), 1);
        assert_eq!(result.funcs[0].1.params[0].1, "bool");
    }

    #[test]
    fn merge_imported_signature_mismatch() {
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Imported("f1".into()),
                &[("x", "bool")],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "i32")],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("signature_mismatch"));
    }

    #[test]
    fn merge_imported_signature_match_ok() {
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Imported("f1".into()),
                &[("x", "i32")],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "i32")],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let result = merge_impl(partial, full).unwrap();
        assert_eq!(result.funcs.len(), 1);
    }

    #[test]
    fn merge_uid_conflict() {
        let partial = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Internal("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Exported("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(errs[0].message.contains("uid_conflict"));
    }

    #[test]
    fn merge_imported_func_not_in_full() {
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f_missing",
                FuncSource::Imported("f_missing".into()),
                &[],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(errs[0].message.contains("signature_mismatch"));
    }

    #[test]
    fn merge_syms_override() {
        let partial = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: Syms {
                wit_syms: vec![("k1".into(), "partial_v".into())],
                internal: vec![SymEntry {
                    uid: "s1".into(),
                    display_name: "partial_name".into(),
                }],
                local: vec![SymEntry {
                    uid: "l1".into(),
                    display_name: "partial_local".into(),
                }],
            },
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: Syms {
                wit_syms: vec![
                    ("k1".into(), "full_v".into()),
                    ("k2".into(), "full_v2".into()),
                ],
                internal: vec![SymEntry {
                    uid: "s1".into(),
                    display_name: "full_name".into(),
                }],
                local: vec![SymEntry {
                    uid: "l1".into(),
                    display_name: "full_local".into(),
                }],
            },
        };
        let result = merge_impl(partial, full).unwrap();
        // k1 should be overridden, k2 should remain
        let k1 = result
            .syms
            .wit_syms
            .iter()
            .find(|(k, _)| k == "k1")
            .unwrap();
        assert_eq!(k1.1, "partial_v");
        let k2 = result
            .syms
            .wit_syms
            .iter()
            .find(|(k, _)| k == "k2")
            .unwrap();
        assert_eq!(k2.1, "full_v2");
        assert_eq!(result.syms.internal[0].display_name, "partial_name");
        assert_eq!(result.syms.local[0].display_name, "partial_local");
    }

    #[test]
    fn merge_type_uid_conflict() {
        let partial = WastComponent {
            funcs: vec![],
            types: vec![mk_type(
                "t1",
                TypeSource::Internal("t1".into()),
                WitType::Primitive(PrimitiveType::U32),
            )],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![mk_type(
                "t1",
                TypeSource::Exported("t1".into()),
                WitType::Primitive(PrimitiveType::U32),
            )],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(errs[0].message.contains("uid_conflict"));
    }

    #[test]
    fn merge_imported_type_mismatch() {
        let partial = WastComponent {
            funcs: vec![],
            types: vec![mk_type(
                "t1",
                TypeSource::Imported("t1".into()),
                WitType::Primitive(PrimitiveType::Bool),
            )],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![mk_type(
                "t1",
                TypeSource::Internal("t1".into()),
                WitType::Primitive(PrimitiveType::U32),
            )],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(errs[0].message.contains("signature_mismatch"));
    }

    #[test]
    fn merge_adds_new_internal_type() {
        let partial = WastComponent {
            funcs: vec![],
            types: vec![mk_type(
                "t_new",
                TypeSource::Internal("t_new".into()),
                WitType::Primitive(PrimitiveType::F64),
            )],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: empty_syms(),
        };
        let result = merge_impl(partial, full).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].0, "t_new");
    }

    // ── extract: body analysis (call refs) ──

    fn mk_body_calling(targets: &[&str]) -> Vec<u8> {
        let instrs: Vec<Instruction> = targets
            .iter()
            .map(|uid| Instruction::Call {
                func_uid: uid.to_string(),
                args: vec![],
            })
            .collect();
        wast_pattern_analyzer::serialize_body(&instrs)
    }

    #[test]
    fn extract_finds_called_funcs_as_imported() {
        let body = mk_body_calling(&["f2"]);
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(body);
        let full = WastComponent {
            funcs: vec![
                f1,
                mk_func(
                    "f2",
                    FuncSource::Internal("f2".into()),
                    &[("x", "i32")],
                    None,
                ),
                mk_func("f3", FuncSource::Internal("f3".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let uids: HashSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
        assert!(uids.contains("f1"), "target func should be included");
        assert!(uids.contains("f2"), "called func should be included");
        assert!(
            !uids.contains("f3"),
            "unrelated func should NOT be included"
        );
        // f2 should be imported, not internal with body
        let f2_entry = result.funcs.iter().find(|(u, _)| u == "f2").unwrap();
        assert!(
            matches!(&f2_entry.1.source, FuncSource::Imported(_)),
            "called func should become imported"
        );
        assert!(
            f2_entry.1.body.is_none(),
            "imported func body should be stripped"
        );
    }

    #[test]
    fn extract_with_include_caller_finds_callers() {
        let body_calls_f1 = mk_body_calling(&["f1"]);
        let mut f2 = mk_func("f2", FuncSource::Internal("f2".into()), &[], None);
        f2.1.body = Some(body_calls_f1);
        let full = WastComponent {
            funcs: vec![
                mk_func("f1", FuncSource::Internal("f1".into()), &[], None),
                f2,
                mk_func("f3", FuncSource::Internal("f3".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: true,
            }],
        );
        let uids: HashSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
        assert!(uids.contains("f1"), "target should be included");
        assert!(uids.contains("f2"), "caller of target should be included");
        assert!(!uids.contains("f3"), "non-caller should NOT be included");
    }

    // ── merge: missing_dependency ──

    #[test]
    fn merge_detects_missing_dependency() {
        let body = mk_body_calling(&["f_missing"]);
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(body);
        let partial = WastComponent {
            funcs: vec![f1],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("missing_dependency")),
            "should report missing_dependency error"
        );
        assert!(
            errs.iter().any(|e| e.message.contains("f_missing")),
            "error should mention the missing func uid"
        );
    }

    #[test]
    fn merge_no_missing_dependency_when_ref_exists() {
        let body = mk_body_calling(&["f_existing"]);
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(body);
        let partial = WastComponent {
            funcs: vec![f1],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![mk_func(
                "f_existing",
                FuncSource::Internal("f_existing".into()),
                &[],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let result = merge_impl(partial, full);
        assert!(
            result.is_ok(),
            "should not error when called func exists in full"
        );
    }

    #[test]
    fn merge_multiple_errors_collected() {
        let partial = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Imported("f1".into()),
                    &[("x", "bool")],
                    None,
                ),
                mk_func("f2", FuncSource::Internal("f2".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let full = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Internal("f1".into()),
                    &[("x", "i32")],
                    None,
                ),
                mk_func("f2", FuncSource::Exported("f2".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert_eq!(errs.len(), 2);
    }
}
