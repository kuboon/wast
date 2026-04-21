#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

mod syms_yaml;
mod wit_parser;

use bindings::wast::core::types::{
    ExtractTarget, FuncSource, PrimitiveType, SymEntry, Syms, TypeSource, WastComponent, WastError,
    WastFunc, WastTypeDef, WitType,
};

struct Component;

/// Default language for syms files.
const DEFAULT_LANG: &str = "en";

// ---------------------------------------------------------------------------
// Helper: create a WastError from a message string
// ---------------------------------------------------------------------------
fn err(msg: impl Into<String>) -> WastError {
    WastError {
        message: msg.into(),
        location: None,
    }
}

fn err_at(msg: impl Into<String>, loc: impl Into<String>) -> WastError {
    WastError {
        message: msg.into(),
        location: Some(loc.into()),
    }
}

// ---------------------------------------------------------------------------
// Conversions: bindings <-> serde mirror types
// ---------------------------------------------------------------------------

fn prim_to_serde(p: &PrimitiveType) -> wast_types::PrimitiveType {
    match p {
        PrimitiveType::U32 => wast_types::PrimitiveType::U32,
        PrimitiveType::U64 => wast_types::PrimitiveType::U64,
        PrimitiveType::I32 => wast_types::PrimitiveType::I32,
        PrimitiveType::I64 => wast_types::PrimitiveType::I64,
        PrimitiveType::F32 => wast_types::PrimitiveType::F32,
        PrimitiveType::F64 => wast_types::PrimitiveType::F64,
        PrimitiveType::Bool => wast_types::PrimitiveType::Bool,
        PrimitiveType::Char => wast_types::PrimitiveType::Char,
        PrimitiveType::String => wast_types::PrimitiveType::String,
    }
}

fn prim_from_serde(p: &wast_types::PrimitiveType) -> PrimitiveType {
    match p {
        wast_types::PrimitiveType::U32 => PrimitiveType::U32,
        wast_types::PrimitiveType::U64 => PrimitiveType::U64,
        wast_types::PrimitiveType::I32 => PrimitiveType::I32,
        wast_types::PrimitiveType::I64 => PrimitiveType::I64,
        wast_types::PrimitiveType::F32 => PrimitiveType::F32,
        wast_types::PrimitiveType::F64 => PrimitiveType::F64,
        wast_types::PrimitiveType::Bool => PrimitiveType::Bool,
        wast_types::PrimitiveType::Char => PrimitiveType::Char,
        wast_types::PrimitiveType::String => PrimitiveType::String,
    }
}

fn wit_type_to_serde(t: &WitType) -> wast_types::WitType {
    match t {
        WitType::Primitive(p) => wast_types::WitType::Primitive(prim_to_serde(p)),
        WitType::Option(r) => wast_types::WitType::Option(r.clone()),
        WitType::Result((a, b)) => wast_types::WitType::Result(a.clone(), b.clone()),
        WitType::List(r) => wast_types::WitType::List(r.clone()),
        WitType::Record(fields) => wast_types::WitType::Record(fields.clone()),
        WitType::Variant(cases) => wast_types::WitType::Variant(cases.clone()),
        WitType::Tuple(elems) => wast_types::WitType::Tuple(elems.clone()),
    }
}

fn wit_type_from_serde(t: &wast_types::WitType) -> WitType {
    match t {
        wast_types::WitType::Primitive(p) => WitType::Primitive(prim_from_serde(p)),
        wast_types::WitType::Option(r) => WitType::Option(r.clone()),
        wast_types::WitType::Result(a, b) => WitType::Result((a.clone(), b.clone())),
        wast_types::WitType::List(r) => WitType::List(r.clone()),
        wast_types::WitType::Record(fields) => WitType::Record(fields.clone()),
        wast_types::WitType::Variant(cases) => WitType::Variant(cases.clone()),
        wast_types::WitType::Tuple(elems) => WitType::Tuple(elems.clone()),
    }
}

fn func_source_to_serde(s: &FuncSource) -> wast_types::FuncSource {
    match s {
        FuncSource::Internal(id) => wast_types::FuncSource::Internal(id.clone()),
        FuncSource::Imported(id) => wast_types::FuncSource::Imported(id.clone()),
        FuncSource::Exported(id) => wast_types::FuncSource::Exported(id.clone()),
    }
}

fn func_source_from_serde(s: &wast_types::FuncSource) -> FuncSource {
    match s {
        wast_types::FuncSource::Internal(id) => FuncSource::Internal(id.clone()),
        wast_types::FuncSource::Imported(id) => FuncSource::Imported(id.clone()),
        wast_types::FuncSource::Exported(id) => FuncSource::Exported(id.clone()),
    }
}

fn type_source_to_serde(s: &TypeSource) -> wast_types::TypeSource {
    match s {
        TypeSource::Internal(id) => wast_types::TypeSource::Internal(id.clone()),
        TypeSource::Imported(id) => wast_types::TypeSource::Imported(id.clone()),
        TypeSource::Exported(id) => wast_types::TypeSource::Exported(id.clone()),
    }
}

fn type_source_from_serde(s: &wast_types::TypeSource) -> TypeSource {
    match s {
        wast_types::TypeSource::Internal(id) => TypeSource::Internal(id.clone()),
        wast_types::TypeSource::Imported(id) => TypeSource::Imported(id.clone()),
        wast_types::TypeSource::Exported(id) => TypeSource::Exported(id.clone()),
    }
}

/// Convert WastComponent to on-disk format (no syms — those go in syms.*.yaml).
fn component_to_db(c: &WastComponent) -> wast_types::WastDb {
    wast_types::WastDb {
        funcs: c
            .funcs
            .iter()
            .map(|(uid, f)| wast_types::WastFuncRow {
                uid: uid.clone(),
                func: wast_types::WastFunc {
                    source: func_source_to_serde(&f.source),
                    params: f.params.clone(),
                    result: f.result.clone(),
                    body: f.body.clone(),
                },
            })
            .collect(),
        types: c
            .types
            .iter()
            .map(|(uid, td)| wast_types::WastTypeRow {
                uid: uid.clone(),
                def: wast_types::WastTypeDef {
                    source: type_source_to_serde(&td.source),
                    definition: wit_type_to_serde(&td.definition),
                },
            })
            .collect(),
    }
}

/// Convert on-disk format to WastComponent (syms empty — loaded separately from YAML).
fn component_from_db(db: &wast_types::WastDb) -> WastComponent {
    WastComponent {
        funcs: db
            .funcs
            .iter()
            .map(|row| {
                (
                    row.uid.clone(),
                    WastFunc {
                        source: func_source_from_serde(&row.func.source),
                        params: row.func.params.clone(),
                        result: row.func.result.clone(),
                        body: row.func.body.clone(),
                    },
                )
            })
            .collect(),
        types: db
            .types
            .iter()
            .map(|row| {
                (
                    row.uid.clone(),
                    WastTypeDef {
                        source: type_source_from_serde(&row.def.source),
                        definition: wit_type_from_serde(&row.def.definition),
                    },
                )
            })
            .collect(),
        syms: Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        },
    }
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

fn db_path(path: &str) -> String {
    format!("{}/wast.json", path)
}

fn wit_path(path: &str) -> String {
    format!("{}/world.wit", path)
}

fn syms_path(path: &str, lang: &str) -> String {
    format!("{}/syms.{}.yaml", path, lang)
}

// ---------------------------------------------------------------------------
// Syms file I/O
// ---------------------------------------------------------------------------

fn read_syms(path: &str, lang: &str) -> wast_types::Syms {
    let p = syms_path(path, lang);
    match std::fs::read_to_string(&p) {
        Ok(data) => syms_yaml::parse_syms_yaml(&data).unwrap_or_else(|_| wast_types::Syms {
            wit_syms: Vec::new(),
            internal: Vec::new(),
            local: Vec::new(),
        }),
        Err(_) => wast_types::Syms {
            wit_syms: Vec::new(),
            internal: Vec::new(),
            local: Vec::new(),
        },
    }
}

fn write_syms(path: &str, lang: &str, syms: &wast_types::Syms) -> Result<(), WastError> {
    let p = syms_path(path, lang);
    let yaml = syms_yaml::write_syms_yaml(syms);
    std::fs::write(&p, yaml.as_bytes())
        .map_err(|e| err_at(format!("failed to write {}: {}", p, e), p))?;
    Ok(())
}

fn syms_to_serde(syms: &Syms) -> wast_types::Syms {
    wast_types::Syms {
        wit_syms: syms.wit_syms.clone(),
        internal: syms
            .internal
            .iter()
            .map(|e| wast_types::SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
        local: syms
            .local
            .iter()
            .map(|e| wast_types::SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
    }
}

fn syms_from_serde(s: &wast_types::Syms) -> Syms {
    Syms {
        wit_syms: s.wit_syms.clone(),
        internal: s
            .internal
            .iter()
            .map(|e| SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
        local: s
            .local
            .iter()
            .map(|e| SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// world.wit validation
// ---------------------------------------------------------------------------

/// Validate that the WastComponent's exported and imported funcs are consistent
/// with what `world.wit` declares. Internal funcs are not checked.
fn validate_against_wit(path: &str, component: &WastComponent) -> Result<(), WastError> {
    let wp = wit_path(path);
    if !std::path::Path::new(&wp).exists() {
        return Err(WastError {
            message: "wit_not_found".to_string(),
            location: Some(wp.clone()),
        });
    }

    let wit_src = std::fs::read_to_string(&wp)
        .map_err(|e| err_at(format!("failed to read {}: {}", wp, e), wp.clone()))?;
    let parsed = wit_parser::parse_world(&wit_src)
        .map_err(|e| err_at(format!("wit parse error: {}", e), wp))?;

    // Build lookup maps: wit_path -> param count
    let wit_exports: std::collections::HashMap<&str, usize> = parsed
        .exports
        .iter()
        .map(|f| (f.wit_path.as_str(), f.params.len()))
        .collect();

    let wit_imports: std::collections::HashMap<&str, usize> = parsed
        .imports
        .iter()
        .map(|f| (f.wit_path.as_str(), f.params.len()))
        .collect();

    for (uid, func) in &component.funcs {
        match &func.source {
            FuncSource::Exported(wit_id) => match wit_exports.get(wit_id.as_str()) {
                None => {
                    return Err(err(format!(
                        "wit_inconsistency: exported func {} not found in world.wit",
                        uid
                    )));
                }
                Some(&expected_params) => {
                    if func.params.len() != expected_params {
                        return Err(err(format!(
                            "wit_inconsistency: func {} param count mismatch",
                            uid
                        )));
                    }
                }
            },
            FuncSource::Imported(wit_id) => match wit_imports.get(wit_id.as_str()) {
                None => {
                    return Err(err(format!(
                        "wit_inconsistency: imported func {} not found in world.wit",
                        uid
                    )));
                }
                Some(&expected_params) => {
                    if func.params.len() != expected_params {
                        return Err(err(format!(
                            "wit_inconsistency: func {} param count mismatch",
                            uid
                        )));
                    }
                }
            },
            FuncSource::Internal(_) => {
                // Internal funcs are not in WIT — skip validation
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Component file I/O (wast.json + syms)
// ---------------------------------------------------------------------------

fn read_component_from_disk(path: &str) -> Result<WastComponent, WastError> {
    let db = db_path(path);

    // Check if wast.json exists; if not, report db_not_found
    if !std::path::Path::new(&db).exists() {
        return Err(WastError {
            message: "db_not_found".to_string(),
            location: Some(db),
        });
    }

    let data = std::fs::read_to_string(&db)
        .map_err(|e| err_at(format!("failed to read {}: {}", db, e), db.clone()))?;
    let sc: wast_types::WastDb = serde_json::from_str(&data)
        .map_err(|e| err_at(format!("invalid JSON in {}: {}", db, e), db))?;
    let mut component = component_from_db(&sc);

    // Read syms from YAML and merge into the component's syms
    let file_syms = read_syms(path, DEFAULT_LANG);
    let file_syms_binding = syms_from_serde(&file_syms);
    merge_syms_into(&mut component.syms, &file_syms_binding);

    Ok(component)
}

/// Merge file-based syms into a component's syms. File syms take precedence for
/// matching keys/uids.
fn merge_syms_into(target: &mut Syms, source: &Syms) {
    // Merge wit_syms
    for (k, v) in &source.wit_syms {
        if let Some(existing) = target.wit_syms.iter_mut().find(|(ek, _)| ek == k) {
            existing.1 = v.clone();
        } else {
            target.wit_syms.push((k.clone(), v.clone()));
        }
    }

    // Merge internal
    for e in &source.internal {
        if let Some(existing) = target.internal.iter_mut().find(|x| x.uid == e.uid) {
            existing.display_name = e.display_name.clone();
        } else {
            target.internal.push(SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            });
        }
    }

    // Merge local
    for e in &source.local {
        if let Some(existing) = target.local.iter_mut().find(|x| x.uid == e.uid) {
            existing.display_name = e.display_name.clone();
        } else {
            target.local.push(SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            });
        }
    }
}

fn write_component_to_disk(path: &str, component: &WastComponent) -> Result<(), WastError> {
    // Ensure the directory exists
    std::fs::create_dir_all(path)
        .map_err(|e| err(format!("failed to create directory {}: {}", path, e)))?;

    let db = db_path(path);
    let sc = component_to_db(component);
    let json = serde_json::to_string_pretty(&sc)
        .map_err(|e| err(format!("JSON serialization error: {}", e)))?;
    std::fs::write(&db, json.as_bytes())
        .map_err(|e| err_at(format!("failed to write {}: {}", db, e), db))?;

    // Write syms YAML file
    let serde_syms = syms_to_serde(&component.syms);
    write_syms(path, DEFAULT_LANG, &serde_syms)?;

    Ok(())
}

/// Filter a component to only include funcs matching the given targets.
/// This is a simple filter — a more sophisticated implementation would
/// also include transitive dependencies (callers, referenced types, etc.).
fn filter_by_targets(component: WastComponent, targets: &[ExtractTarget]) -> WastComponent {
    let target_syms: std::collections::HashSet<&str> =
        targets.iter().map(|t| t.sym.as_str()).collect();

    let filtered_funcs: Vec<_> = component
        .funcs
        .into_iter()
        .filter(|(uid, _)| target_syms.contains(uid.as_str()))
        .collect();

    // Collect type refs used by the filtered funcs so we keep only relevant types
    let mut used_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_, f) in &filtered_funcs {
        for (_, tref) in &f.params {
            used_types.insert(tref.clone());
        }
        if let Some(ref r) = f.result {
            used_types.insert(r.clone());
        }
    }

    let filtered_types: Vec<_> = component
        .types
        .into_iter()
        .filter(|(uid, _)| used_types.contains(uid.as_str()))
        .collect();

    WastComponent {
        funcs: filtered_funcs,
        types: filtered_types,
        syms: component.syms,
    }
}

/// Merge `partial` into `full`. Funcs/types in partial overwrite those
/// with the same UID in full; new entries are appended. Syms are merged
/// by deduplicating on uid.
fn merge_components(full: WastComponent, partial: WastComponent) -> WastComponent {
    // Merge funcs: partial overwrites full for matching UIDs
    let mut func_map: Vec<(String, WastFunc)> = Vec::new();
    let partial_func_uids: std::collections::HashSet<String> =
        partial.funcs.iter().map(|(uid, _)| uid.clone()).collect();

    // Keep full funcs that aren't overridden
    for (uid, f) in full.funcs {
        if !partial_func_uids.contains(&uid) {
            func_map.push((uid, f));
        }
    }
    // Add all partial funcs
    func_map.extend(partial.funcs);

    // Merge types similarly
    let mut type_map: Vec<(String, WastTypeDef)> = Vec::new();
    let partial_type_uids: std::collections::HashSet<String> =
        partial.types.iter().map(|(uid, _)| uid.clone()).collect();

    for (uid, td) in full.types {
        if !partial_type_uids.contains(&uid) {
            type_map.push((uid, td));
        }
    }
    type_map.extend(partial.types);

    // Merge syms: combine and deduplicate
    let mut wit_syms = full.syms.wit_syms;
    for entry in partial.syms.wit_syms {
        if !wit_syms.iter().any(|(k, _)| k == &entry.0) {
            wit_syms.push(entry);
        }
    }

    let internal = merge_sym_entries(full.syms.internal, partial.syms.internal);
    let local = merge_sym_entries(full.syms.local, partial.syms.local);

    WastComponent {
        funcs: func_map,
        types: type_map,
        syms: Syms {
            wit_syms,
            internal,
            local,
        },
    }
}

fn merge_sym_entries(mut base: Vec<SymEntry>, overlay: Vec<SymEntry>) -> Vec<SymEntry> {
    for entry in overlay {
        if let Some(existing) = base.iter_mut().find(|e| e.uid == entry.uid) {
            existing.display_name = entry.display_name;
        } else {
            base.push(entry);
        }
    }
    base
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

impl bindings::exports::wast::core::file_manager::Guest for Component {
    fn bindgen(path: String) -> Result<(), WastError> {
        // Check world.wit exists
        let wit = wit_path(&path);
        if !std::path::Path::new(&wit).exists() {
            return Err(err("wit_not_found: world.wit does not exist"));
        }

        // Check wast.json does NOT exist
        let db = db_path(&path);
        if std::path::Path::new(&db).exists() {
            return Err(err("db_exists: wast.json already exists"));
        }

        // Parse world.wit
        let wit_src = std::fs::read_to_string(&wit)
            .map_err(|e| err_at(format!("failed to read {}: {}", wit, e), wit.clone()))?;
        let parsed = wit_parser::parse_world(&wit_src)
            .map_err(|e| err_at(format!("wit parse error: {}", e), wit))?;

        // Build funcs, types, and wit_syms from parsed world
        let mut funcs: Vec<(String, WastFunc)> = Vec::new();
        let mut types: Vec<(String, WastTypeDef)> = Vec::new();
        let mut wit_syms: Vec<(String, String)> = Vec::new();
        let mut seen_types: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Helper closure: ensure a primitive type entry exists for a given WIT type name.
        let ensure_type = |type_name: &str,
                           types: &mut Vec<(String, WastTypeDef)>,
                           seen: &mut std::collections::HashSet<String>| {
            if !seen.contains(type_name) {
                if let Some(prim) = wit_parser::to_primitive_type(type_name) {
                    seen.insert(type_name.to_string());
                    types.push((
                        type_name.to_string(),
                        WastTypeDef {
                            source: TypeSource::Imported(type_name.to_string()),
                            definition: WitType::Primitive(prim),
                        },
                    ));
                }
            }
        };

        // Process imported funcs
        for f in &parsed.imports {
            for (_, tname) in &f.params {
                ensure_type(tname, &mut types, &mut seen_types);
            }
            if let Some(ref ret) = f.result {
                ensure_type(ret, &mut types, &mut seen_types);
            }

            funcs.push((
                f.wit_path.clone(),
                WastFunc {
                    source: FuncSource::Imported(f.wit_path.clone()),
                    params: f
                        .params
                        .iter()
                        .map(|(pname, ptype)| (pname.clone(), ptype.clone()))
                        .collect(),
                    result: f.result.clone(),
                    body: None,
                },
            ));
            wit_syms.push((f.wit_path.clone(), f.name.clone()));
        }

        // Process exported funcs
        for f in &parsed.exports {
            for (_, tname) in &f.params {
                ensure_type(tname, &mut types, &mut seen_types);
            }
            if let Some(ref ret) = f.result {
                ensure_type(ret, &mut types, &mut seen_types);
            }

            funcs.push((
                f.wit_path.clone(),
                WastFunc {
                    source: FuncSource::Exported(f.wit_path.clone()),
                    params: f
                        .params
                        .iter()
                        .map(|(pname, ptype)| (pname.clone(), ptype.clone()))
                        .collect(),
                    result: f.result.clone(),
                    body: None,
                },
            ));
            wit_syms.push((f.wit_path.clone(), f.name.clone()));
        }

        let component = WastComponent {
            funcs,
            types,
            syms: Syms {
                wit_syms,
                internal: vec![],
                local: vec![],
            },
        };

        write_component_to_disk(&path, &component)?;
        Ok(())
    }

    fn read(path: String, targets: Option<Vec<ExtractTarget>>) -> Result<WastComponent, WastError> {
        let component = read_component_from_disk(&path)?;

        match targets {
            Some(ref t) if !t.is_empty() => Ok(filter_by_targets(component, t)),
            _ => Ok(component),
        }
    }

    fn write(path: String, component: WastComponent) -> Result<(), WastError> {
        // Validate component against world.wit
        validate_against_wit(&path, &component)?;

        write_component_to_disk(&path, &component)?;
        Ok(())
    }

    fn merge(path: String, partial: WastComponent) -> Result<(), WastError> {
        // Validate partial component against world.wit
        validate_against_wit(&path, &partial)?;

        let full = read_component_from_disk(&path)?;
        let merged = merge_components(full, partial);
        write_component_to_disk(&path, &merged)?;
        Ok(())
    }
}

bindings::export!(Component with_types_in bindings);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temp directory with a world.wit file and return its path.
    fn setup_dir(wit_content: &str) -> String {
        let dir = std::env::temp_dir().join(format!("wast_fm_test_{}", std::process::id()));
        // Use a unique sub-dir per test to avoid collisions
        let dir = dir.join(format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("world.wit"), wit_content).unwrap();
        dir.to_string_lossy().to_string()
    }

    fn cleanup_dir(path: &str) {
        let _ = std::fs::remove_dir_all(path);
    }

    fn make_component(funcs: Vec<(String, WastFunc)>) -> WastComponent {
        WastComponent {
            funcs,
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
        }
    }

    const SAMPLE_WIT: &str = r#"
package test:pkg;

world bot {
  import log: func(msg: string);
  export handle-event: func(event-id: u32) -> bool;
}
"#;

    #[test]
    fn validate_matching_wit_passes() {
        let dir = setup_dir(SAMPLE_WIT);
        let component = make_component(vec![
            (
                "handle-event".to_string(),
                WastFunc {
                    source: FuncSource::Exported("handle-event".to_string()),
                    params: vec![("event-id".to_string(), "u32".to_string())],
                    result: Some("bool".to_string()),
                    body: None,
                },
            ),
            (
                "log".to_string(),
                WastFunc {
                    source: FuncSource::Imported("log".to_string()),
                    params: vec![("msg".to_string(), "string".to_string())],
                    result: None,
                    body: None,
                },
            ),
            (
                "helper".to_string(),
                WastFunc {
                    source: FuncSource::Internal("helper".to_string()),
                    params: vec![],
                    result: None,
                    body: None,
                },
            ),
        ]);

        let result = validate_against_wit(&dir, &component);
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        cleanup_dir(&dir);
    }

    #[test]
    fn validate_mismatched_export_fails() {
        let dir = setup_dir(SAMPLE_WIT);
        // Export func not declared in world.wit
        let component = make_component(vec![(
            "unknown-export".to_string(),
            WastFunc {
                source: FuncSource::Exported("unknown-export".to_string()),
                params: vec![],
                result: None,
                body: None,
            },
        )]);

        let result = validate_against_wit(&dir, &component);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("wit_inconsistency") && msg.contains("not found in world.wit"),
            "unexpected error message: {}",
            msg
        );
        cleanup_dir(&dir);
    }

    #[test]
    fn validate_missing_import_fails() {
        let dir = setup_dir(SAMPLE_WIT);
        // Import func not declared in world.wit
        let component = make_component(vec![(
            "missing-import".to_string(),
            WastFunc {
                source: FuncSource::Imported("missing-import".to_string()),
                params: vec![],
                result: None,
                body: None,
            },
        )]);

        let result = validate_against_wit(&dir, &component);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("wit_inconsistency") && msg.contains("not found in world.wit"),
            "unexpected error message: {}",
            msg
        );
        cleanup_dir(&dir);
    }

    #[test]
    fn validate_param_count_mismatch_fails() {
        let dir = setup_dir(SAMPLE_WIT);
        // Export with wrong param count (handle-event expects 1 param)
        let component = make_component(vec![(
            "handle-event".to_string(),
            WastFunc {
                source: FuncSource::Exported("handle-event".to_string()),
                params: vec![
                    ("a".to_string(), "u32".to_string()),
                    ("b".to_string(), "u32".to_string()),
                ],
                result: Some("bool".to_string()),
                body: None,
            },
        )]);

        let result = validate_against_wit(&dir, &component);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("wit_inconsistency") && msg.contains("param count mismatch"),
            "unexpected error message: {}",
            msg
        );
        cleanup_dir(&dir);
    }

    #[test]
    fn validate_wit_not_found() {
        let dir = std::env::temp_dir().join(format!("wast_fm_noexist_{}", std::process::id()));
        let path = dir.to_string_lossy().to_string();
        let component = make_component(vec![]);
        let result = validate_against_wit(&path, &component);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("wit_not_found"));
    }
}
