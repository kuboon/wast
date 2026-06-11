//! `world.wit` parsing for the codec, built on the real `wit-parser` crate
//! (same version as `crates/compiler`). Replaces a former hand-rolled
//! line-oriented parser that broke on nested blocks, multi-line signatures,
//! generics, and block comments — and accepted garbage.
//!
//! The public surface intentionally mirrors the old module: `parse_world`
//! returns a [`ParsedWorld`] with flattened import/export function lists.
//! In addition it now carries [`ParsedWorld::types`]: `wast.json`-shaped
//! type rows for every type referenced by those functions (primitives,
//! user-declared records/variants/…, and anonymous compounds such as
//! `option<u32>`), so `compile_wit` can materialize a db with no dangling
//! type refs.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use wast_types::{PrimitiveType, TypeSource, WastTypeDef, WitType};
use wit_parser::{Handle, Resolve, Results, Type, TypeDefKind, TypeId, WorldItem};

#[derive(Debug, Clone, PartialEq)]
pub struct WitFunc {
    /// Stable identifier within the world: `name` for world-level funcs,
    /// `iface-name/name` for interface members.
    pub wit_path: String,
    pub name: String,
    /// `(param-name, type-ref)` pairs. Type refs are either primitive names
    /// (`u32`, `string`, …) or uids of entries in [`ParsedWorld::types`].
    pub params: Vec<(String, String)>,
    pub result: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedWorld {
    pub world_name: String,
    pub imports: Vec<WitFunc>,
    pub exports: Vec<WitFunc>,
    /// Type rows (uid + definition) for every type referenced by the funcs
    /// above, including transitively referenced component types.
    pub types: Vec<(String, WastTypeDef)>,
}

pub fn parse_world(src: &str) -> Result<ParsedWorld, String> {
    let mut resolve = Resolve::default();
    let pkg = resolve
        .push_str("world.wit", src)
        .map_err(|e| format!("{e:#}"))?;
    let world_id = resolve
        .select_world(pkg, None)
        .map_err(|e| format!("{e:#}"))?;
    let world = &resolve.worlds[world_id];

    let mut registry = TypeRegistry::new(&resolve);
    let mut imports = Vec::new();
    let mut exports = Vec::new();

    for (_, item) in &world.imports {
        collect_world_item(&resolve, &mut registry, item, &mut imports)?;
    }
    for (_, item) in &world.exports {
        collect_world_item(&resolve, &mut registry, item, &mut exports)?;
    }

    Ok(ParsedWorld {
        world_name: world.name.clone(),
        imports,
        exports,
        types: registry.rows,
    })
}

fn collect_world_item(
    resolve: &Resolve,
    registry: &mut TypeRegistry,
    item: &WorldItem,
    out: &mut Vec<WitFunc>,
) -> Result<(), String> {
    match item {
        WorldItem::Function(f) => {
            out.push(convert_func(registry, f, None)?);
        }
        WorldItem::Interface { id, .. } => {
            let iface = &resolve.interfaces[*id];
            let iface_name = iface
                .name
                .clone()
                .unwrap_or_else(|| "anonymous-interface".to_string());
            for (_, f) in &iface.functions {
                out.push(convert_func(registry, f, Some(&iface_name))?);
            }
        }
        // World-level type declarations (`record point { … }` inside the
        // world, or `use`d types). Register them so user-declared types get
        // rows even before any func references them.
        WorldItem::Type(id) => {
            registry.register(*id)?;
        }
    }
    Ok(())
}

fn convert_func(
    registry: &mut TypeRegistry,
    f: &wit_parser::Function,
    iface: Option<&str>,
) -> Result<WitFunc, String> {
    let mut params = Vec::new();
    for (pname, pty) in f.params.iter() {
        params.push((pname.clone(), registry.type_ref(pty)?));
    }
    let result = match &f.results {
        Results::Anon(ty) => Some(registry.type_ref(ty)?),
        Results::Named(named) if named.is_empty() => None,
        Results::Named(named) if named.len() == 1 => Some(registry.type_ref(&named[0].1)?),
        Results::Named(_) => {
            return Err(format!(
                "func {:?}: multi-value named results are not supported",
                f.name
            ));
        }
    };
    let wit_path = match iface {
        Some(iface) => format!("{}/{}", iface, f.name),
        None => f.name.clone(),
    };
    Ok(WitFunc {
        wit_path,
        name: f.name.clone(),
        params,
        result,
    })
}

/// Collects `wast.json` type rows for every WIT type encountered while
/// flattening function signatures.
///
/// Naming scheme for uids:
/// - primitives keep their WIT name (`u32`, `string`, …; `s32`/`s64` for the
///   signed types).
/// - named user declarations keep their declared name (`point`).
/// - anonymous compounds get a canonical structural name (`option<u32>`,
///   `list<point>`, `tuple<u32,string>`, `result<u32,string>`).
struct TypeRegistry<'a> {
    resolve: &'a Resolve,
    rows: Vec<(String, WastTypeDef)>,
    /// uid → index into `rows`, used for dedup and collision detection.
    by_uid: BTreeMap<String, usize>,
    /// Already-registered wit-parser type ids.
    by_id: BTreeMap<TypeId, String>,
    seen_primitives: BTreeSet<String>,
}

impl<'a> TypeRegistry<'a> {
    fn new(resolve: &'a Resolve) -> Self {
        Self {
            resolve,
            rows: Vec::new(),
            by_uid: BTreeMap::new(),
            by_id: BTreeMap::new(),
            seen_primitives: BTreeSet::new(),
        }
    }

    /// Return the type-ref string for `ty`, registering rows as needed.
    fn type_ref(&mut self, ty: &Type) -> Result<String, String> {
        match ty {
            Type::Bool => self.primitive("bool", PrimitiveType::Bool),
            Type::U32 => self.primitive("u32", PrimitiveType::U32),
            Type::U64 => self.primitive("u64", PrimitiveType::U64),
            Type::S32 => self.primitive("s32", PrimitiveType::I32),
            Type::S64 => self.primitive("s64", PrimitiveType::I64),
            Type::F32 => self.primitive("f32", PrimitiveType::F32),
            Type::F64 => self.primitive("f64", PrimitiveType::F64),
            Type::Char => self.primitive("char", PrimitiveType::Char),
            Type::String => self.primitive("string", PrimitiveType::String),
            Type::U8 | Type::U16 | Type::S8 | Type::S16 => Err(format!(
                "unsupported primitive type {ty:?}: wast supports 32/64-bit numerics only"
            )),
            Type::Id(id) => self.register(*id),
        }
    }

    fn primitive(&mut self, name: &str, p: PrimitiveType) -> Result<String, String> {
        if self.seen_primitives.insert(name.to_string()) {
            self.push_row(
                name.to_string(),
                WastTypeDef {
                    source: TypeSource::Imported(name.to_string()),
                    definition: WitType::Primitive(p),
                },
            );
        }
        Ok(name.to_string())
    }

    fn push_row(&mut self, uid: String, def: WastTypeDef) {
        self.by_uid.insert(uid.clone(), self.rows.len());
        self.rows.push((uid, def));
    }

    fn register(&mut self, id: TypeId) -> Result<String, String> {
        if let Some(uid) = self.by_id.get(&id) {
            return Ok(uid.clone());
        }
        let td = &self.resolve.types[id];

        // Type aliases (`type a = b`, `use iface.{t}`) resolve through to
        // their target so refs land on the canonical row.
        if let TypeDefKind::Type(inner) = &td.kind {
            let uid = self.type_ref(inner)?;
            self.by_id.insert(id, uid.clone());
            return Ok(uid);
        }

        let (uid, definition) = match &td.kind {
            TypeDefKind::Record(record) => {
                let mut fields = Vec::new();
                for field in &record.fields {
                    fields.push((field.name.clone(), self.type_ref(&field.ty)?));
                }
                (self.named_uid(td), WitType::Record(fields))
            }
            TypeDefKind::Variant(variant) => {
                let mut cases = Vec::new();
                for case in &variant.cases {
                    let payload = match &case.ty {
                        Some(ty) => Some(self.type_ref(ty)?),
                        None => None,
                    };
                    cases.push((case.name.clone(), payload));
                }
                (self.named_uid(td), WitType::Variant(cases))
            }
            TypeDefKind::Enum(e) => {
                let cases: Vec<String> = e.cases.iter().map(|c| c.name.clone()).collect();
                (self.named_uid(td), WitType::Enum(cases))
            }
            TypeDefKind::Flags(f) => {
                let names: Vec<String> = f.flags.iter().map(|f| f.name.clone()).collect();
                (self.named_uid(td), WitType::Flags(names))
            }
            TypeDefKind::Resource => (self.named_uid(td), WitType::Resource),
            TypeDefKind::Option(inner) => {
                let inner_ref = self.type_ref(inner)?;
                let uid = td
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("option<{inner_ref}>"));
                (uid, WitType::Option(inner_ref))
            }
            TypeDefKind::Result(r) => {
                let (ok, err) = match (&r.ok, &r.err) {
                    (Some(ok), Some(err)) => (self.type_ref(ok)?, self.type_ref(err)?),
                    _ => {
                        return Err(
                            "unsupported type: result with an omitted ok/err side".to_string()
                        );
                    }
                };
                let uid = td
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("result<{ok},{err}>"));
                (uid, WitType::Result(ok, err))
            }
            TypeDefKind::List(inner) => {
                let inner_ref = self.type_ref(inner)?;
                let uid = td
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("list<{inner_ref}>"));
                (uid, WitType::List(inner_ref))
            }
            TypeDefKind::Tuple(tuple) => {
                let mut refs = Vec::new();
                for ty in &tuple.types {
                    refs.push(self.type_ref(ty)?);
                }
                let uid = td
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("tuple<{}>", refs.join(",")));
                (uid, WitType::Tuple(refs))
            }
            TypeDefKind::Handle(handle) => {
                let (resource_uid, definition, prefix) = match handle {
                    Handle::Own(rid) => {
                        let r = self.register(*rid)?;
                        (r.clone(), WitType::Own(r), "own")
                    }
                    Handle::Borrow(rid) => {
                        let r = self.register(*rid)?;
                        (r.clone(), WitType::Borrow(r), "borrow")
                    }
                };
                let uid = td
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{prefix}<{resource_uid}>"));
                (uid, definition)
            }
            TypeDefKind::Future(_) | TypeDefKind::Stream(_) => {
                return Err(format!(
                    "unsupported type: {} is not representable in wast",
                    td.kind.as_str()
                ));
            }
            TypeDefKind::Unknown => {
                return Err("unsupported type: unresolved foreign type".to_string());
            }
            TypeDefKind::Type(_) => unreachable!("aliases handled above"),
        };

        // Dedup / collision handling. Two distinct wit-parser ids can map to
        // the same canonical uid (e.g. `option<u32>` used twice) — reuse the
        // row when the definitions agree, disambiguate otherwise.
        let uid = self.dedup_uid(uid, &definition);
        self.by_id.insert(id, uid.clone());
        Ok(uid)
    }

    fn dedup_uid(&mut self, uid: String, definition: &WitType) -> String {
        let source = |u: &str| TypeSource::Imported(u.to_string());
        match self.by_uid.get(&uid) {
            Some(&idx)
                if format!("{:?}", self.rows[idx].1.definition) == format!("{definition:?}") =>
            {
                uid
            }
            Some(_) => {
                // Same name, different structure (e.g. same-named records in
                // two interfaces) — suffix until free.
                let mut n = 2;
                loop {
                    let candidate = format!("{uid}#{n}");
                    match self.by_uid.get(&candidate) {
                        Some(&idx)
                            if format!("{:?}", self.rows[idx].1.definition)
                                == format!("{definition:?}") =>
                        {
                            return candidate;
                        }
                        Some(_) => n += 1,
                        None => {
                            self.push_row(
                                candidate.clone(),
                                WastTypeDef {
                                    source: source(&candidate),
                                    definition: definition.clone(),
                                },
                            );
                            return candidate;
                        }
                    }
                }
            }
            None => {
                self.push_row(
                    uid.clone(),
                    WastTypeDef {
                        source: source(&uid),
                        definition: definition.clone(),
                    },
                );
                uid
            }
        }
    }

    /// uid for kinds that always carry a declared name in valid WIT.
    fn named_uid(&self, td: &wit_parser::TypeDef) -> String {
        td.name
            .clone()
            .unwrap_or_else(|| format!("anon-{}", td.kind.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_world_interface_reference_exports() {
        let src = r#"
package wast:core@0.1.0;

interface file-manager {
    bindgen: func(path: string) -> result<_, string>;
    read: func(path: string) -> result<string, string>;
}

world file-manager-world {
    export file-manager;
}
"#;
        // result<_, string> has an omitted ok side → clear error (the wast
        // type model requires both sides).
        let err = parse_world(src).expect_err("omitted result side should error");
        assert!(err.contains("result"), "{err}");
    }

    #[test]
    fn parses_interface_reference_with_full_types() {
        let src = r#"
package wast:core@0.1.0;

interface file-manager {
    read: func(path: string) -> result<string, string>;
}

world file-manager-world {
    export file-manager;
}
"#;
        let parsed = parse_world(src).expect("parse world");
        assert_eq!(parsed.world_name, "file-manager-world");
        assert_eq!(parsed.exports.len(), 1);
        assert_eq!(parsed.exports[0].wit_path, "file-manager/read");
        assert_eq!(
            parsed.exports[0].result.as_deref(),
            Some("result<string,string>")
        );
        // The anonymous result row must be materialized.
        assert!(
            parsed
                .types
                .iter()
                .any(|(uid, _)| uid == "result<string,string>")
        );
    }

    #[test]
    fn parses_nested_record_and_multiline_signature() {
        // The old line-oriented parser broke on the record's closing brace
        // and on signatures spanning multiple lines.
        let src = r#"
package test:pkg;

world w {
  record point {
    x: u32,
    y: u32,
  }
  export make-point: func(
    x: u32,
    y: u32,
  ) -> point;
  export pick: func(points: list<point>, idx: u32) -> option<point>;
}
"#;
        let parsed = parse_world(src).expect("parse world");
        assert_eq!(parsed.exports.len(), 2);
        let make_point = &parsed.exports[0];
        assert_eq!(make_point.wit_path, "make-point");
        assert_eq!(
            make_point.params,
            vec![
                ("x".to_string(), "u32".to_string()),
                ("y".to_string(), "u32".to_string())
            ]
        );
        assert_eq!(make_point.result.as_deref(), Some("point"));

        let pick = &parsed.exports[1];
        assert_eq!(pick.params[0].1, "list<point>");
        assert_eq!(pick.result.as_deref(), Some("option<point>"));

        let uids: Vec<&str> = parsed.types.iter().map(|(u, _)| u.as_str()).collect();
        assert!(uids.contains(&"point"));
        assert!(uids.contains(&"list<point>"));
        assert!(uids.contains(&"option<point>"));
        let point = parsed.types.iter().find(|(u, _)| u == "point").unwrap();
        assert!(matches!(&point.1.definition, WitType::Record(fields)
            if fields.len() == 2 && fields[0].0 == "x" && fields[1].0 == "y"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_world("this is not wit").is_err());
        assert!(parse_world("world w { export broken").is_err());
    }

    #[test]
    fn handles_block_comments() {
        let src = r#"
package test:pkg;

/* a block comment
   spanning lines */
world w {
  /* another */ export f: func() -> u32;
}
"#;
        let parsed = parse_world(src).expect("parse world");
        assert_eq!(parsed.exports.len(), 1);
        assert_eq!(parsed.exports[0].wit_path, "f");
    }
}
