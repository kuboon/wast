//! v0.30: heterogeneous `variant` at `MatchVariant`. The v0.29 narrow
//! pattern (set wider binding first, narrow in the narrower-case branch)
//! is generalized to N-case variants. A case whose payload core type
//! matches the flat-joined slot sets its binding directly; a narrower
//! case inserts an `i32.wrap_i64` before `local.set`.

use wasmtime::component::{Component, ComponentType, Lift, Linker, Lower};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, MatchArm, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

// variant id { short(u32), long(u64), anon }
//
//   kind(i: id) -> u32
//     match i {
//       short(x) -> x,      // u32 binding narrowed from joined i64
//       long(y)  -> 1,      // u64 binding declared but unused
//       anon     -> 2,
//     }

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(variant)]
enum Id {
    #[component(name = "short")]
    Short(u32),
    #[component(name = "long")]
    Long(u64),
    #[component(name = "anon")]
    Anon,
}

fn db_kind() -> WastDb {
    WastDb {
        funcs: vec![WastFuncRow {
            uid: "kind".into(),
            func: WastFunc {
                source: FuncSource::Exported("kind".into()),
                params: vec![("i".into(), "id".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchVariant {
                    value: Box::new(Instruction::LocalGet { uid: "i".into() }),
                    arms: vec![
                        MatchArm {
                            case: "short".into(),
                            binding: Some("x".into()),
                            body: vec![Instruction::LocalGet { uid: "x".into() }],
                        },
                        MatchArm {
                            case: "long".into(),
                            binding: Some("y".into()),
                            body: vec![Instruction::Const { value: 1 }],
                        },
                        MatchArm {
                            case: "anon".into(),
                            binding: None,
                            body: vec![Instruction::Const { value: 2 }],
                        },
                    ],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "id".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("id".into()),
                definition: WitType::Variant(vec![
                    ("short".into(), Some("u32".into())),
                    ("long".into(), Some("u64".into())),
                    ("anon".into(), None),
                ]),
            },
        }],
    }
}

#[test]
fn match_variant_hetero_short_narrows() {
    let db = db_kind();
    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Id,), (u32,)>(&mut store, "kind")
        .unwrap();

    assert_eq!(func.call(&mut store, (Id::Short(42),)).unwrap(), (42u32,));
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Id::Short(u32::MAX),)).unwrap(),
        (u32::MAX,)
    );
    func.post_return(&mut store).unwrap();
}

#[test]
fn match_variant_hetero_long_and_anon() {
    let db = db_kind();
    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Id,), (u32,)>(&mut store, "kind")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Id::Long(1_000_000_000_000u64),))
            .unwrap(),
        (1u32,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Id::Long(u64::MAX),)).unwrap(),
        (1u32,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Id::Anon,)).unwrap(), (2u32,));
    func.post_return(&mut store).unwrap();
}
