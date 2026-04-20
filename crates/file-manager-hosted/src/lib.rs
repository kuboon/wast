wit_bindgen::generate!({
    path: "../../wit-hosted",
    world: "file-manager-hosted-world",
});

mod serde_types;
mod syms_yaml;
mod wit_parser;

use crate::wast::file_manager_hosted::types::{
    ComponentFiles, FuncSource as BindingFuncSource, PrimitiveType as BindingPrimitiveType,
    SymEntry as BindingSymEntry, Syms as BindingSyms, TypeSource as BindingTypeSource,
    WastComponent, WastError, WastFunc as BindingWastFunc, WastTypeDef as BindingWastTypeDef,
    WitType as BindingWitType,
};
use serde_types::{
    FuncSource, PrimitiveType, Syms, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef,
    WastTypeRow, WitType,
};
use wit_parser::ParsedWorld;

struct Component;

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

fn parse_primitive(name: &str) -> Option<PrimitiveType> {
    match name {
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

fn primitive_to_binding(value: &PrimitiveType) -> BindingPrimitiveType {
    match value {
        PrimitiveType::U32 => BindingPrimitiveType::U32,
        PrimitiveType::U64 => BindingPrimitiveType::U64,
        PrimitiveType::I32 => BindingPrimitiveType::I32,
        PrimitiveType::I64 => BindingPrimitiveType::I64,
        PrimitiveType::F32 => BindingPrimitiveType::F32,
        PrimitiveType::F64 => BindingPrimitiveType::F64,
        PrimitiveType::Bool => BindingPrimitiveType::Bool,
        PrimitiveType::Char => BindingPrimitiveType::Char,
        PrimitiveType::String => BindingPrimitiveType::String,
    }
}

fn primitive_from_binding(value: &BindingPrimitiveType) -> PrimitiveType {
    match value {
        BindingPrimitiveType::U32 => PrimitiveType::U32,
        BindingPrimitiveType::U64 => PrimitiveType::U64,
        BindingPrimitiveType::I32 => PrimitiveType::I32,
        BindingPrimitiveType::I64 => PrimitiveType::I64,
        BindingPrimitiveType::F32 => PrimitiveType::F32,
        BindingPrimitiveType::F64 => PrimitiveType::F64,
        BindingPrimitiveType::Bool => PrimitiveType::Bool,
        BindingPrimitiveType::Char => PrimitiveType::Char,
        BindingPrimitiveType::String => PrimitiveType::String,
    }
}

fn wit_type_to_binding(value: &WitType) -> BindingWitType {
    match value {
        WitType::Primitive(p) => BindingWitType::Primitive(primitive_to_binding(p)),
        WitType::Option(type_ref) => BindingWitType::Option(type_ref.clone()),
        WitType::Result(ok, err) => BindingWitType::Result((ok.clone(), err.clone())),
        WitType::List(type_ref) => BindingWitType::List(type_ref.clone()),
        WitType::Record(fields) => BindingWitType::Record(fields.clone()),
        WitType::Variant(cases) => BindingWitType::Variant(cases.clone()),
        WitType::Tuple(items) => BindingWitType::Tuple(items.clone()),
    }
}

fn wit_type_from_binding(value: &BindingWitType) -> WitType {
    match value {
        BindingWitType::Primitive(p) => WitType::Primitive(primitive_from_binding(p)),
        BindingWitType::Option(type_ref) => WitType::Option(type_ref.clone()),
        BindingWitType::Result((ok, err)) => WitType::Result(ok.clone(), err.clone()),
        BindingWitType::List(type_ref) => WitType::List(type_ref.clone()),
        BindingWitType::Record(fields) => WitType::Record(fields.clone()),
        BindingWitType::Variant(cases) => WitType::Variant(cases.clone()),
        BindingWitType::Tuple(items) => WitType::Tuple(items.clone()),
    }
}

fn func_source_to_binding(value: &FuncSource) -> BindingFuncSource {
    match value {
        FuncSource::Internal(uid) => BindingFuncSource::Internal(uid.clone()),
        FuncSource::Imported(uid) => BindingFuncSource::Imported(uid.clone()),
        FuncSource::Exported(uid) => BindingFuncSource::Exported(uid.clone()),
    }
}

fn func_source_from_binding(value: &BindingFuncSource) -> FuncSource {
    match value {
        BindingFuncSource::Internal(uid) => FuncSource::Internal(uid.clone()),
        BindingFuncSource::Imported(uid) => FuncSource::Imported(uid.clone()),
        BindingFuncSource::Exported(uid) => FuncSource::Exported(uid.clone()),
    }
}

fn type_source_to_binding(value: &TypeSource) -> BindingTypeSource {
    match value {
        TypeSource::Internal(uid) => BindingTypeSource::Internal(uid.clone()),
        TypeSource::Imported(uid) => BindingTypeSource::Imported(uid.clone()),
        TypeSource::Exported(uid) => BindingTypeSource::Exported(uid.clone()),
    }
}

fn type_source_from_binding(value: &BindingTypeSource) -> TypeSource {
    match value {
        BindingTypeSource::Internal(uid) => TypeSource::Internal(uid.clone()),
        BindingTypeSource::Imported(uid) => TypeSource::Imported(uid.clone()),
        BindingTypeSource::Exported(uid) => TypeSource::Exported(uid.clone()),
    }
}

fn db_to_binding(db: &WastDb, syms: &Syms) -> WastComponent {
    WastComponent {
        funcs: db
            .funcs
            .iter()
            .map(|row| {
                (
                    row.uid.clone(),
                    BindingWastFunc {
                        source: func_source_to_binding(&row.func.source),
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
                    BindingWastTypeDef {
                        source: type_source_to_binding(&row.def.source),
                        definition: wit_type_to_binding(&row.def.definition),
                    },
                )
            })
            .collect(),
        syms: BindingSyms {
            wit_syms: syms.wit_syms.clone(),
            internal: syms
                .internal
                .iter()
                .map(|entry| BindingSymEntry {
                    uid: entry.uid.clone(),
                    display_name: entry.display_name.clone(),
                })
                .collect(),
            local: syms
                .local
                .iter()
                .map(|entry| BindingSymEntry {
                    uid: entry.uid.clone(),
                    display_name: entry.display_name.clone(),
                })
                .collect(),
        },
    }
}

fn binding_to_db(component: &WastComponent) -> (WastDb, Syms) {
    let db = WastDb {
        funcs: component
            .funcs
            .iter()
            .map(|(uid, func)| WastFuncRow {
                uid: uid.clone(),
                func: WastFunc {
                    source: func_source_from_binding(&func.source),
                    params: func.params.clone(),
                    result: func.result.clone(),
                    body: func.body.clone(),
                },
            })
            .collect(),
        types: component
            .types
            .iter()
            .map(|(uid, type_def)| WastTypeRow {
                uid: uid.clone(),
                def: WastTypeDef {
                    source: type_source_from_binding(&type_def.source),
                    definition: wit_type_from_binding(&type_def.definition),
                },
            })
            .collect(),
    };

    let syms = Syms {
        wit_syms: component.syms.wit_syms.clone(),
        internal: component
            .syms
            .internal
            .iter()
            .map(|entry| serde_types::SymEntry {
                uid: entry.uid.clone(),
                display_name: entry.display_name.clone(),
            })
            .collect(),
        local: component
            .syms
            .local
            .iter()
            .map(|entry| serde_types::SymEntry {
                uid: entry.uid.clone(),
                display_name: entry.display_name.clone(),
            })
            .collect(),
    };

    (db, syms)
}

fn parse_utf8(bytes: &[u8], label: &str) -> Result<String, WastError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|e| err_at(format!("invalid UTF-8 in {}: {}", label, e), label))
}

fn read_db_and_syms(
    db_bytes: &[u8],
    syms_bytes: Option<&[u8]>,
) -> Result<(WastDb, Syms), WastError> {
    let db_text = parse_utf8(db_bytes, "wast.json")?;
    let db: WastDb = serde_json::from_str(&db_text)
        .map_err(|e| err_at(format!("invalid JSON in wast.json: {}", e), "wast.json"))?;

    let syms = match syms_bytes {
        Some(bytes) => {
            let text = parse_utf8(bytes, "syms.en.yaml")?;
            syms_yaml::parse_syms_yaml(&text).map_err(|e| {
                err_at(
                    format!("invalid YAML in syms.en.yaml: {}", e),
                    "syms.en.yaml",
                )
            })?
        }
        None => Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        },
    };

    Ok((db, syms))
}

fn write_db_and_syms(db: &WastDb, syms: &Syms) -> Result<ComponentFiles, WastError> {
    let db_json = serde_json::to_string_pretty(db)
        .map_err(|e| err(format!("JSON serialization error: {}", e)))?;
    let syms_yaml = syms_yaml::write_syms_yaml(syms);

    Ok(ComponentFiles {
        wast_json: db_json.into_bytes(),
        syms_en_yaml: Some(syms_yaml.into_bytes()),
    })
}

fn parse_world_bytes(world_wit: &[u8]) -> Result<ParsedWorld, WastError> {
    let wit_src = parse_utf8(world_wit, "world.wit")?;
    wit_parser::parse_world(&wit_src)
        .map_err(|e| err_at(format!("wit parse error: {}", e), "world.wit"))
}

fn merge_sym_entries(
    mut base: Vec<serde_types::SymEntry>,
    overlay: Vec<serde_types::SymEntry>,
) -> Vec<serde_types::SymEntry> {
    for entry in overlay {
        if let Some(existing) = base.iter_mut().find(|e| e.uid == entry.uid) {
            existing.display_name = entry.display_name;
        } else {
            base.push(entry);
        }
    }
    base
}

fn merge_db_and_syms(
    full_db: WastDb,
    full_syms: Syms,
    partial_db: WastDb,
    partial_syms: Syms,
) -> (WastDb, Syms) {
    let partial_func_uids: std::collections::BTreeSet<String> =
        partial_db.funcs.iter().map(|row| row.uid.clone()).collect();
    let partial_type_uids: std::collections::BTreeSet<String> =
        partial_db.types.iter().map(|row| row.uid.clone()).collect();

    let mut funcs: Vec<WastFuncRow> = full_db
        .funcs
        .into_iter()
        .filter(|row| !partial_func_uids.contains(&row.uid))
        .collect();
    funcs.extend(partial_db.funcs);

    let mut types: Vec<WastTypeRow> = full_db
        .types
        .into_iter()
        .filter(|row| !partial_type_uids.contains(&row.uid))
        .collect();
    types.extend(partial_db.types);

    let mut wit_syms = full_syms.wit_syms;
    for entry in partial_syms.wit_syms {
        if let Some(existing) = wit_syms.iter_mut().find(|(k, _)| k == &entry.0) {
            existing.1 = entry.1;
        } else {
            wit_syms.push(entry);
        }
    }

    let internal = merge_sym_entries(full_syms.internal, partial_syms.internal);
    let local = merge_sym_entries(full_syms.local, partial_syms.local);

    (
        WastDb { funcs, types },
        Syms {
            wit_syms,
            internal,
            local,
        },
    )
}

fn validate_against_parsed_world(parsed: &ParsedWorld, db: &WastDb) -> Result<(), WastError> {
    let wit_exports: std::collections::BTreeMap<&str, usize> = parsed
        .exports
        .iter()
        .map(|func| (func.wit_path.as_str(), func.params.len()))
        .collect();
    let wit_imports: std::collections::BTreeMap<&str, usize> = parsed
        .imports
        .iter()
        .map(|func| (func.wit_path.as_str(), func.params.len()))
        .collect();

    for row in &db.funcs {
        let uid = &row.uid;
        let func = &row.func;
        match &func.source {
            FuncSource::Exported(wit_id) => match wit_exports.get(wit_id.as_str()) {
                None => {
                    return Err(err(format!(
                        "wit_inconsistency: exported func {} not found in world.wit",
                        uid
                    )));
                }
                Some(expected_params) if func.params.len() != *expected_params => {
                    return Err(err(format!(
                        "wit_inconsistency: func {} param count mismatch",
                        uid
                    )));
                }
                Some(_) => {}
            },
            FuncSource::Imported(wit_id) => match wit_imports.get(wit_id.as_str()) {
                None => {
                    return Err(err(format!(
                        "wit_inconsistency: imported func {} not found in world.wit",
                        uid
                    )));
                }
                Some(expected_params) if func.params.len() != *expected_params => {
                    return Err(err(format!(
                        "wit_inconsistency: func {} param count mismatch",
                        uid
                    )));
                }
                Some(_) => {}
            },
            FuncSource::Internal(_) => {}
        }
    }

    Ok(())
}

fn validate_against_wit(world_wit: &[u8], db: &WastDb) -> Result<(), WastError> {
    let parsed = parse_world_bytes(world_wit)?;
    validate_against_parsed_world(&parsed, db)
}

impl exports::wast::file_manager_hosted::file_manager_bindgen::Guest for Component {
    fn bindgen(world_wit: Vec<u8>) -> Result<ComponentFiles, WastError> {
        let parsed = parse_world_bytes(&world_wit)?;

        let mut funcs: Vec<WastFuncRow> = Vec::new();
        let mut types: Vec<WastTypeRow> = Vec::new();
        let mut wit_syms: Vec<(String, String)> = Vec::new();
        let mut seen_types = std::collections::BTreeSet::<String>::new();

        let mut ensure_type = |type_name: &str| {
            if seen_types.contains(type_name) {
                return;
            }
            if let Some(p) = parse_primitive(type_name) {
                seen_types.insert(type_name.to_string());
                types.push(WastTypeRow {
                    uid: type_name.to_string(),
                    def: WastTypeDef {
                        source: TypeSource::Imported(type_name.to_string()),
                        definition: WitType::Primitive(p),
                    },
                });
            }
        };

        for f in &parsed.imports {
            for (_, tname) in &f.params {
                ensure_type(tname);
            }
            if let Some(ret) = &f.result {
                ensure_type(ret);
            }
            funcs.push(WastFuncRow {
                uid: f.wit_path.clone(),
                func: WastFunc {
                    source: FuncSource::Imported(f.wit_path.clone()),
                    params: f.params.clone(),
                    result: f.result.clone(),
                    body: None,
                },
            });
            wit_syms.push((f.wit_path.clone(), f.name.clone()));
        }

        for f in &parsed.exports {
            for (_, tname) in &f.params {
                ensure_type(tname);
            }
            if let Some(ret) = &f.result {
                ensure_type(ret);
            }
            funcs.push(WastFuncRow {
                uid: f.wit_path.clone(),
                func: WastFunc {
                    source: FuncSource::Exported(f.wit_path.clone()),
                    params: f.params.clone(),
                    result: f.result.clone(),
                    body: None,
                },
            });
            wit_syms.push((f.wit_path.clone(), f.name.clone()));
        }

        let db = WastDb { funcs, types };
        let syms = Syms {
            wit_syms,
            internal: vec![],
            local: vec![],
        };

        write_db_and_syms(&db, &syms)
    }

    fn read(wast_json: Vec<u8>, syms_en_yaml: Option<Vec<u8>>) -> Result<WastComponent, WastError> {
        let (db, syms) = read_db_and_syms(&wast_json, syms_en_yaml.as_deref())?;
        Ok(db_to_binding(&db, &syms))
    }

    fn write(world_wit: Vec<u8>, component: WastComponent) -> Result<ComponentFiles, WastError> {
        let (db, syms) = binding_to_db(&component);
        validate_against_wit(&world_wit, &db)?;
        write_db_and_syms(&db, &syms)
    }

    fn merge(
        world_wit: Vec<u8>,
        full: ComponentFiles,
        partial: WastComponent,
    ) -> Result<ComponentFiles, WastError> {
        let (full_db, full_syms) = read_db_and_syms(&full.wast_json, full.syms_en_yaml.as_deref())?;
        let (partial_db, partial_syms) = binding_to_db(&partial);
        validate_against_wit(&world_wit, &partial_db)?;
        let (merged_db, merged_syms) =
            merge_db_and_syms(full_db, full_syms, partial_db, partial_syms);
        write_db_and_syms(&merged_db, &merged_syms)
    }
}

export!(Component);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exports::wast::file_manager_hosted::file_manager_bindgen::Guest;

    fn sample_parsed_world() -> ParsedWorld {
        wit_parser::parse_world(
            r#"
package test:pkg@0.1.0;

world bot {
  import log: func(msg: string);
  export handle-event: func(event-id: u32) -> bool;
}
"#,
        )
        .expect("parsed world")
    }

    fn sample_world_bytes() -> Vec<u8> {
        br#"
package test:pkg@0.1.0;

world bot {
  import log: func(msg: string);
  export handle-event: func(event-id: u32) -> bool;
}
"#
        .to_vec()
    }

    #[test]
    fn validate_against_parsed_world_rejects_missing_export() {
        let db = WastDb {
            funcs: vec![WastFuncRow {
                uid: "wrong".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("wrong".to_string()),
                    params: vec![("event-id".to_string(), "u32".to_string())],
                    result: Some("bool".to_string()),
                    body: None,
                },
            }],
            types: vec![],
        };

        let result = validate_against_parsed_world(&sample_parsed_world(), &db);
        let error = result.expect_err("expected validation error");
        assert!(error.message.contains("wit_inconsistency"));
        assert!(error.message.contains("not found in world.wit"));
    }

    #[test]
    fn validate_against_parsed_world_rejects_param_count_mismatch() {
        let db = WastDb {
            funcs: vec![WastFuncRow {
                uid: "handle-event".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("handle-event".to_string()),
                    params: vec![],
                    result: Some("bool".to_string()),
                    body: None,
                },
            }],
            types: vec![],
        };

        let result = validate_against_parsed_world(&sample_parsed_world(), &db);
        let error = result.expect_err("expected validation error");
        assert!(error.message.contains("wit_inconsistency"));
        assert!(error.message.contains("param count mismatch"));
    }

    #[test]
    fn bindgen_and_read_roundtrip() {
        let files = <Component as Guest>::bindgen(sample_world_bytes()).expect("bindgen");
        let component =
            <Component as Guest>::read(files.wast_json, files.syms_en_yaml).expect("read");

        assert_eq!(component.funcs.len(), 2);
        assert_eq!(component.types.len(), 3);
        assert_eq!(component.syms.wit_syms.len(), 2);
    }

    #[test]
    fn merge_returns_updated_serialized_files() {
        let full = <Component as Guest>::bindgen(sample_world_bytes()).expect("bindgen");
        let mut partial =
            <Component as Guest>::read(full.wast_json.clone(), full.syms_en_yaml.clone())
                .expect("read");

        partial.funcs.push((
            "internal/helper".to_string(),
            BindingWastFunc {
                source: BindingFuncSource::Internal("internal/helper".to_string()),
                params: vec![("event-id".to_string(), "u32".to_string())],
                result: None,
                body: Some(vec![1, 2, 3]),
            },
        ));
        partial.syms.internal.push(BindingSymEntry {
            uid: "internal/helper".to_string(),
            display_name: "helper".to_string(),
        });

        let merged =
            <Component as Guest>::merge(sample_world_bytes(), full, partial).expect("merge");
        let reloaded =
            <Component as Guest>::read(merged.wast_json, merged.syms_en_yaml).expect("reload");

        assert!(
            reloaded
                .funcs
                .iter()
                .any(|(uid, _)| uid == "internal/helper")
        );
        assert!(
            reloaded
                .syms
                .internal
                .iter()
                .any(|entry| entry.uid == "internal/helper" && entry.display_name == "helper")
        );
    }
}
