#[allow(warnings)]
mod bindings;

use bindings::wast::core::types::*;
use std::collections::HashMap;

struct Component;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a map from UID -> display name using the various sym tables.
fn build_func_name_map(syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in &syms.internal {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    for (uid, display) in &syms.wit_syms {
        map.insert(uid.clone(), display.clone());
    }
    map
}

fn build_local_name_map(syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in &syms.local {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    map
}

fn build_type_name_map(_types: &[(TypeUid, WastTypeDef)], syms: &Syms) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // Use internal syms for type names too
    for entry in &syms.internal {
        map.insert(entry.uid.clone(), entry.display_name.clone());
    }
    for (uid, display) in &syms.wit_syms {
        map.insert(uid.clone(), display.clone());
    }
    map
}

fn resolve_type_ref(
    type_ref: &WitTypeRef,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> String {
    // First check if there's a direct type definition we can inline
    for (uid, typedef) in types {
        if uid == type_ref {
            return format_wit_type(&typedef.definition, types, type_names);
        }
    }
    // Fall back to name map or raw UID
    type_names
        .get(type_ref.as_str())
        .cloned()
        .unwrap_or_else(|| type_ref.clone())
}

fn format_wit_type(
    wit_type: &WitType,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> String {
    match wit_type {
        WitType::Primitive(p) => primitive_name(p).to_string(),
        WitType::Option(inner) => {
            format!("option<{}>", resolve_type_ref(inner, types, type_names))
        }
        WitType::Result((ok, err)) => {
            format!(
                "result<{}, {}>",
                resolve_type_ref(ok, types, type_names),
                resolve_type_ref(err, types, type_names)
            )
        }
        WitType::List(inner) => {
            format!("list<{}>", resolve_type_ref(inner, types, type_names))
        }
        WitType::Record(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(name, tref)| {
                    let n = type_names
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    format!("{}: {}", n, resolve_type_ref(tref, types, type_names))
                })
                .collect();
            format!("record {{ {} }}", parts.join(", "))
        }
        WitType::Variant(cases) => {
            let parts: Vec<String> = cases
                .iter()
                .map(|(name, tref)| {
                    let n = type_names
                        .get(name.as_str())
                        .cloned()
                        .unwrap_or_else(|| name.clone());
                    match tref {
                        Some(t) => format!("{}({})", n, resolve_type_ref(t, types, type_names)),
                        None => n,
                    }
                })
                .collect();
            format!("variant {{ {} }}", parts.join(", "))
        }
        WitType::Tuple(refs) => {
            let parts: Vec<String> = refs
                .iter()
                .map(|r| resolve_type_ref(r, types, type_names))
                .collect();
            format!("tuple<{}>", parts.join(", "))
        }
    }
}

fn primitive_name(p: &PrimitiveType) -> &'static str {
    match p {
        PrimitiveType::U32 => "u32",
        PrimitiveType::U64 => "u64",
        PrimitiveType::I32 => "i32",
        PrimitiveType::I64 => "i64",
        PrimitiveType::F32 => "f32",
        PrimitiveType::F64 => "f64",
        PrimitiveType::Bool => "bool",
        PrimitiveType::Char => "char",
        PrimitiveType::String => "string",
    }
}

fn parse_primitive(s: &str) -> Option<PrimitiveType> {
    match s {
        "u32" => Some(PrimitiveType::U32),
        "u64" => Some(PrimitiveType::U64),
        "i32" => Some(PrimitiveType::I32),
        "i64" => Some(PrimitiveType::I64),
        "f32" => Some(PrimitiveType::F32),
        "f64" => Some(PrimitiveType::F64),
        "bool" => Some(PrimitiveType::Bool),
        "char" => Some(PrimitiveType::Char),
        "string" => Some(PrimitiveType::String),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// to_text
// ---------------------------------------------------------------------------

fn func_to_text(
    func_uid: &str,
    func: &WastFunc,
    func_names: &HashMap<String, String>,
    local_names: &HashMap<String, String>,
    type_names: &HashMap<String, String>,
    types: &[(TypeUid, WastTypeDef)],
) -> String {
    let source_uid = match &func.source {
        FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => u.clone(),
    };

    let name = func_names
        .get(&source_uid)
        .or_else(|| func_names.get(func_uid))
        .cloned()
        .unwrap_or_else(|| func_uid.to_string());

    let params_str = func
        .params
        .iter()
        .map(|(param_uid, type_ref)| {
            let pname = local_names
                .get(param_uid.as_str())
                .cloned()
                .unwrap_or_else(|| param_uid.clone());
            let tname = resolve_type_ref(type_ref, types, type_names);
            format!("{}: {}", pname, tname)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let result_str = match &func.result {
        Some(type_ref) => format!(" -> {}", resolve_type_ref(type_ref, types, type_names)),
        None => String::new(),
    };

    let signature = format!("{}({}){}", name, params_str, result_str);

    match &func.source {
        FuncSource::Imported(_) => {
            format!("# import {}", signature)
        }
        FuncSource::Exported(_) => {
            let body_line = match &func.body {
                Some(b) => format!("  # [body: {} bytes]", b.len()),
                None => "  # [no body]".to_string(),
            };
            format!("# export\ndef {}\n{}\nend", signature, body_line)
        }
        FuncSource::Internal(_) => {
            let body_line = match &func.body {
                Some(b) => format!("  # [body: {} bytes]", b.len()),
                None => "  # [no body]".to_string(),
            };
            format!("def {}\n{}\nend", signature, body_line)
        }
    }
}

// ---------------------------------------------------------------------------
// from_text — parser
// ---------------------------------------------------------------------------

struct ParsedFunc {
    name: String,
    params: Vec<(String, String)>, // (param_name, type_string)
    result_type: Option<String>,
    _is_import: bool,
    _is_export: bool,
}

fn parse_type_ref_str(
    s: &str,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
) -> WitTypeRef {
    let s = s.trim();
    // Check if it's a primitive — if so, look for a matching type UID, otherwise use the name as UID
    if parse_primitive(s).is_some() {
        // Look for an existing type entry for this primitive
        for (uid, td) in types {
            if let WitType::Primitive(p) = &td.definition {
                if primitive_name(p) == s {
                    return uid.clone();
                }
            }
        }
        // Also check type_names reverse
        for (uid, name) in type_names {
            if name == s {
                return uid.clone();
            }
        }
        // Fall back to the string itself as UID
        s.to_string()
    } else {
        // Named type — reverse lookup
        for (uid, name) in type_names {
            if name == s {
                return uid.clone();
            }
        }
        s.to_string()
    }
}

/// Parse a signature string like `name(p1: type1, p2: type2) -> ret`
fn parse_signature(sig: &str) -> Option<ParsedFunc> {
    let sig = sig.trim();

    let paren_open = sig.find('(')?;
    let name = sig[..paren_open].trim().to_string();
    if name.is_empty() {
        return None;
    }

    // Find matching closing paren
    let rest = &sig[paren_open + 1..];
    let paren_close = rest.find(')')?;
    let params_str = &rest[..paren_close];
    let after_params = rest[paren_close + 1..].trim();

    let params: Vec<(String, String)> = if params_str.trim().is_empty() {
        vec![]
    } else {
        params_str
            .split(',')
            .map(|p| {
                let p = p.trim();
                if let Some(colon) = p.find(':') {
                    (
                        p[..colon].trim().to_string(),
                        p[colon + 1..].trim().to_string(),
                    )
                } else {
                    (p.to_string(), "unknown".to_string())
                }
            })
            .collect()
    };

    let result_type = if after_params.starts_with("->") {
        Some(after_params[2..].trim().to_string())
    } else {
        None
    };

    Some(ParsedFunc {
        name,
        params,
        result_type,
        _is_import: false,
        _is_export: false,
    })
}

fn generate_uid() -> String {
    // Simple deterministic-ish UID from a counter mixed with some bits.
    // In a real implementation this would use randomness; for wasm32 we use
    // a simple static counter approach.
    use core::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0xa000);
    let val = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:04x}", val & 0xffff)
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::syntax_plugin::Guest for Component {
    fn to_text(component: WastComponent) -> String {
        let func_names = build_func_name_map(&component.syms);
        let local_names = build_local_name_map(&component.syms);
        let type_names = build_type_name_map(&component.types, &component.syms);

        let mut parts: Vec<String> = Vec::new();

        for (func_uid, func) in &component.funcs {
            parts.push(func_to_text(
                func_uid,
                func,
                &func_names,
                &local_names,
                &type_names,
                &component.types,
            ));
        }

        parts.join("\n\n")
    }

    fn from_text(text: String, existing: WastComponent) -> Result<WastComponent, Vec<WastError>> {
        let func_names = build_func_name_map(&existing.syms);
        let local_names = build_local_name_map(&existing.syms);
        let type_names = build_type_name_map(&existing.types, &existing.syms);

        // Reverse maps: display_name -> uid
        let rev_func: HashMap<String, String> = func_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();
        let rev_local: HashMap<String, String> = local_names
            .iter()
            .map(|(k, v)| (v.clone(), k.clone()))
            .collect();

        // Existing funcs by uid for body preservation
        let existing_funcs: HashMap<String, &WastFunc> = existing
            .funcs
            .iter()
            .map(|(uid, f)| (uid.clone(), f))
            .collect();

        // Also build a map from source uid -> (func_uid, func)
        let existing_by_source: HashMap<String, (&str, &WastFunc)> = existing
            .funcs
            .iter()
            .map(|(uid, f)| {
                let src = match &f.source {
                    FuncSource::Internal(u) | FuncSource::Imported(u) | FuncSource::Exported(u) => {
                        u.clone()
                    }
                };
                (src, (uid.as_str(), f))
            })
            .collect();

        let mut errors: Vec<WastError> = Vec::new();
        let mut funcs: Vec<(FuncUid, WastFunc)> = Vec::new();
        let mut new_syms_internal: Vec<SymEntry> = existing.syms.internal.clone();
        let mut new_syms_local: Vec<SymEntry> = existing.syms.local.clone();

        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();

            // Skip empty lines and pure comments that aren't import/export markers
            if line.is_empty() {
                i += 1;
                continue;
            }

            // Handle import comment: # import name(params) -> result
            if line.starts_with("# import ") {
                let sig_str = &line["# import ".len()..];
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source, true);

                        let params = resolve_params(
                            &parsed.params,
                            &rev_local,
                            &existing.types,
                            &type_names,
                            &mut new_syms_local,
                        );
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                        // Preserve existing body if available
                        let body = existing_by_source
                            .get(&source_uid)
                            .and_then(|(_, f)| f.body.clone())
                            .or_else(|| existing_funcs.get(&func_uid).and_then(|f| f.body.clone()));

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source: FuncSource::Imported(source_uid),
                                params,
                                result,
                                body,
                            },
                        ));
                    }
                    None => {
                        errors.push(WastError {
                            message: format!(
                                "parse_error: cannot parse import signature: {}",
                                line
                            ),
                            location: Some(format!("line {}", i + 1)),
                        });
                    }
                }
                i += 1;
                continue;
            }

            // Handle export marker + def
            if line == "# export" {
                i += 1;
                // Next non-empty line should be a def
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                if i < lines.len() && lines[i].trim().starts_with("def ") {
                    let def_line = lines[i].trim();
                    let sig_str = &def_line["def ".len()..];
                    match parse_signature(sig_str) {
                        Some(parsed) => {
                            // Consume body until `end`
                            i += 1;
                            while i < lines.len() && lines[i].trim() != "end" {
                                i += 1;
                            }
                            if i < lines.len() {
                                i += 1; // skip 'end'
                            }

                            let (func_uid, source_uid) = resolve_func_uid(
                                &parsed.name,
                                &rev_func,
                                &existing_by_source,
                                false,
                            );

                            let params = resolve_params(
                                &parsed.params,
                                &rev_local,
                                &existing.types,
                                &type_names,
                                &mut new_syms_local,
                            );
                            let result = parsed
                                .result_type
                                .as_ref()
                                .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                            let body = existing_by_source
                                .get(&source_uid)
                                .and_then(|(_, f)| f.body.clone())
                                .or_else(|| {
                                    existing_funcs.get(&func_uid).and_then(|f| f.body.clone())
                                });

                            // Ensure sym entry exists
                            ensure_func_sym(&source_uid, &parsed.name, &mut new_syms_internal);

                            funcs.push((
                                func_uid,
                                WastFunc {
                                    source: FuncSource::Exported(source_uid),
                                    params,
                                    result,
                                    body,
                                },
                            ));
                        }
                        None => {
                            errors.push(WastError {
                                message: format!(
                                    "parse_error: cannot parse def after export: {}",
                                    def_line
                                ),
                                location: Some(format!("line {}", i + 1)),
                            });
                            i += 1;
                        }
                    }
                } else {
                    errors.push(WastError {
                        message: "parse_error: expected 'def' after '# export'".to_string(),
                        location: Some(format!("line {}", i + 1)),
                    });
                }
                continue;
            }

            // Handle plain def (internal)
            if line.starts_with("def ") {
                let sig_str = &line["def ".len()..];
                match parse_signature(sig_str) {
                    Some(parsed) => {
                        // Consume body until `end`
                        i += 1;
                        while i < lines.len() && lines[i].trim() != "end" {
                            i += 1;
                        }
                        if i < lines.len() {
                            i += 1; // skip 'end'
                        }

                        let (func_uid, source_uid) =
                            resolve_func_uid(&parsed.name, &rev_func, &existing_by_source, false);

                        let params = resolve_params(
                            &parsed.params,
                            &rev_local,
                            &existing.types,
                            &type_names,
                            &mut new_syms_local,
                        );
                        let result = parsed
                            .result_type
                            .as_ref()
                            .map(|r| parse_type_ref_str(r, &existing.types, &type_names));

                        let body = existing_by_source
                            .get(&source_uid)
                            .and_then(|(_, f)| f.body.clone())
                            .or_else(|| existing_funcs.get(&func_uid).and_then(|f| f.body.clone()));

                        ensure_func_sym(&source_uid, &parsed.name, &mut new_syms_internal);

                        funcs.push((
                            func_uid,
                            WastFunc {
                                source: FuncSource::Internal(source_uid),
                                params,
                                result,
                                body,
                            },
                        ));
                    }
                    None => {
                        errors.push(WastError {
                            message: format!("parse_error: cannot parse def: {}", line),
                            location: Some(format!("line {}", i + 1)),
                        });
                        i += 1;
                    }
                }
                continue;
            }

            // Skip comment lines that aren't structural
            if line.starts_with('#') {
                i += 1;
                continue;
            }

            // Unrecognized line
            errors.push(WastError {
                message: format!("parse_error: unexpected line: {}", line),
                location: Some(format!("line {}", i + 1)),
            });
            i += 1;
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(WastComponent {
            funcs,
            types: existing.types,
            syms: Syms {
                wit_syms: existing.syms.wit_syms,
                internal: new_syms_internal,
                local: new_syms_local,
            },
        })
    }
}

/// Resolve a function name to (func_uid, source_uid).
/// For known names, use existing UIDs; for new names, generate UIDs.
fn resolve_func_uid(
    name: &str,
    rev_func: &HashMap<String, String>,
    existing_by_source: &HashMap<String, (&str, &WastFunc)>,
    _is_import: bool,
) -> (String, String) {
    if let Some(source_uid) = rev_func.get(name) {
        // Found source uid from sym table. Find the func_uid from existing.
        if let Some((func_uid, _)) = existing_by_source.get(source_uid.as_str()) {
            (func_uid.to_string(), source_uid.clone())
        } else {
            // Source uid known but not in existing funcs — use source_uid as func_uid too
            (source_uid.clone(), source_uid.clone())
        }
    } else {
        // New function — generate UIDs
        let uid = generate_uid();
        (uid.clone(), uid)
    }
}

/// Resolve parameter names and types from parsed strings.
fn resolve_params(
    parsed: &[(String, String)],
    rev_local: &HashMap<String, String>,
    types: &[(TypeUid, WastTypeDef)],
    type_names: &HashMap<String, String>,
    new_syms_local: &mut Vec<SymEntry>,
) -> Vec<(FuncUid, WitTypeRef)> {
    parsed
        .iter()
        .map(|(pname, ptype)| {
            let param_uid = if let Some(uid) = rev_local.get(pname.as_str()) {
                uid.clone()
            } else {
                // New param — generate uid and add sym
                let uid = generate_uid();
                new_syms_local.push(SymEntry {
                    uid: uid.clone(),
                    display_name: pname.clone(),
                });
                uid
            };
            let type_ref = parse_type_ref_str(ptype, types, type_names);
            (param_uid, type_ref)
        })
        .collect()
}

/// Ensure a sym entry exists for a function.
fn ensure_func_sym(source_uid: &str, name: &str, syms_internal: &mut Vec<SymEntry>) {
    if !syms_internal.iter().any(|e| e.uid == source_uid) {
        syms_internal.push(SymEntry {
            uid: source_uid.to_string(),
            display_name: name.to_string(),
        });
    }
}

bindings::export!(Component with_types_in bindings);
