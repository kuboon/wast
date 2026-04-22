//! v0.6 test: `option<T>` / `result<T, E>` in param position with `IsErr`.
//! Full option/result return (with payload) needs `cabi_realloc` and will
//! land with v0.7.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

fn compile_component(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn is_err_on_result_u32_u32() {
    // exported `check-err(r: result<u32, u32>) -> bool  { is_err(r) }`
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "check_err".into(),
            func: WastFunc {
                source: FuncSource::Exported("check-err".into()),
                params: vec![("r".into(), "res_u32".into())],
                result: Some("bool".into()),
                body: Some(serialize_body(&[Instruction::IsErr {
                    value: Box::new(Instruction::LocalGet { uid: "r".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "res_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("res_u32".into()),
                definition: WitType::Result("u32".into(), "u32".into()),
            },
        }],
    };

    let (engine, component) = compile_component(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Result<u32, u32>,), (bool,)>(&mut store, "check-err")
        .unwrap();

    assert_eq!(func.call(&mut store, (Ok(42),)).unwrap(), (false,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (Err(7),)).unwrap(), (true,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn option_u32_param_passes_through_signature() {
    // exported `has-opt(o: option<u32>) -> bool` — we can't MatchOption yet
    // (needs synthesized locals), but at minimum verify the option param
    // flattens correctly: return `true` unconditionally and observe that
    // wasmtime accepts `Some(x)` / `None` calls.
    //
    // Body: push i32.const 1 (true).
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "has_opt".into(),
            func: WastFunc {
                source: FuncSource::Exported("has-opt".into()),
                params: vec![("o".into(), "opt_u32".into())],
                result: Some("bool".into()),
                body: Some(serialize_body(&[Instruction::Const { value: 1 }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "opt_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("opt_u32".into()),
                definition: WitType::Option("u32".into()),
            },
        }],
    };

    let (engine, component) = compile_component(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Option<u32>,), (bool,)>(&mut store, "has-opt")
        .unwrap();

    assert_eq!(func.call(&mut store, (Some(42),)).unwrap(), (true,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (None,)).unwrap(), (true,));
    func.post_return(&mut store).unwrap();
}
