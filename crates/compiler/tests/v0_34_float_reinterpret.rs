//! v0.34: float reinterpret narrows. `result<u32, f32>` joins to a single
//! `i32` slot per the Canonical ABI rule `join(i32, f32) = i32` — the f32
//! case's bit pattern rides in the i32. Lifting the f32 binding requires
//! `f32.reinterpret_i32`.

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

fn db_classify_f32() -> WastDb {
    // classify(r: result<u32, f32>) -> f32
    //   match r {
    //     ok(_)  -> 0.0,
    //     err(v) -> v,        // f32 binding read; needs f32.reinterpret_i32
    //   }
    WastDb {
        funcs: vec![WastFuncRow {
            uid: "classify".into(),
            func: WastFunc {
                source: FuncSource::Exported("classify".into()),
                params: vec![("r".into(), "res_u32_f32".into())],
                result: Some("f32".into()),
                body: Some(serialize_body(&[Instruction::MatchResult {
                    value: Box::new(Instruction::LocalGet { uid: "r".into() }),
                    ok_binding: "v".into(),
                    ok_body: vec![Instruction::Const { value: 0 }],
                    err_binding: "f".into(),
                    err_body: vec![Instruction::LocalGet { uid: "f".into() }],
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32_f32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32_f32".into()),
                definition: WitType::Result("u32".into(), "f32".into()),
            },
        }],
    }
}

#[test]
fn classify_err_f32_reinterprets_from_i32() {
    let db = db_classify_f32();
    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<u32, f32>,), (f32,)>(&mut store, "classify")
        .unwrap();

    assert_eq!(
        func.call(&mut store, (Err(3.14_f32),)).unwrap(),
        (3.14_f32,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (Err(f32::NEG_INFINITY),)).unwrap(),
        (f32::NEG_INFINITY,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Ok(42u32),)).unwrap(), (0.0_f32,));
    func.post_return(&mut store).unwrap();
}
