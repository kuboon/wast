//! v0.29: heterogeneous `result<T, E>` where T and E have different core
//! types. MatchResult's previous "ok/err must share the same core type"
//! limit is lifted for i32/i64. The wider-typed binding is populated from
//! the joined flat slot; the narrower-typed binding is written inside its
//! own case branch via `i32.wrap_i64`.

use wasmtime::component::{Component, Linker};
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

/// result<u32, u64>: ok is the narrower case (i32), err is the wider
/// (i64, matching the joined flat payload slot).
///
///   classify(r: result<u32, u64>) -> u32
///     match r {
///       ok(v)  -> v       // narrower binding read
///       err(_) -> 999     // wider binding declared but unused — we only
///                         // need the branch to run
///     }
fn db_classify() -> WastDb {
    WastDb {
        funcs: vec![WastFuncRow {
            uid: "classify".into(),
            func: WastFunc {
                source: FuncSource::Exported("classify".into()),
                params: vec![("r".into(), "res_u32_u64".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::MatchResult {
                    value: Box::new(Instruction::LocalGet { uid: "r".into() }),
                    ok_binding: "v".into(),
                    ok_body: vec![Instruction::LocalGet { uid: "v".into() }],
                    err_binding: "e".into(),
                    err_body: vec![Instruction::Const { value: 999 }],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32_u64".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32_u64".into()),
                definition: WitType::Result("u32".into(), "u64".into()),
            },
        }],
    }
}

#[test]
fn classify_ok_narrows_from_i64() {
    // Ok(42u32) → 42. Exercises the narrow path: joined i64 set into
    // err_binding (i64), then the ok branch reads err_binding, wraps to
    // i32, and sets ok_binding before reading it.
    let db = db_classify();
    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<u32, u64>,), (u32,)>(&mut store, "classify")
        .unwrap();

    assert_eq!(func.call(&mut store, (Ok(42),)).unwrap(), (42u32,));
    func.post_return(&mut store).unwrap();
    // Also at u32::MAX to confirm wrap_i64 preserves the low 32 bits.
    assert_eq!(func.call(&mut store, (Ok(u32::MAX),)).unwrap(), (u32::MAX,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn classify_err_preserves_wide_payload() {
    // Err(big) → 999. The large u64 must not crash the guest as the
    // joined flat slot passes through unchanged to err_binding.
    let db = db_classify();
    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<u32, u64>,), (u32,)>(&mut store, "classify")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Err(1_000_000_000_000u64),)).unwrap(),
        (999u32,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Err(u64::MAX),)).unwrap(), (999u32,));
    func.post_return(&mut store).unwrap();
}
