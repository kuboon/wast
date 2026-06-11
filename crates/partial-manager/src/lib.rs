#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

use bindings::wast::core::types::{
    ExtractTarget, FuncSource, SymEntry, Syms, TypeSource, WastComponent, WastError, WastFunc,
    WastTypeDef, WitType,
};
use std::collections::BTreeSet;
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
        // enum and flags only carry case-name strings, no type references
        // to trace.
        WitType::Enum(_) | WitType::Flags(_) => vec![],
        // Resource declaration has no payload. own/borrow wrap a resource uid.
        WitType::Resource => vec![],
        WitType::Own(r) | WitType::Borrow(r) => vec![r.clone()],
    }
}

/// Transitively collect all type UIDs needed, starting from a seed set.
fn collect_types_transitively(
    seeds: &[String],
    all_types: &[(String, WastTypeDef)],
) -> BTreeSet<String> {
    let mut needed: BTreeSet<String> = BTreeSet::new();
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
/// A body that fails to deserialize is an error — silently treating it as
/// "no calls" would let corrupt data slip through extract/merge validation.
fn extract_call_refs(body: &[u8]) -> Result<Vec<String>, String> {
    let instructions = wast_pattern_analyzer::deserialize_body(body)?;
    let mut refs = Vec::new();
    for instr in &instructions {
        collect_calls(instr, &mut refs);
    }
    Ok(refs)
}

/// Like [`extract_call_refs`], but wraps deserialization failures in a
/// `WastError` that names the offending func uid.
fn call_refs_for(uid: &str, body: &[u8]) -> Result<Vec<String>, WastError> {
    extract_call_refs(body).map_err(|e| {
        err(
            format!("invalid body for func '{uid}': {e}"),
            Some(uid.to_string()),
        )
    })
}

/// Visit every *direct* child instruction of `instr`.
///
/// The match is EXHAUSTIVE on purpose (no wildcard arm): adding a new
/// `Instruction` variant to the pattern-analyzer IR must fail compilation
/// here so this walk is updated alongside the IR — a `_ => {}` arm was how
/// nested calls inside newer nodes were silently missed before.
fn for_each_child<'a>(instr: &'a Instruction, visit: &mut dyn FnMut(&'a Instruction)) {
    match instr {
        Instruction::Block { body, .. } | Instruction::Loop { body, .. } => {
            for child in body {
                visit(child);
            }
        }
        Instruction::If {
            condition,
            then_body,
            else_body,
        } => {
            visit(condition);
            for child in then_body {
                visit(child);
            }
            for child in else_body {
                visit(child);
            }
        }
        Instruction::BrIf { condition, .. } => visit(condition),
        Instruction::Br { .. } => {}
        Instruction::Return => {}
        Instruction::Call { args, .. } => {
            for (_, arg) in args {
                visit(arg);
            }
        }
        Instruction::LocalGet { .. } => {}
        Instruction::LocalSet { value, .. } => visit(value),
        Instruction::Const { .. } => {}
        Instruction::Compare { lhs, rhs, .. } | Instruction::Arithmetic { lhs, rhs, .. } => {
            visit(lhs);
            visit(rhs);
        }
        Instruction::Some { value }
        | Instruction::Ok { value }
        | Instruction::Err { value }
        | Instruction::IsErr { value }
        | Instruction::StringLen { value }
        | Instruction::ListLen { value } => visit(value),
        Instruction::None => {}
        Instruction::MatchOption {
            value,
            some_body,
            none_body,
            ..
        } => {
            visit(value);
            for child in some_body {
                visit(child);
            }
            for child in none_body {
                visit(child);
            }
        }
        Instruction::MatchResult {
            value,
            ok_body,
            err_body,
            ..
        } => {
            visit(value);
            for child in ok_body {
                visit(child);
            }
            for child in err_body {
                visit(child);
            }
        }
        Instruction::StringLiteral { .. } => {}
        Instruction::ListLiteral { values } | Instruction::TupleLiteral { values } => {
            for child in values {
                visit(child);
            }
        }
        Instruction::RecordGet { value, .. } | Instruction::TupleGet { value, .. } => visit(value),
        Instruction::RecordLiteral { fields } => {
            for (_, value) in fields {
                visit(value);
            }
        }
        Instruction::VariantCtor { value, .. } => {
            if let Some(value) = value {
                visit(value);
            }
        }
        Instruction::MatchVariant { value, arms } => {
            visit(value);
            for arm in arms {
                for child in &arm.body {
                    visit(child);
                }
            }
        }
        Instruction::FlagsCtor { .. } => {}
        Instruction::ResourceNew { rep, .. } => visit(rep),
        Instruction::ResourceRep { handle, .. } | Instruction::ResourceDrop { handle, .. } => {
            visit(handle)
        }
        Instruction::Nop => {}
    }
}

/// Depth-first walk over an instruction tree (the node itself, then all
/// descendants via [`for_each_child`]).
fn walk_instruction<'a>(instr: &'a Instruction, f: &mut dyn FnMut(&'a Instruction)) {
    f(instr);
    for_each_child(instr, &mut |child| walk_instruction(child, f));
}

/// Recursively walk an instruction tree and collect Call func_uid values.
fn collect_calls(instr: &Instruction, out: &mut Vec<String>) {
    walk_instruction(instr, &mut |node| {
        if let Instruction::Call { func_uid, .. } = node {
            out.push(func_uid.clone());
        }
    });
}

/// Collect every local-variable uid a function defines: its param uids plus
/// body-local uids (LocalSet targets and match bindings).
fn collect_local_uids(uid: &str, func: &WastFunc) -> Result<BTreeSet<String>, WastError> {
    let mut locals: BTreeSet<String> = func.params.iter().map(|(name, _)| name.clone()).collect();
    if let Some(ref body) = func.body {
        let instructions = wast_pattern_analyzer::deserialize_body(body).map_err(|e| {
            err(
                format!("invalid body for func '{uid}': {e}"),
                Some(uid.to_string()),
            )
        })?;
        for instr in &instructions {
            walk_instruction(instr, &mut |node| match node {
                Instruction::LocalSet { uid, .. } => {
                    locals.insert(uid.clone());
                }
                Instruction::MatchOption { some_binding, .. } => {
                    locals.insert(some_binding.clone());
                }
                Instruction::MatchResult {
                    ok_binding,
                    err_binding,
                    ..
                } => {
                    locals.insert(ok_binding.clone());
                    locals.insert(err_binding.clone());
                }
                Instruction::MatchVariant { arms, .. } => {
                    for arm in arms {
                        if let Some(binding) = &arm.binding {
                            locals.insert(binding.clone());
                        }
                    }
                }
                _ => {}
            });
        }
    }
    Ok(locals)
}

// ---------------------------------------------------------------------------
// Extract
// ---------------------------------------------------------------------------

fn extract_impl(
    full: WastComponent,
    targets: Vec<ExtractTarget>,
) -> Result<WastComponent, WastError> {
    let target_uids: BTreeSet<&str> = targets.iter().map(|t| t.sym.as_str()).collect();
    let include_caller_targets: BTreeSet<&str> = targets
        .iter()
        .filter(|t| t.include_caller)
        .map(|t| t.sym.as_str())
        .collect();

    // Step 1: collect the *owned* set — funcs that appear in the partial
    // with their bodies. That is the targets, plus the direct callers of any
    // include_caller target.
    let mut owned: BTreeSet<String> = BTreeSet::new();
    for uid in &target_uids {
        if full.funcs.iter().any(|(id, _)| id == uid) {
            owned.insert(uid.to_string());
        }
    }
    if !include_caller_targets.is_empty() {
        for (uid, func) in &full.funcs {
            if owned.contains(uid.as_str()) {
                continue;
            }
            if let Some(ref body) = func.body {
                let calls = call_refs_for(uid, body)?;
                if calls
                    .iter()
                    .any(|c| include_caller_targets.contains(c.as_str()))
                {
                    owned.insert(uid.clone());
                }
            }
        }
    }

    // Step 2: collect the *imported* set — callees of any owned func that
    // aren't themselves owned. Their signatures appear in the partial; their
    // bodies live in full.
    let mut imported: BTreeSet<String> = BTreeSet::new();
    for (uid, func) in &full.funcs {
        if !owned.contains(uid.as_str()) {
            continue;
        }
        if let Some(ref body) = func.body {
            for called in call_refs_for(uid, body)? {
                if !owned.contains(called.as_str())
                    && full.funcs.iter().any(|(id, _)| *id == called)
                {
                    imported.insert(called);
                }
            }
        }
    }

    let included_func_uids: BTreeSet<String> = owned.union(&imported).cloned().collect();

    // Step 3: collect type refs from all included funcs, then transitively.
    let mut type_seeds: Vec<String> = Vec::new();
    for (uid, func) in &full.funcs {
        if included_func_uids.contains(uid.as_str()) {
            type_seeds.extend(referenced_types(func));
        }
    }
    let needed_types = collect_types_transitively(&type_seeds, &full.types);

    // Step 4: build output funcs.
    //
    // Source assignment rule:
    //  - target *without* `include_caller` → forced to `Exported`. The partial
    //    has no proof that all callers are present (a caller that *happens*
    //    to be in `B` because it is itself a target doesn't establish that
    //    no other caller exists in `full`). Locking the signature is the
    //    safe default; `merge` enforces it against `full`'s callers.
    //  - target *with* `include_caller` → keep original source. The flag is
    //    the user's promise that all callers have been pulled in, so the
    //    syntax plugin alone can verify call-site consistency.
    //  - pulled-in caller (added because some target had `include_caller`)
    //    → keep original source, keep body. The body is needed for the
    //    syntax plugin's type check.
    //  - pulled-in callee (signature-only stub) → `Imported(uid)`,
    //    `body = None`.
    //  - exception: a target that is already `Imported` in `full` keeps its
    //    `Imported` source — it has no body to edit and retagging it as
    //    `Exported` would misrepresent its origin.
    let out_funcs: Vec<(String, WastFunc)> = full
        .funcs
        .iter()
        .filter(|(uid, _)| included_func_uids.contains(uid.as_str()))
        .map(|(uid, func)| {
            if owned.contains(uid.as_str()) {
                let is_target_no_callers = targets
                    .iter()
                    .any(|t| t.sym.as_str() == uid && !t.include_caller);
                let mut out_func = func.clone();
                if is_target_no_callers && !matches!(func.source, FuncSource::Imported(_)) {
                    out_func.source = FuncSource::Exported(uid.clone());
                }
                (uid.clone(), out_func)
            } else {
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

    // Step 5: Build output syms — include syms whose UID matches an included
    // func or type, plus local syms for the included funcs' params and
    // body-locals (LocalSet targets, match bindings). Without the local-uid
    // seeding, `syms.local` entries were silently dropped on extract.
    let mut local_uids: BTreeSet<String> = BTreeSet::new();
    for (uid, func) in &out_funcs {
        local_uids.extend(collect_local_uids(uid, func)?);
    }
    let all_included: BTreeSet<&str> = included_func_uids
        .iter()
        .map(|s| s.as_str())
        .chain(needed_types.iter().map(|s| s.as_str()))
        .chain(local_uids.iter().map(|s| s.as_str()))
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

    Ok(WastComponent {
        funcs: out_funcs,
        types: out_types,
        syms: Syms {
            wit_syms: out_wit_syms,
            internal: out_internal,
            local: out_local,
        },
    })
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

fn merge_impl(
    partial: WastComponent,
    mut full: WastComponent,
) -> Result<WastComponent, Vec<WastError>> {
    let mut errors: Vec<WastError> = Vec::new();

    // ── Caller revalidation (signature-change safety) ────────────────────
    // When the partial changes an *Internal* func's signature, every caller
    // of that func must be visible inside the partial (where the syntax
    // plugin re-validated the call sites). A caller that only lives in
    // `full` would silently keep passing arguments for the old signature, so
    // scan the bodies of full's funcs that are NOT included in the partial
    // and reject the merge if any of them call a sig-changed func.
    let partial_uids: BTreeSet<&str> = partial.funcs.iter().map(|(uid, _)| uid.as_str()).collect();
    let sig_changed: BTreeSet<&str> = partial
        .funcs
        .iter()
        .filter(|(uid, pfunc)| {
            matches!(&pfunc.source, FuncSource::Internal(_))
                && full.funcs.iter().any(|(fid, ffunc)| {
                    fid == uid
                        && matches!(&ffunc.source, FuncSource::Internal(_))
                        && !signatures_match(pfunc, ffunc)
                })
        })
        .map(|(uid, _)| uid.as_str())
        .collect();
    if !sig_changed.is_empty() {
        for (uid, ffunc) in &full.funcs {
            if partial_uids.contains(uid.as_str()) {
                continue;
            }
            if let Some(ref body) = ffunc.body {
                match call_refs_for(uid, body) {
                    Err(e) => errors.push(e),
                    Ok(calls) => {
                        for called in calls {
                            if sig_changed.contains(called.as_str()) {
                                errors.push(err(
                                    format!(
                                        "conflict: func '{uid}' calls '{called}', whose signature \
                                         is changed by this partial, but '{uid}' is not included \
                                         in the partial — re-extract with include-caller so all \
                                         call sites are revalidated"
                                    ),
                                    Some(uid.clone()),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Merge funcs.
    //
    // Source semantics:
    //  - Imported(uid): the partial only carries this func's signature (no
    //    body in the partial). Verify the signature matches `full` and
    //    leave the full entry untouched.
    //  - Exported(uid): the partial owns this func's *implementation*. Its
    //    signature is the boundary contract, so a sig change must match
    //    `full`'s entry (otherwise hidden callers would break). The body
    //    / params / result are propagated; we preserve `full`'s original
    //    `source` tag (a partial-promoted `Internal` should stay Internal
    //    in `full`).
    //  - Internal(uid): the partial fully owns this func (it had a caller
    //    inside the partial). Add or replace the entry in `full`.
    for (uid, pfunc) in &partial.funcs {
        match &pfunc.source {
            FuncSource::Imported(_) => {
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
            FuncSource::Exported(_) => {
                if let Some(entry) = full.funcs.iter_mut().find(|(fid, _)| fid == uid) {
                    if !signatures_match(pfunc, &entry.1) {
                        errors.push(err(
                            format!("signature_mismatch: func '{}'", uid),
                            Some(uid.clone()),
                        ));
                        continue;
                    }
                    // Sig matches → propagate body / params / result.
                    // Preserve `full`'s source tag so a partial-promoted
                    // Internal func stays Internal in `full`.
                    let original_source = entry.1.source.clone();
                    entry.1 = pfunc.clone();
                    entry.1.source = original_source;
                } else {
                    errors.push(err(
                        format!("signature_mismatch: func '{}' not found in full", uid),
                        Some(uid.clone()),
                    ));
                }
            }
            FuncSource::Internal(_) => {
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
    let all_func_uids: BTreeSet<&str> = full
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
            match call_refs_for(uid, body) {
                Err(e) => errors.push(e),
                Ok(calls) => {
                    for called in calls {
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
        // The WIT contract for `extract` has no error channel; an undecodable
        // body means `full` itself is corrupt, so trap with a clear message
        // rather than silently producing a partial built from broken data.
        match extract_impl(full, targets) {
            Ok(component) => component,
            Err(e) => panic!("extract failed: {}", e.message),
        }
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

    /// Test helper: extract that must succeed.
    fn extract_ok(full: WastComponent, targets: Vec<ExtractTarget>) -> WastComponent {
        extract_impl(full, targets).expect("extract should succeed")
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
        let result = extract_ok(
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
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let type_uids: BTreeSet<String> = result.types.iter().map(|(u, _)| u.clone()).collect();
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
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let type_uids: BTreeSet<String> = result.types.iter().map(|(u, _)| u.clone()).collect();
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
        let result = extract_ok(
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
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        assert!(matches!(&result.funcs[0].1.source, FuncSource::Exported(_)));
    }

    #[test]
    fn extract_target_without_include_caller_forces_exported() {
        // Internal target with include_caller=false → forced to Exported.
        // The partial has no proof that all callers are present, so the
        // signature must be locked.
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Internal("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        assert!(matches!(&result.funcs[0].1.source, FuncSource::Exported(_)));
    }

    #[test]
    fn extract_target_with_include_caller_and_pulled_caller_keep_original() {
        // f1 is internal in full. f2 calls f1. With include_caller=true on
        // f1, f2 is pulled in. Both keep their original source (Internal):
        //  - f1 has the include_caller flag → caller list is complete →
        //    signature can be edited → don't lock as Exported.
        //  - f2 is a pulled-in caller (not a target) → keep original; its
        //    body is preserved so the syntax plugin can check call sites.
        let body_calls_f1 = mk_body_calling(&["f1"]);
        let mut f2 = mk_func("f2", FuncSource::Internal("f2".into()), &[], None);
        f2.1.body = Some(body_calls_f1);
        let full = WastComponent {
            funcs: vec![
                mk_func("f1", FuncSource::Internal("f1".into()), &[], None),
                f2,
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: true,
            }],
        );
        let f1 = result.funcs.iter().find(|(u, _)| u == "f1").unwrap();
        let f2 = result.funcs.iter().find(|(u, _)| u == "f2").unwrap();
        assert!(
            matches!(&f1.1.source, FuncSource::Internal(_)),
            "f1 with include_caller → keep Internal"
        );
        assert!(
            matches!(&f2.1.source, FuncSource::Internal(_)),
            "pulled-in caller f2 → keep Internal"
        );
        assert!(
            f2.1.body.is_some(),
            "pulled-in caller body must be preserved"
        );
    }

    #[test]
    fn extract_two_targets_one_calls_other_both_get_exported() {
        // target=[poly, square], both include_caller=false. poly calls
        // square. Even though square has a caller (poly) inside the
        // partial, that doesn't establish "all callers visible" — there
        // might be other callers in full. Both targets are forced Exported.
        let body = mk_body_calling(&["square"]);
        let mut poly = mk_func("poly", FuncSource::Internal("poly".into()), &[], None);
        poly.1.body = Some(body);
        let full = WastComponent {
            funcs: vec![
                mk_func("square", FuncSource::Internal("square".into()), &[], None),
                poly,
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_ok(
            full,
            vec![
                ExtractTarget {
                    sym: "poly".into(),
                    include_caller: false,
                },
                ExtractTarget {
                    sym: "square".into(),
                    include_caller: false,
                },
            ],
        );
        let square = result.funcs.iter().find(|(u, _)| u == "square").unwrap();
        let poly = result.funcs.iter().find(|(u, _)| u == "poly").unwrap();
        assert!(
            matches!(&square.1.source, FuncSource::Exported(_)),
            "target without include_caller → Exported (even with poly visible)"
        );
        assert!(matches!(&poly.1.source, FuncSource::Exported(_)));
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
        let result = extract_ok(
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
        let uids: BTreeSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
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
        let uids: BTreeSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
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
    fn merge_exported_propagates_body_and_keeps_full_source() {
        // partial.f1 = Exported with body B' and matching signature.
        // full.f1   = Internal with body B (sig matches).
        // Expectation:
        //   - sig matches -> no error
        //   - full.f1.body becomes B' (the partial's body)
        //   - full.f1.source stays Internal (partial's Exported was a
        //     boundary marker, not a kind change)
        let mut partial_func = mk_func(
            "f1",
            FuncSource::Exported("f1".into()),
            &[("x", "i32")],
            Some("i32"),
        );
        partial_func.1.body = Some(vec![1, 2, 3]); // edited body
        let partial = WastComponent {
            funcs: vec![partial_func],
            types: vec![],
            syms: empty_syms(),
        };

        let mut full_func = mk_func(
            "f1",
            FuncSource::Internal("f1".into()),
            &[("x", "i32")],
            Some("i32"),
        );
        full_func.1.body = Some(vec![9, 9]); // original body
        let full = WastComponent {
            funcs: vec![full_func],
            types: vec![],
            syms: empty_syms(),
        };

        let merged = merge_impl(partial, full).unwrap();
        assert_eq!(merged.funcs.len(), 1);
        let (_, m) = &merged.funcs[0];
        assert_eq!(m.body, Some(vec![1, 2, 3]), "body should propagate");
        assert!(
            matches!(&m.source, FuncSource::Internal(_)),
            "source tag from `full` should be preserved (Internal)"
        );
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
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let uids: BTreeSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
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
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: true,
            }],
        );
        let uids: BTreeSet<String> = result.funcs.iter().map(|(u, _)| u.clone()).collect();
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

    // ── exhaustive body walk (nested calls) ──

    #[test]
    fn extract_finds_call_nested_in_record_literal() {
        // A Call buried inside a RecordLiteral field must be discovered so
        // the callee is pulled in as an Imported stub.
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::RecordLiteral {
            fields: vec![(
                "f".into(),
                Instruction::Call {
                    func_uid: "f2".into(),
                    args: vec![],
                },
            )],
        }]);
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(body);
        let full = WastComponent {
            funcs: vec![
                f1,
                mk_func("f2", FuncSource::Internal("f2".into()), &[], None),
            ],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let f2 = result.funcs.iter().find(|(u, _)| u == "f2");
        assert!(
            f2.is_some(),
            "callee nested in RecordLiteral must be included"
        );
        assert!(matches!(&f2.unwrap().1.source, FuncSource::Imported(_)));
    }

    #[test]
    fn merge_missing_dependency_in_match_variant_arm() {
        // A Call inside a MatchVariant arm body must be seen by the
        // missing_dependency check.
        let body = wast_pattern_analyzer::serialize_body(&[Instruction::MatchVariant {
            value: Box::new(Instruction::LocalGet { uid: "v".into() }),
            arms: vec![wast_pattern_analyzer::MatchArm {
                case: "c".into(),
                binding: Option::None,
                body: vec![Instruction::Call {
                    func_uid: "f_missing".into(),
                    args: vec![],
                }],
            }],
        }]);
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
            errs.iter().any(
                |e| e.message.contains("missing_dependency") && e.message.contains("f_missing")
            ),
            "call inside MatchVariant arm must be validated: {errs:?}"
        );
    }

    // ── extract: syms.local preservation ──

    #[test]
    fn extract_preserves_local_syms_for_params_and_body_locals() {
        let body = wast_pattern_analyzer::serialize_body(&[
            Instruction::LocalSet {
                uid: "loc_y".into(),
                value: Box::new(Instruction::Const { value: 1 }),
            },
            Instruction::MatchOption {
                value: Box::new(Instruction::LocalGet { uid: "p_x".into() }),
                some_binding: "bind_z".into(),
                some_body: vec![],
                none_body: vec![],
            },
        ]);
        let mut f1 = mk_func(
            "f1",
            FuncSource::Internal("f1".into()),
            &[("p_x", "u32")],
            None,
        );
        f1.1.body = Some(body);
        let full = WastComponent {
            funcs: vec![f1],
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![
                    SymEntry {
                        uid: "p_x".into(),
                        display_name: "x".into(),
                    },
                    SymEntry {
                        uid: "loc_y".into(),
                        display_name: "y".into(),
                    },
                    SymEntry {
                        uid: "bind_z".into(),
                        display_name: "z".into(),
                    },
                    SymEntry {
                        uid: "unrelated".into(),
                        display_name: "nope".into(),
                    },
                ],
            },
        };
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        let local_uids: BTreeSet<&str> = result.syms.local.iter().map(|e| e.uid.as_str()).collect();
        assert!(local_uids.contains("p_x"), "param sym must survive");
        assert!(local_uids.contains("loc_y"), "LocalSet sym must survive");
        assert!(
            local_uids.contains("bind_z"),
            "match binding sym must survive"
        );
        assert!(
            !local_uids.contains("unrelated"),
            "unrelated local sym must be filtered"
        );
    }

    // ── extract: Imported target keeps its source ──

    #[test]
    fn extract_imported_target_keeps_imported_source() {
        let full = WastComponent {
            funcs: vec![mk_func("f1", FuncSource::Imported("f1".into()), &[], None)],
            types: vec![],
            syms: empty_syms(),
        };
        let result = extract_ok(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        );
        assert!(
            matches!(&result.funcs[0].1.source, FuncSource::Imported(_)),
            "Imported target must NOT be retagged as Exported"
        );
    }

    // ── merge: caller revalidation on Internal signature change ──

    #[test]
    fn merge_internal_sig_change_with_hidden_caller_errors() {
        let caller_body = mk_body_calling(&["f1"]);
        let mut f2 = mk_func("f2", FuncSource::Internal("f2".into()), &[], None);
        f2.1.body = Some(caller_body);
        let full = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Internal("f1".into()),
                    &[("x", "i32")],
                    None,
                ),
                f2,
            ],
            types: vec![],
            syms: empty_syms(),
        };
        // Partial changes f1's signature but does NOT include caller f2.
        let partial = WastComponent {
            funcs: vec![mk_func(
                "f1",
                FuncSource::Internal("f1".into()),
                &[("x", "bool")],
                None,
            )],
            types: vec![],
            syms: empty_syms(),
        };
        let errs = merge_impl(partial, full).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("conflict") && e.message.contains("f2")),
            "hidden caller must be reported: {errs:?}"
        );
    }

    #[test]
    fn merge_internal_sig_change_with_included_caller_ok() {
        let caller_body = mk_body_calling(&["f1"]);
        let mut full_f2 = mk_func("f2", FuncSource::Internal("f2".into()), &[], None);
        full_f2.1.body = Some(caller_body.clone());
        let full = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Internal("f1".into()),
                    &[("x", "i32")],
                    None,
                ),
                full_f2,
            ],
            types: vec![],
            syms: empty_syms(),
        };
        // Partial changes f1's signature AND includes the (revalidated)
        // caller f2 — no conflict.
        let mut partial_f2 = mk_func("f2", FuncSource::Internal("f2".into()), &[], None);
        partial_f2.1.body = Some(caller_body);
        let partial = WastComponent {
            funcs: vec![
                mk_func(
                    "f1",
                    FuncSource::Internal("f1".into()),
                    &[("x", "bool")],
                    None,
                ),
                partial_f2,
            ],
            types: vec![],
            syms: empty_syms(),
        };
        assert!(merge_impl(partial, full).is_ok());
    }

    // ── corrupt body handling ──

    #[test]
    fn merge_corrupt_body_errors_with_func_uid() {
        // 0xFF is an unsupported body-format version → deserialization fails
        // and merge must surface an error naming the func, not silently
        // treat the body as "no calls".
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(vec![0xFF, 1, 2, 3]);
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
                .any(|e| e.message.contains("f1") && e.message.contains("body")),
            "corrupt body must produce an error naming the func: {errs:?}"
        );
    }

    #[test]
    fn extract_corrupt_body_errors() {
        let mut f1 = mk_func("f1", FuncSource::Internal("f1".into()), &[], None);
        f1.1.body = Some(vec![0xFF, 1, 2, 3]);
        let full = WastComponent {
            funcs: vec![f1],
            types: vec![],
            syms: empty_syms(),
        };
        let err = extract_impl(
            full,
            vec![ExtractTarget {
                sym: "f1".into(),
                include_caller: false,
            }],
        )
        .unwrap_err();
        assert!(
            err.message.contains("f1"),
            "extract error must name the func: {}",
            err.message
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
