#[allow(warnings)]
mod bindings;

mod serde_types;
mod syms_yaml;

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

fn prim_to_serde(p: &PrimitiveType) -> serde_types::PrimitiveType {
    match p {
        PrimitiveType::U32 => serde_types::PrimitiveType::U32,
        PrimitiveType::U64 => serde_types::PrimitiveType::U64,
        PrimitiveType::I32 => serde_types::PrimitiveType::I32,
        PrimitiveType::I64 => serde_types::PrimitiveType::I64,
        PrimitiveType::F32 => serde_types::PrimitiveType::F32,
        PrimitiveType::F64 => serde_types::PrimitiveType::F64,
        PrimitiveType::Bool => serde_types::PrimitiveType::Bool,
        PrimitiveType::Char => serde_types::PrimitiveType::Char,
        PrimitiveType::String => serde_types::PrimitiveType::String,
    }
}

fn prim_from_serde(p: &serde_types::PrimitiveType) -> PrimitiveType {
    match p {
        serde_types::PrimitiveType::U32 => PrimitiveType::U32,
        serde_types::PrimitiveType::U64 => PrimitiveType::U64,
        serde_types::PrimitiveType::I32 => PrimitiveType::I32,
        serde_types::PrimitiveType::I64 => PrimitiveType::I64,
        serde_types::PrimitiveType::F32 => PrimitiveType::F32,
        serde_types::PrimitiveType::F64 => PrimitiveType::F64,
        serde_types::PrimitiveType::Bool => PrimitiveType::Bool,
        serde_types::PrimitiveType::Char => PrimitiveType::Char,
        serde_types::PrimitiveType::String => PrimitiveType::String,
    }
}

fn wit_type_to_serde(t: &WitType) -> serde_types::WitType {
    match t {
        WitType::Primitive(p) => serde_types::WitType::Primitive(prim_to_serde(p)),
        WitType::Option(r) => serde_types::WitType::Option(r.clone()),
        WitType::Result((a, b)) => serde_types::WitType::Result(a.clone(), b.clone()),
        WitType::List(r) => serde_types::WitType::List(r.clone()),
        WitType::Record(fields) => serde_types::WitType::Record(fields.clone()),
        WitType::Variant(cases) => serde_types::WitType::Variant(cases.clone()),
        WitType::Tuple(elems) => serde_types::WitType::Tuple(elems.clone()),
    }
}

fn wit_type_from_serde(t: &serde_types::WitType) -> WitType {
    match t {
        serde_types::WitType::Primitive(p) => WitType::Primitive(prim_from_serde(p)),
        serde_types::WitType::Option(r) => WitType::Option(r.clone()),
        serde_types::WitType::Result(a, b) => WitType::Result((a.clone(), b.clone())),
        serde_types::WitType::List(r) => WitType::List(r.clone()),
        serde_types::WitType::Record(fields) => WitType::Record(fields.clone()),
        serde_types::WitType::Variant(cases) => WitType::Variant(cases.clone()),
        serde_types::WitType::Tuple(elems) => WitType::Tuple(elems.clone()),
    }
}

fn func_source_to_serde(s: &FuncSource) -> serde_types::FuncSource {
    match s {
        FuncSource::Internal(id) => serde_types::FuncSource::Internal(id.clone()),
        FuncSource::Imported(id) => serde_types::FuncSource::Imported(id.clone()),
        FuncSource::Exported(id) => serde_types::FuncSource::Exported(id.clone()),
    }
}

fn func_source_from_serde(s: &serde_types::FuncSource) -> FuncSource {
    match s {
        serde_types::FuncSource::Internal(id) => FuncSource::Internal(id.clone()),
        serde_types::FuncSource::Imported(id) => FuncSource::Imported(id.clone()),
        serde_types::FuncSource::Exported(id) => FuncSource::Exported(id.clone()),
    }
}

fn type_source_to_serde(s: &TypeSource) -> serde_types::TypeSource {
    match s {
        TypeSource::Internal(id) => serde_types::TypeSource::Internal(id.clone()),
        TypeSource::Imported(id) => serde_types::TypeSource::Imported(id.clone()),
        TypeSource::Exported(id) => serde_types::TypeSource::Exported(id.clone()),
    }
}

fn type_source_from_serde(s: &serde_types::TypeSource) -> TypeSource {
    match s {
        serde_types::TypeSource::Internal(id) => TypeSource::Internal(id.clone()),
        serde_types::TypeSource::Imported(id) => TypeSource::Imported(id.clone()),
        serde_types::TypeSource::Exported(id) => TypeSource::Exported(id.clone()),
    }
}

fn component_to_serde(c: &WastComponent) -> serde_types::WastComponent {
    serde_types::WastComponent {
        funcs: c
            .funcs
            .iter()
            .map(|(uid, f)| {
                (
                    uid.clone(),
                    serde_types::WastFunc {
                        source: func_source_to_serde(&f.source),
                        params: f.params.clone(),
                        result: f.result.clone(),
                        body: f.body.clone(),
                    },
                )
            })
            .collect(),
        types: c
            .types
            .iter()
            .map(|(uid, td)| {
                (
                    uid.clone(),
                    serde_types::WastTypeDef {
                        source: type_source_to_serde(&td.source),
                        definition: wit_type_to_serde(&td.definition),
                    },
                )
            })
            .collect(),
        syms: serde_types::Syms {
            wit_syms: c.syms.wit_syms.clone(),
            internal: c
                .syms
                .internal
                .iter()
                .map(|e| serde_types::SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
            local: c
                .syms
                .local
                .iter()
                .map(|e| serde_types::SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
        },
    }
}

fn component_from_serde(c: &serde_types::WastComponent) -> WastComponent {
    WastComponent {
        funcs: c
            .funcs
            .iter()
            .map(|(uid, f)| {
                (
                    uid.clone(),
                    WastFunc {
                        source: func_source_from_serde(&f.source),
                        params: f.params.clone(),
                        result: f.result.clone(),
                        body: f.body.clone(),
                    },
                )
            })
            .collect(),
        types: c
            .types
            .iter()
            .map(|(uid, td)| {
                (
                    uid.clone(),
                    WastTypeDef {
                        source: type_source_from_serde(&td.source),
                        definition: wit_type_from_serde(&td.definition),
                    },
                )
            })
            .collect(),
        syms: Syms {
            wit_syms: c.syms.wit_syms.clone(),
            internal: c
                .syms
                .internal
                .iter()
                .map(|e| SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
            local: c
                .syms
                .local
                .iter()
                .map(|e| SymEntry {
                    uid: e.uid.clone(),
                    display_name: e.display_name.clone(),
                })
                .collect(),
        },
    }
}

// ---------------------------------------------------------------------------
// File I/O helpers
// ---------------------------------------------------------------------------

fn db_path(path: &str) -> String {
    format!("{}/wast.db", path)
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

fn read_syms(path: &str, lang: &str) -> serde_types::Syms {
    let p = syms_path(path, lang);
    match std::fs::read_to_string(&p) {
        Ok(data) => syms_yaml::parse_syms_yaml(&data).unwrap_or_else(|_| serde_types::Syms {
            wit_syms: Vec::new(),
            internal: Vec::new(),
            local: Vec::new(),
        }),
        Err(_) => serde_types::Syms {
            wit_syms: Vec::new(),
            internal: Vec::new(),
            local: Vec::new(),
        },
    }
}

fn write_syms(path: &str, lang: &str, syms: &serde_types::Syms) -> Result<(), WastError> {
    let p = syms_path(path, lang);
    let yaml = syms_yaml::write_syms_yaml(syms);
    std::fs::write(&p, yaml.as_bytes())
        .map_err(|e| err_at(format!("failed to write {}: {}", p, e), p))?;
    Ok(())
}

fn syms_to_serde(syms: &Syms) -> serde_types::Syms {
    serde_types::Syms {
        wit_syms: syms.wit_syms.clone(),
        internal: syms
            .internal
            .iter()
            .map(|e| serde_types::SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
        local: syms
            .local
            .iter()
            .map(|e| serde_types::SymEntry {
                uid: e.uid.clone(),
                display_name: e.display_name.clone(),
            })
            .collect(),
    }
}

fn syms_from_serde(s: &serde_types::Syms) -> Syms {
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

/// Basic world.wit validation: ensure the file exists.
/// More detailed WIT parsing is a TODO.
fn validate_wit_exists(path: &str) -> Result<(), WastError> {
    let wp = wit_path(path);
    if !std::path::Path::new(&wp).exists() {
        return Err(WastError {
            message: "wit_not_found".to_string(),
            location: Some(wp),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Component file I/O (wast.db + syms)
// ---------------------------------------------------------------------------

fn read_component_from_disk(path: &str) -> Result<WastComponent, WastError> {
    let db = db_path(path);

    // Check if wast.db exists; if not, report db_not_found
    if !std::path::Path::new(&db).exists() {
        return Err(WastError {
            message: "db_not_found".to_string(),
            location: Some(db),
        });
    }

    let data = std::fs::read_to_string(&db)
        .map_err(|e| err_at(format!("failed to read {}: {}", db, e), db.clone()))?;
    let sc: serde_types::WastComponent = serde_json::from_str(&data)
        .map_err(|e| err_at(format!("invalid JSON in {}: {}", db, e), db))?;
    let mut component = component_from_serde(&sc);

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
    let sc = component_to_serde(component);
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

        // Check wast.db does NOT exist
        let db = db_path(&path);
        if std::path::Path::new(&db).exists() {
            return Err(err("db_exists: wast.db already exists"));
        }

        // Create empty WastComponent and write it
        let empty = WastComponent {
            funcs: vec![],
            types: vec![],
            syms: Syms {
                wit_syms: vec![],
                internal: vec![],
                local: vec![],
            },
        };

        // TODO: parse world.wit and populate exported/imported funcs and types
        write_component_to_disk(&path, &empty)?;
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
        // Validate that world.wit exists
        validate_wit_exists(&path)?;

        write_component_to_disk(&path, &component)?;
        Ok(())
    }

    fn merge(path: String, partial: WastComponent) -> Result<(), WastError> {
        // Validate that world.wit exists
        validate_wit_exists(&path)?;

        let full = read_component_from_disk(&path)?;
        let merged = merge_components(full, partial);
        write_component_to_disk(&path, &merged)?;
        Ok(())
    }
}

bindings::export!(Component with_types_in bindings);
