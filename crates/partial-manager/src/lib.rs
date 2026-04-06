#[allow(warnings)]
mod bindings;

use bindings::wast::core::types::{
    ExtractTarget, FuncSource, SymEntry, Syms, TypeSource, WastComponent, WastError, WastFunc,
    WastTypeDef,
};

struct Component;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn err(msg: impl Into<String>) -> WastError {
    WastError {
        message: msg.into(),
        location: None,
    }
}

/// Collect type UIDs referenced by a function's params and result.
fn referenced_types(f: &WastFunc) -> Vec<String> {
    let mut refs: Vec<String> = f.params.iter().map(|(_, t)| t.clone()).collect();
    if let Some(ref r) = f.result {
        refs.push(r.clone());
    }
    refs
}

/// Check if two functions have the same signature (params types + result type).
fn signatures_match(a: &WastFunc, b: &WastFunc) -> bool {
    let a_param_types: Vec<&str> = a.params.iter().map(|(_, t)| t.as_str()).collect();
    let b_param_types: Vec<&str> = b.params.iter().map(|(_, t)| t.as_str()).collect();
    a_param_types == b_param_types && a.result == b.result
}

/// Check if two type definitions are equivalent.
fn type_defs_match(a: &WastTypeDef, b: &WastTypeDef) -> bool {
    // Compare definitions structurally using Debug (not ideal but works for now)
    format!("{:?}", a.definition) == format!("{:?}", b.definition)
}

// ---------------------------------------------------------------------------
// Extract
// ---------------------------------------------------------------------------

fn extract_impl(full: WastComponent, targets: Vec<ExtractTarget>) -> WastComponent {
    let target_uids: Vec<&str> = targets.iter().map(|t| t.sym.as_str()).collect();

    // Find target funcs and collect their UIDs
    let mut included_func_uids: Vec<String> = Vec::new();
    let mut included_type_uids: Vec<String> = Vec::new();

    // Step 1: Add target funcs
    for uid in &target_uids {
        if let Some((_, func)) = full.funcs.iter().find(|(id, _)| id == uid) {
            included_func_uids.push(uid.to_string());
            // Collect referenced types
            for type_ref in referenced_types(func) {
                if !included_type_uids.contains(&type_ref) {
                    included_type_uids.push(type_ref);
                }
            }
        }
    }

    // Step 2: If include-caller, add funcs that call any target
    // (Body analysis not yet implemented — skip for now)
    for target in &targets {
        if target.include_caller {
            // TODO: analyze bodies to find callers
        }
    }

    // Step 3: Build output funcs
    let mut out_funcs: Vec<(String, WastFunc)> = Vec::new();
    for (uid, func) in &full.funcs {
        if included_func_uids.contains(uid) {
            // Target func: keep as-is
            out_funcs.push((uid.clone(), func.clone()));
        }
        // Non-target funcs referenced by targets would be added as imported
        // (requires body analysis — skipped for now)
    }

    // Step 4: Build output types
    let mut out_types: Vec<(String, WastTypeDef)> = Vec::new();
    for (uid, typedef) in &full.types {
        if included_type_uids.contains(uid) {
            out_types.push((uid.clone(), typedef.clone()));
        }
    }

    // Step 5: Build output syms — only for included UIDs
    let mut out_wit_syms: Vec<(String, String)> = Vec::new();
    let mut out_internal: Vec<SymEntry> = Vec::new();
    let mut out_local: Vec<SymEntry> = Vec::new();

    for entry in &full.syms.internal {
        if included_func_uids.contains(&entry.uid) {
            out_internal.push(entry.clone());
        }
    }

    // Include all local syms that might be used by included funcs
    // (Without body analysis, include locals for params of included funcs)
    let param_uids: Vec<String> = out_funcs
        .iter()
        .flat_map(|(_, f)| f.params.iter().map(|(uid, _)| uid.clone()))
        .collect();

    for entry in &full.syms.local {
        if param_uids.contains(&entry.uid) {
            out_local.push(entry.clone());
        }
    }

    // Include wit syms for any WIT-path func UIDs
    for (path, name) in &full.syms.wit_syms {
        if included_func_uids.iter().any(|uid| uid == path)
            || included_type_uids.iter().any(|uid| uid == path)
        {
            out_wit_syms.push((path.clone(), name.clone()));
        }
    }

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
            FuncSource::Imported(id) | FuncSource::Exported(id) => {
                // Must exist in full with matching signature
                if let Some((_, ffunc)) = full.funcs.iter().find(|(fid, _)| fid == id) {
                    if !signatures_match(pfunc, ffunc) {
                        errors.push(err(format!("signature_mismatch: func {}", id)));
                    }
                } else {
                    errors.push(err(format!(
                        "missing_dependency: imported/exported func {} not found in full",
                        id
                    )));
                }
            }
            FuncSource::Internal(id) => {
                // Check for conflict with non-internal
                if let Some((_, ffunc)) = full.funcs.iter().find(|(fid, _)| fid == id) {
                    if !matches!(&ffunc.source, FuncSource::Internal(_)) {
                        errors.push(err(format!(
                            "uid_conflict: internal func {} conflicts with existing non-internal",
                            id
                        )));
                        continue;
                    }
                }
                // Add or update
                if let Some(entry) = full.funcs.iter_mut().find(|(fid, _)| fid == id) {
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
            TypeSource::Imported(id) | TypeSource::Exported(id) => {
                if let Some((_, ftype)) = full.types.iter().find(|(fid, _)| fid == id) {
                    if !type_defs_match(ptype, ftype) {
                        errors.push(err(format!("signature_mismatch: type {}", id)));
                    }
                } else {
                    errors.push(err(format!(
                        "missing_dependency: imported/exported type {} not found in full",
                        id
                    )));
                }
            }
            TypeSource::Internal(id) => {
                if let Some((_, ftype)) = full.types.iter().find(|(fid, _)| fid == id) {
                    if !matches!(&ftype.source, TypeSource::Internal(_)) {
                        errors.push(err(format!(
                            "uid_conflict: internal type {} conflicts with existing non-internal",
                            id
                        )));
                        continue;
                    }
                }
                if let Some(entry) = full.types.iter_mut().find(|(fid, _)| fid == id) {
                    entry.1 = ptype.clone();
                } else {
                    full.types.push((uid.clone(), ptype.clone()));
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
