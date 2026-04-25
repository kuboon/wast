//! v0.35: mixed-width joined-slot LocalGet copy via per-case disc branch.
//! When a heterogeneous variant joins to slots with different widths
//! (e.g. `result<u64, string>` joined as `[i64, i32]`), the unconditional
//! uniform-width copy from v0.33 doesn't apply — `i64.store` of slot 0
//! would overwrite bytes that the err-case's `len` field needs. The new
//! disc-branched path emits a per-case write that uses each case's
//! natural memory layout.

use wasmtime::component::{Component, ComponentType, Lift, Linker, Lower};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[derive(ComponentType, Lower, Lift, Clone, Debug, PartialEq)]
#[component(record)]
struct Outcome {
    #[component(name = "outcome")]
    outcome: Result<u64, String>,
    #[component(name = "tag")]
    tag: u32,
}

#[test]
fn record_with_result_u64_string_field_from_param() {
    // wrap(r: result<u64, string>, t: u32) -> outcome
    //   { outcome: r, tag: t }
    //
    // result<u64, string>:
    //   joined flat = [i64, i32]   (mixed widths!)
    //   memory: payload region max(8, 8) bytes at offset 8 (max align 8)
    //     ok=u64:    u64@8 (8 bytes)
    //     err=string: ptr@8, len@12 (each 4 bytes)
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "wrap".into(),
            func: WastFunc {
                source: FuncSource::Exported("wrap".into()),
                params: vec![
                    ("r".into(), "res-u64-string".into()),
                    ("t".into(), "u32".into()),
                ],
                result: Some("outcome-rec".into()),
                body: Some(serialize_body(&[Instruction::RecordLiteral {
                    fields: vec![
                        ("outcome".into(), Instruction::LocalGet { uid: "r".into() }),
                        ("tag".into(), Instruction::LocalGet { uid: "t".into() }),
                    ],
                }])),
            },
        }],
        types: vec![
            WastTypeRow {
                uid: "res-u64-string".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("res-u64-string".into()),
                    definition: WitType::Result("u64".into(), "string".into()),
                },
            },
            WastTypeRow {
                uid: "outcome-rec".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("outcome-rec".into()),
                    definition: WitType::Record(vec![
                        ("outcome".into(), "res-u64-string".into()),
                        ("tag".into(), "u32".into()),
                    ]),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<u64, &str>, u32), (Outcome,)>(&mut store, "wrap")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Ok(1_000_000_000_000u64), 1))
            .unwrap(),
        (Outcome {
            outcome: Ok(1_000_000_000_000u64),
            tag: 1,
        },)
    );
    func.post_return(&mut store).unwrap();

    assert_eq!(
        func.call(&mut store, (Ok(u64::MAX), 9)).unwrap(),
        (Outcome {
            outcome: Ok(u64::MAX),
            tag: 9,
        },)
    );
    func.post_return(&mut store).unwrap();

    assert_eq!(
        func.call(&mut store, (Err("oops"), 7)).unwrap(),
        (Outcome {
            outcome: Err("oops".into()),
            tag: 7,
        },)
    );
    func.post_return(&mut store).unwrap();

    assert_eq!(
        func.call(&mut store, (Err(""), 0)).unwrap(),
        (Outcome {
            outcome: Err("".into()),
            tag: 0,
        },)
    );
    func.post_return(&mut store).unwrap();
}
