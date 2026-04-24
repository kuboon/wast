//! v0.23: WIT resource types. End-to-end — declare a `counter` resource
//! with a constructor that stores a u32 rep and a `get` method that returns
//! it, then verify via `wasmtime::component::bindgen!` using a fixed WIT
//! that matches our synthesizer's output.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

wasmtime::component::bindgen!({
    path: "tests/wit",
    world: "generated",
});

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn resource_constructor_and_method() {
    // resource counter {
    //   constructor(init: u32);
    //   get: func() -> u32;
    // }
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "counter_ctor".into(),
                func: WastFunc {
                    source: FuncSource::Exported("[constructor]counter".into()),
                    params: vec![("init".into(), "u32".into())],
                    result: Some("own_counter".into()),
                    body: Some(serialize_body(&[Instruction::ResourceNew {
                        resource: "counter".into(),
                        rep: Box::new(Instruction::LocalGet { uid: "init".into() }),
                    }])),
                },
            },
            WastFuncRow {
                uid: "counter_get".into(),
                func: WastFunc {
                    source: FuncSource::Exported("[method]counter.get".into()),
                    params: vec![("self_".into(), "borrow_counter".into())],
                    result: Some("u32".into()),
                    // When wasmtime lowers a borrow<R> into a component that
                    // owns R, it passes the rep directly instead of a fresh
                    // handle index — so `self_` IS the rep. Just return it.
                    body: Some(serialize_body(&[Instruction::LocalGet {
                        uid: "self_".into(),
                    }])),
                },
            },
        ],
        types: vec![
            WastTypeRow {
                uid: "counter".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("counter".into()),
                    definition: WitType::Resource,
                },
            },
            WastTypeRow {
                uid: "own_counter".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("own_counter".into()),
                    definition: WitType::Own("counter".into()),
                },
            },
            WastTypeRow {
                uid: "borrow_counter".into(),
                def: WastTypeDef {
                    source: TypeSource::Internal("borrow_counter".into()),
                    definition: WitType::Borrow("counter".into()),
                },
            },
        ],
    };

    let (engine, component) = load(&db);
    let mut linker: Linker<()> = Linker::new(&engine);
    // bindgen! generates add_to_linker, but only for the import side; exports
    // don't need linker plumbing beyond instantiation.
    let _ = &mut linker;
    let mut store = Store::new(&engine, ());
    let generated = Generated::instantiate(&mut store, &component, &linker).unwrap();
    let iface = generated.wast_generated_generated_iface();
    let counter = iface.counter();

    let handle: wasmtime::component::ResourceAny =
        counter.call_constructor(&mut store, 42).unwrap();
    let v = counter.call_get(&mut store, handle).unwrap();
    assert_eq!(v, 42);
    handle.resource_drop(&mut store).unwrap();
}
