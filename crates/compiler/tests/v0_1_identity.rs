//! v0.1 test: compile a `u32 -> u32` identity component and verify via
//! wasmtime that `identity(42)` returns `42`.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

const WORLD_WIT: &str = r#"package example:foo@0.1.0;

world t {
  export identity: func(x: u32) -> u32;
}
"#;

fn identity_db() -> WastDb {
    let body = serialize_body(&[
        Instruction::LocalGet { uid: "x".into() },
        Instruction::Return,
    ]);
    WastDb {
        funcs: vec![WastFuncRow {
            uid: "identity".into(),
            func: WastFunc {
                source: FuncSource::Exported("identity".into()),
                params: vec![("x".into(), "u32".into())],
                result: Some("u32".into()),
                body: Some(body),
            },
        }],
        types: vec![],
    }
}

#[test]
fn identity_u32_roundtrip() {
    let db = identity_db();
    let wasm = wast_compiler::compile(&db, WORLD_WIT).expect("compile should succeed");

    let config = Config::new();
    let engine = Engine::new(&config).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component must load");

    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker
        .instantiate(&mut store, &component)
        .expect("instantiate");

    let func = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "identity")
        .expect("typed func lookup");

    let (result,) = func.call(&mut store, (42,)).expect("call");
    assert_eq!(result, 42);
}
