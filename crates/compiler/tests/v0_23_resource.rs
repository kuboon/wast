//! v0.23: WIT resource types. End-to-end — declare a `counter` resource
//! with a constructor that stores a u32 rep and a `get` method that returns
//! it, then verify via `wasmtime::component::bindgen!` using a fixed WIT
//! that matches our synthesizer's output.

use std::sync::{Arc, Mutex};

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

#[derive(Default)]
struct HostState {
    dropped: Arc<Mutex<Vec<u32>>>,
}

impl GeneratedImports for HostState {
    fn record_drop(&mut self, rep: u32) {
        self.dropped.lock().unwrap().push(rep);
    }
}

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn counter_db() -> WastDb {
    // world generated {
    //   import record-drop: func(rep: u32);
    //   export generated-iface;  // resource counter { ctor; get; zero; } + [dtor]
    // }
    WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "record_drop".into(),
                func: WastFunc {
                    source: FuncSource::Imported("record-drop".into()),
                    params: vec![("rep".into(), "u32".into())],
                    result: None,
                    body: None,
                },
            },
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
            // Static factory: `zero: static func() -> counter`. No `self`;
            // the body constructs a brand-new handle with rep=0.
            WastFuncRow {
                uid: "counter_zero".into(),
                func: WastFunc {
                    source: FuncSource::Exported("[static]counter.zero".into()),
                    params: vec![],
                    result: Some("own_counter".into()),
                    body: Some(serialize_body(&[Instruction::ResourceNew {
                        resource: "counter".into(),
                        rep: Box::new(Instruction::Const { value: 0 }),
                    }])),
                },
            },
            // Custom destructor: implicit in WIT (no member syntax). Core
            // export `[dtor]counter` receives the rep (not the handle) and
            // forwards it to the imported `record-drop` so the host can
            // observe each drop.
            WastFuncRow {
                uid: "counter_dtor".into(),
                func: WastFunc {
                    source: FuncSource::Exported("[dtor]counter".into()),
                    params: vec![("rep".into(), "u32".into())],
                    result: None,
                    body: Some(serialize_body(&[Instruction::Call {
                        // FuncMap keys by source-inner name, not row uid.
                        func_uid: "record-drop".into(),
                        args: vec![("rep".into(), Instruction::LocalGet { uid: "rep".into() })],
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
    }
}

fn setup(db: &WastDb) -> (Store<HostState>, Generated) {
    let (engine, component) = load(db);
    let mut linker: Linker<HostState> = Linker::new(&engine);
    Generated::add_to_linker(&mut linker, |s: &mut HostState| s).unwrap();
    let mut store = Store::new(&engine, HostState::default());
    let generated = Generated::instantiate(&mut store, &component, &linker).unwrap();
    (store, generated)
}

#[test]
fn resource_constructor_and_method() {
    let db = counter_db();
    let (mut store, generated) = setup(&db);
    let counter = generated.wast_generated_generated_iface().counter();

    let handle: wasmtime::component::ResourceAny =
        counter.call_constructor(&mut store, 42).unwrap();
    let v = counter.call_get(&mut store, handle).unwrap();
    assert_eq!(v, 42);
    handle.resource_drop(&mut store).unwrap();
}

#[test]
fn resource_static_factory_method() {
    // `zero: static func() -> counter` — no self, returns a fresh counter
    // with rep=0. Verify the returned handle's get() observes 0.
    let db = counter_db();
    let (mut store, generated) = setup(&db);
    let counter = generated.wast_generated_generated_iface().counter();

    let handle: wasmtime::component::ResourceAny = counter.call_zero(&mut store).unwrap();
    let v = counter.call_get(&mut store, handle).unwrap();
    assert_eq!(v, 0);
    handle.resource_drop(&mut store).unwrap();
}

#[test]
fn resource_dtor_observes_drops() {
    // Dtor forwards every dropped rep to the imported `record-drop` host
    // func. Construct three counters with distinct reps, drop them in any
    // order, then assert the host received exactly that multiset of reps.
    let db = counter_db();
    let (mut store, generated) = setup(&db);
    let log = store.data().dropped.clone();
    let counter = generated.wast_generated_generated_iface().counter();

    let h1 = counter.call_constructor(&mut store, 11).unwrap();
    let h2 = counter.call_constructor(&mut store, 22).unwrap();
    let h3 = counter.call_zero(&mut store).unwrap();
    assert!(log.lock().unwrap().is_empty(), "no drops yet");

    h2.resource_drop(&mut store).unwrap();
    h1.resource_drop(&mut store).unwrap();
    h3.resource_drop(&mut store).unwrap();

    let mut observed = log.lock().unwrap().clone();
    observed.sort();
    assert_eq!(observed, vec![0, 11, 22]);
}
