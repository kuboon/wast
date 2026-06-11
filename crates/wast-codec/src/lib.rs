wit_bindgen::generate!({
    path: "../../wit-codec",
    world: "codec-world",
});

mod syms_yaml;
mod wit_parser;

use crate::wast::codec::types::{
    ComponentFiles, FuncSource as BindingFuncSource, PrimitiveType as BindingPrimitiveType,
    SymEntry as BindingSymEntry, Syms as BindingSyms, TypeSource as BindingTypeSource,
    WastComponent, WastError, WastFunc as BindingWastFunc, WastTypeDef as BindingWastTypeDef,
    WitType as BindingWitType,
};
use wast_types::{
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
        WitType::Enum(cases) => BindingWitType::Enum(cases.clone()),
        WitType::Flags(names) => BindingWitType::Flags(names.clone()),
        WitType::Resource => BindingWitType::Resource,
        WitType::Own(r) => BindingWitType::Own(r.clone()),
        WitType::Borrow(r) => BindingWitType::Borrow(r.clone()),
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
        BindingWitType::Enum(cases) => WitType::Enum(cases.clone()),
        BindingWitType::Flags(names) => WitType::Flags(names.clone()),
        BindingWitType::Resource => WitType::Resource,
        BindingWitType::Own(r) => WitType::Own(r.clone()),
        BindingWitType::Borrow(r) => WitType::Borrow(r.clone()),
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
    let db = WastDb::new(
        component
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
        component
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
    );

    let syms = Syms {
        wit_syms: component.syms.wit_syms.clone(),
        internal: component
            .syms
            .internal
            .iter()
            .map(|entry| wast_types::SymEntry {
                uid: entry.uid.clone(),
                display_name: entry.display_name.clone(),
            })
            .collect(),
        local: component
            .syms
            .local
            .iter()
            .map(|entry| wast_types::SymEntry {
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
    if db.version != WastDb::CURRENT_VERSION {
        return Err(err_at(
            format!(
                "unsupported wast.json schema version {} (this build supports version {})",
                db.version,
                WastDb::CURRENT_VERSION
            ),
            "wast.json",
        ));
    }
    let mut seen_funcs = std::collections::BTreeSet::new();
    for row in &db.funcs {
        if !seen_funcs.insert(row.uid.as_str()) {
            return Err(err_at(
                format!("duplicate func uid '{}' in wast.json", row.uid),
                "wast.json",
            ));
        }
    }
    let mut seen_types = std::collections::BTreeSet::new();
    for row in &db.types {
        if !seen_types.insert(row.uid.as_str()) {
            return Err(err_at(
                format!("duplicate type uid '{}' in wast.json", row.uid),
                "wast.json",
            ));
        }
    }

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
    mut base: Vec<wast_types::SymEntry>,
    overlay: Vec<wast_types::SymEntry>,
) -> Vec<wast_types::SymEntry> {
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
    let partial_type_uids: std::collections::BTreeSet<String> =
        partial_db.types.iter().map(|row| row.uid.clone()).collect();

    // Func-merge contract (kept aligned with partial-manager's merge
    // semantics where cheap):
    //  - A partial row with `body: None` while the full row HAS a body is a
    //    signature-only stub (this is how `extract` represents pulled-in
    //    callees, retagged as Imported). Blindly replacing the full row
    //    would silently destroy the real implementation, so the full row's
    //    body AND source are preserved.
    //  - Otherwise the partial row wins, except that an `Exported` partial
    //    source does not overwrite full's source tag: extract retags owned
    //    funcs as Exported only to lock their signature, and that marker
    //    must not leak into storage (mirrors partial-manager's
    //    source-preservation rule).
    let mut funcs: Vec<WastFuncRow> = full_db.funcs;
    for mut prow in partial_db.funcs {
        if let Some(existing) = funcs.iter_mut().find(|row| row.uid == prow.uid) {
            if prow.func.body.is_none() && existing.func.body.is_some() {
                prow.func.body = existing.func.body.clone();
                prow.func.source = existing.func.source.clone();
            } else if matches!(prow.func.source, FuncSource::Exported(_)) {
                prow.func.source = existing.func.source.clone();
            }
            *existing = prow;
        } else {
            funcs.push(prow);
        }
    }

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
        WastDb::new(funcs, types),
        Syms {
            wit_syms,
            internal,
            local,
        },
    )
}

/// Lookup table from type-ref uid to its definition.
type TypeTable<'a> = std::collections::BTreeMap<&'a str, &'a WitType>;

/// Resolve a type ref against a table, treating primitive names (incl. the
/// WIT spellings `s32`/`s64` for the signed types) as built-ins.
fn expand_ref<'a>(type_ref: &str, table: &TypeTable<'a>) -> Option<std::borrow::Cow<'a, WitType>> {
    let prim = match type_ref {
        "s32" => Some(PrimitiveType::I32),
        "s64" => Some(PrimitiveType::I64),
        other => parse_primitive(other),
    };
    if let Some(p) = prim {
        return Some(std::borrow::Cow::Owned(WitType::Primitive(p)));
    }
    table
        .get(type_ref)
        .map(|t| std::borrow::Cow::Borrowed(*t))
}

/// Recursion guard for [`type_refs_equiv`]. WIT types cannot be recursive,
/// but db tables are untrusted input and could contain ref cycles.
const MAX_TYPE_DEPTH: u32 = 64;

/// Structural equivalence of two type refs, each resolved against its own
/// table. Uid spellings are meaningless (the db may call `option<u32>`
/// "opt_u32"); only the resolved structure counts.
///
/// TODO: this covers all current `WitType` shapes structurally; subtleties
/// like resource identity across own/borrow boundaries are compared
/// structurally only (a `Resource` matches any `Resource`).
fn type_refs_equiv(
    a_ref: &str,
    a_table: &TypeTable,
    b_ref: &str,
    b_table: &TypeTable,
    depth: u32,
) -> bool {
    if depth == 0 {
        // Cycle in untrusted input (or pathological nesting) — refuse.
        return false;
    }
    let (a, b) = match (expand_ref(a_ref, a_table), expand_ref(b_ref, b_table)) {
        (Some(a), Some(b)) => (a, b),
        // Dangling ref on either side.
        _ => return false,
    };
    let recurse =
        |ra: &str, rb: &str| -> bool { type_refs_equiv(ra, a_table, rb, b_table, depth - 1) };
    match (a.as_ref(), b.as_ref()) {
        (WitType::Primitive(pa), WitType::Primitive(pb)) => {
            std::mem::discriminant(pa) == std::mem::discriminant(pb)
        }
        (WitType::Option(ia), WitType::Option(ib)) => recurse(ia, ib),
        (WitType::Result(oa, ea), WitType::Result(ob, eb)) => recurse(oa, ob) && recurse(ea, eb),
        (WitType::List(ia), WitType::List(ib)) => recurse(ia, ib),
        (WitType::Record(fa), WitType::Record(fb)) => {
            fa.len() == fb.len()
                && fa
                    .iter()
                    .zip(fb)
                    .all(|((na, ra), (nb, rb))| na == nb && recurse(ra, rb))
        }
        (WitType::Variant(ca), WitType::Variant(cb)) => {
            ca.len() == cb.len()
                && ca.iter().zip(cb).all(|((na, pa), (nb, pb))| {
                    na == nb
                        && match (pa, pb) {
                            (Some(ra), Some(rb)) => recurse(ra, rb),
                            (Option::None, Option::None) => true,
                            _ => false,
                        }
                })
        }
        (WitType::Tuple(ta), WitType::Tuple(tb)) => {
            ta.len() == tb.len() && ta.iter().zip(tb).all(|(ra, rb)| recurse(ra, rb))
        }
        (WitType::Enum(ca), WitType::Enum(cb)) => ca == cb,
        (WitType::Flags(na), WitType::Flags(nb)) => na == nb,
        (WitType::Resource, WitType::Resource) => true,
        (WitType::Own(ra), WitType::Own(rb)) | (WitType::Borrow(ra), WitType::Borrow(rb)) => {
            recurse(ra, rb)
        }
        _ => false,
    }
}

/// Validate one wast func against its world.wit counterpart: param count,
/// param names, param types (structural), and result presence + type.
fn validate_func_signature(
    uid: &str,
    func: &WastFunc,
    wit_func: &wit_parser::WitFunc,
    db_table: &TypeTable,
    wit_table: &TypeTable,
) -> Result<(), WastError> {
    if func.params.len() != wit_func.params.len() {
        return Err(err(format!(
            "wit_inconsistency: func {} param count mismatch (wast.json has {}, world.wit has {})",
            uid,
            func.params.len(),
            wit_func.params.len()
        )));
    }
    for ((db_name, db_ref), (wit_name, wit_ref)) in func.params.iter().zip(&wit_func.params) {
        if db_name != wit_name {
            return Err(err(format!(
                "wit_inconsistency: func {} param name mismatch \
                 (wast.json has '{}', world.wit has '{}')",
                uid, db_name, wit_name
            )));
        }
        if !type_refs_equiv(db_ref, db_table, wit_ref, wit_table, MAX_TYPE_DEPTH) {
            return Err(err(format!(
                "wit_inconsistency: func {} param '{}' type mismatch \
                 (wast.json ref '{}' does not match world.wit type '{}')",
                uid, db_name, db_ref, wit_ref
            )));
        }
    }
    match (&func.result, &wit_func.result) {
        (Option::None, Option::None) => {}
        (Some(db_ref), Some(wit_ref)) => {
            if !type_refs_equiv(db_ref, db_table, wit_ref, wit_table, MAX_TYPE_DEPTH) {
                return Err(err(format!(
                    "wit_inconsistency: func {} result type mismatch \
                     (wast.json ref '{}' does not match world.wit type '{}')",
                    uid, db_ref, wit_ref
                )));
            }
        }
        (Some(_), Option::None) => {
            return Err(err(format!(
                "wit_inconsistency: func {} declares a result but world.wit has none",
                uid
            )));
        }
        (Option::None, Some(_)) => {
            return Err(err(format!(
                "wit_inconsistency: func {} has no result but world.wit declares one",
                uid
            )));
        }
    }
    Ok(())
}

fn validate_against_parsed_world(parsed: &ParsedWorld, db: &WastDb) -> Result<(), WastError> {
    let wit_exports: std::collections::BTreeMap<&str, &wit_parser::WitFunc> = parsed
        .exports
        .iter()
        .map(|func| (func.wit_path.as_str(), func))
        .collect();
    let wit_imports: std::collections::BTreeMap<&str, &wit_parser::WitFunc> = parsed
        .imports
        .iter()
        .map(|func| (func.wit_path.as_str(), func))
        .collect();
    let db_table: TypeTable = db
        .types
        .iter()
        .map(|row| (row.uid.as_str(), &row.def.definition))
        .collect();
    let wit_table: TypeTable = parsed
        .types
        .iter()
        .map(|(uid, def)| (uid.as_str(), &def.definition))
        .collect();

    for row in &db.funcs {
        let uid = &row.uid;
        let func = &row.func;
        let (lookup, kind) = match &func.source {
            FuncSource::Exported(wit_id) => (wit_exports.get(wit_id.as_str()), "exported"),
            FuncSource::Imported(wit_id) => (wit_imports.get(wit_id.as_str()), "imported"),
            FuncSource::Internal(_) => continue,
        };
        match lookup {
            None => {
                return Err(err(format!(
                    "wit_inconsistency: {} func {} not found in world.wit",
                    kind, uid
                )));
            }
            Some(wit_func) => {
                validate_func_signature(uid, func, wit_func, &db_table, &wit_table)?;
            }
        }
    }

    Ok(())
}

fn validate_against_wit(world_wit: &[u8], db: &WastDb) -> Result<(), WastError> {
    let parsed = parse_world_bytes(world_wit)?;
    validate_against_parsed_world(&parsed, db)
}

impl exports::wast::codec::codec::Guest for Component {
    fn compile_wit(world_wit: Vec<u8>) -> Result<ComponentFiles, WastError> {
        let parsed = parse_world_bytes(&world_wit)?;

        let mut funcs: Vec<WastFuncRow> = Vec::new();
        let mut wit_syms: Vec<(String, String)> = Vec::new();

        // A world may both import and export a func with the same wit path
        // (`import f: func(); export f: func();`). Row uids must be unique,
        // so imported funcs that collide with an export get a `#import`
        // suffix on their uid; the `source` keeps the true wit path.
        let export_paths: std::collections::BTreeSet<&str> = parsed
            .exports
            .iter()
            .map(|f| f.wit_path.as_str())
            .collect();

        for f in &parsed.imports {
            let uid = if export_paths.contains(f.wit_path.as_str()) {
                format!("{}#import", f.wit_path)
            } else {
                f.wit_path.clone()
            };
            funcs.push(WastFuncRow {
                uid: uid.clone(),
                func: WastFunc {
                    source: FuncSource::Imported(f.wit_path.clone()),
                    params: f.params.clone(),
                    result: f.result.clone(),
                    body: None,
                },
            });
            wit_syms.push((uid, f.name.clone()));
        }

        for f in &parsed.exports {
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

        // Materialize a row for every type the funcs reference — primitives,
        // user-declared types, and anonymous compounds alike — so the
        // generated db has no dangling type refs.
        let types: Vec<WastTypeRow> = parsed
            .types
            .iter()
            .map(|(uid, def)| WastTypeRow {
                uid: uid.clone(),
                def: def.clone(),
            })
            .collect();

        let db = WastDb::new(funcs, types);
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
    use crate::exports::wast::codec::codec::Guest;

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
            version: 1,
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
            version: 1,
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
    fn compile_wit_and_read_roundtrip() {
        let files = <Component as Guest>::compile_wit(sample_world_bytes()).expect("compile_wit");
        let component =
            <Component as Guest>::read(files.wast_json, files.syms_en_yaml).expect("read");

        assert_eq!(component.funcs.len(), 2);
        assert_eq!(component.types.len(), 3);
        assert_eq!(component.syms.wit_syms.len(), 2);
    }

    #[test]
    fn merge_returns_updated_serialized_files() {
        let full = <Component as Guest>::compile_wit(sample_world_bytes()).expect("compile_wit");
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

    // ── wast.json schema version ──

    fn db_json(version_field: Option<&str>) -> Vec<u8> {
        let version = version_field
            .map(|v| format!("\"version\": {v}, "))
            .unwrap_or_default();
        format!("{{ {version}\"funcs\": [], \"types\": [] }}").into_bytes()
    }

    #[test]
    fn read_accepts_version_1() {
        assert!(<Component as Guest>::read(db_json(Some("1")), None).is_ok());
    }

    #[test]
    fn read_rejects_missing_version() {
        let error = <Component as Guest>::read(db_json(None), None).expect_err("missing version");
        assert!(error.message.contains("version"), "{}", error.message);
    }

    #[test]
    fn read_rejects_wrong_version() {
        let error = <Component as Guest>::read(db_json(Some("2")), None).expect_err("version 2");
        assert!(
            error.message.contains("unsupported wast.json schema version 2"),
            "{}",
            error.message
        );
    }

    #[test]
    fn read_rejects_duplicate_uid() {
        let json = br#"{
            "version": 1,
            "funcs": [
                { "uid": "dup", "source": { "Internal": "dup" }, "params": [], "result": null, "body": null },
                { "uid": "dup", "source": { "Internal": "dup" }, "params": [], "result": null, "body": null }
            ],
            "types": []
        }"#;
        let error = <Component as Guest>::read(json.to_vec(), None).expect_err("duplicate uid");
        assert!(
            error.message.contains("duplicate func uid 'dup'"),
            "{}",
            error.message
        );
    }

    // ── compile_wit: import/export name collision (uid disambiguation) ──

    #[test]
    fn compile_wit_disambiguates_import_export_collision() {
        let wit = br#"
package test:pkg;

world w {
  import ping: func() -> u32;
  export ping: func() -> u32;
}
"#
        .to_vec();
        let files = <Component as Guest>::compile_wit(wit).expect("compile_wit");
        let component =
            <Component as Guest>::read(files.wast_json, files.syms_en_yaml).expect("read");
        let uids: Vec<&str> = component
            .funcs
            .iter()
            .map(|(uid, _)| uid.as_str())
            .collect();
        assert_eq!(uids.len(), 2);
        assert!(uids.contains(&"ping"), "{uids:?}");
        assert!(uids.contains(&"ping#import"), "{uids:?}");
        let import = component
            .funcs
            .iter()
            .find(|(uid, _)| uid == "ping#import")
            .unwrap();
        assert!(
            matches!(&import.1.source, BindingFuncSource::Imported(path) if path == "ping"),
            "source must keep the true wit path"
        );
    }

    // ── merge: imported stub must not clobber a real body ──

    #[test]
    fn merge_preserves_body_when_partial_has_imported_stub() {
        // full: internal func WITH body. partial: the same uid as an
        // extract-produced Imported stub (body: None). merge must keep the
        // full row's body and source instead of silently dropping them.
        let real_body = vec![1u8, 0]; // version byte + empty instruction list
        let full_db = WastDb::new(
            vec![WastFuncRow {
                uid: "helper".to_string(),
                func: WastFunc {
                    source: FuncSource::Internal("helper".to_string()),
                    params: vec![],
                    result: None,
                    body: Some(real_body.clone()),
                },
            }],
            vec![],
        );
        let full_syms = Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        };
        let partial_db = WastDb::new(
            vec![WastFuncRow {
                uid: "helper".to_string(),
                func: WastFunc {
                    source: FuncSource::Imported("helper".to_string()),
                    params: vec![],
                    result: None,
                    body: None,
                },
            }],
            vec![],
        );
        let partial_syms = full_syms.clone();

        let (merged_db, _) = merge_db_and_syms(full_db, full_syms, partial_db, partial_syms);
        assert_eq!(merged_db.funcs.len(), 1);
        let row = &merged_db.funcs[0];
        assert_eq!(
            row.func.body,
            Some(real_body),
            "imported stub must not erase the real body"
        );
        assert!(
            matches!(&row.func.source, FuncSource::Internal(_)),
            "source must be preserved alongside the body"
        );
    }

    #[test]
    fn merge_keeps_full_source_for_exported_partial_rows() {
        // extract retags owned funcs as Exported to lock their signature;
        // that marker must not overwrite full's Internal source tag.
        let full_db = WastDb::new(
            vec![WastFuncRow {
                uid: "f".to_string(),
                func: WastFunc {
                    source: FuncSource::Internal("f".to_string()),
                    params: vec![],
                    result: None,
                    body: Some(vec![1, 0]),
                },
            }],
            vec![],
        );
        let empty = Syms {
            wit_syms: vec![],
            internal: vec![],
            local: vec![],
        };
        let partial_db = WastDb::new(
            vec![WastFuncRow {
                uid: "f".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("f".to_string()),
                    params: vec![],
                    result: None,
                    body: Some(vec![1, 1, 33]), // edited body (Nop)
                },
            }],
            vec![],
        );
        let (merged_db, _) = merge_db_and_syms(full_db, empty.clone(), partial_db, empty);
        let row = &merged_db.funcs[0];
        assert_eq!(row.func.body, Some(vec![1, 1, 33]), "body must propagate");
        assert!(matches!(&row.func.source, FuncSource::Internal(_)));
    }

    // ── validation: names / types / result presence ──

    #[test]
    fn validate_rejects_param_name_mismatch() {
        let db = WastDb {
            version: 1,
            funcs: vec![WastFuncRow {
                uid: "handle-event".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("handle-event".to_string()),
                    params: vec![("wrong-name".to_string(), "u32".to_string())],
                    result: Some("bool".to_string()),
                    body: None,
                },
            }],
            types: vec![],
        };
        let error = validate_against_parsed_world(&sample_parsed_world(), &db)
            .expect_err("expected validation error");
        assert!(error.message.contains("param name mismatch"), "{}", error.message);
    }

    #[test]
    fn validate_rejects_param_type_mismatch() {
        let db = WastDb {
            version: 1,
            funcs: vec![WastFuncRow {
                uid: "handle-event".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("handle-event".to_string()),
                    params: vec![("event-id".to_string(), "u64".to_string())],
                    result: Some("bool".to_string()),
                    body: None,
                },
            }],
            types: vec![],
        };
        let error = validate_against_parsed_world(&sample_parsed_world(), &db)
            .expect_err("expected validation error");
        assert!(error.message.contains("type mismatch"), "{}", error.message);
    }

    #[test]
    fn validate_rejects_result_presence_mismatch() {
        let db = WastDb {
            version: 1,
            funcs: vec![WastFuncRow {
                uid: "handle-event".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("handle-event".to_string()),
                    params: vec![("event-id".to_string(), "u32".to_string())],
                    result: None,
                    body: None,
                },
            }],
            types: vec![],
        };
        let error = validate_against_parsed_world(&sample_parsed_world(), &db)
            .expect_err("expected validation error");
        assert!(
            error.message.contains("world.wit declares one"),
            "{}",
            error.message
        );
    }

    #[test]
    fn validate_accepts_structurally_equal_refs_with_different_uids() {
        // The db calls its option type "opt_u32"; the world.wit side calls it
        // "option<u32>". Structural comparison must accept this.
        let wit = br#"
package test:pkg;

world w {
  export unwrap-or: func(o: option<u32>, default: u32) -> u32;
}
"#;
        let parsed =
            wit_parser::parse_world(std::str::from_utf8(wit).unwrap()).expect("parse world");
        let db = WastDb {
            version: 1,
            funcs: vec![WastFuncRow {
                uid: "unwrap_or".to_string(),
                func: WastFunc {
                    source: FuncSource::Exported("unwrap-or".to_string()),
                    params: vec![
                        ("o".to_string(), "opt_u32".to_string()),
                        ("default".to_string(), "u32".to_string()),
                    ],
                    result: Some("u32".to_string()),
                    body: None,
                },
            }],
            types: vec![WastTypeRow {
                uid: "opt_u32".to_string(),
                def: WastTypeDef {
                    source: TypeSource::Internal("opt_u32".to_string()),
                    definition: WitType::Option("u32".to_string()),
                },
            }],
        };
        validate_against_parsed_world(&parsed, &db).expect("structural match should validate");
    }

    // ── sample-wast fixture: compile_wit with compound types, no dangling refs ──

    fn sample_wast_path(file: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/sample-wast")
            .join(file)
    }

    #[test]
    fn compile_sample_world_has_no_dangling_type_refs() {
        let world = std::fs::read(sample_wast_path("world.wit")).expect("read world.wit");
        let files = <Component as Guest>::compile_wit(world).expect("compile_wit");
        let component =
            <Component as Guest>::read(files.wast_json, files.syms_en_yaml).expect("read");

        let type_uids: std::collections::BTreeSet<&str> = component
            .types
            .iter()
            .map(|(uid, _)| uid.as_str())
            .collect();
        let resolves = |r: &str| parse_primitive(r).is_some() || type_uids.contains(r);
        for (uid, func) in &component.funcs {
            for (pname, pref) in &func.params {
                assert!(
                    resolves(pref),
                    "dangling param type ref '{pref}' on {uid}.{pname}"
                );
            }
            if let Some(ret) = &func.result {
                assert!(resolves(ret), "dangling result type ref '{ret}' on {uid}");
            }
        }
        // The user-declared record and the anonymous option must have rows.
        assert!(type_uids.contains("point"), "{type_uids:?}");
        assert!(type_uids.contains("option<u32>"), "{type_uids:?}");
        let point = component.types.iter().find(|(u, _)| u == "point").unwrap();
        assert!(matches!(&point.1.definition, BindingWitType::Record(_)));
    }

    #[test]
    fn sample_fixture_reads_validates_and_bodies_decode() {
        let wast_json = std::fs::read(sample_wast_path("wast.json")).expect("read wast.json");
        let syms = std::fs::read(sample_wast_path("syms.en.yaml")).expect("read syms");
        let world = std::fs::read(sample_wast_path("world.wit")).expect("read world.wit");

        // read must succeed (version field + unique uids).
        let component =
            <Component as Guest>::read(wast_json, Some(syms)).expect("read sample fixture");
        assert_eq!(component.funcs.len(), 12);

        // every persisted body must decode with the current format version.
        for (uid, func) in &component.funcs {
            if let Some(body) = &func.body {
                wast_pattern_analyzer::deserialize_body(body)
                    .unwrap_or_else(|e| panic!("body of '{uid}' failed to decode: {e}"));
            }
        }

        // write (which validates against world.wit) must succeed.
        <Component as Guest>::write(world, component).expect("write/validate sample fixture");
    }
}
