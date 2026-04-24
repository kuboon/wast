//! v0.28: imported resource. The host declares a `counter` resource via an
//! imported interface; our compiled component calls the imported
//! constructor/get/drop and exposes a `roundtrip` func that round-trips a
//! u32 through the host-owned counter.

use std::sync::{Arc, Mutex};

use wasmtime::component::{Component, Linker, Resource, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

wasmtime::component::bindgen!({
    path: "tests/wit/imported",
    world: "generated",
    with: {
        "wast:generated/imported-iface/counter": HostCounter,
    },
});

// The rep stored on the host side for each `counter` handle. Every host
// resource in wasmtime needs a type it can anchor on; the bindgen `with:`
// clause above wires "wast:generated/imported-iface/counter" to this one.
pub struct HostCounter {
    value: u32,
}

struct HostState {
    table: ResourceTable,
    // Running log of the rep passed to each drop — purely for assertion.
    drops: Arc<Mutex<Vec<u32>>>,
}

impl wast::generated::imported_iface::Host for HostState {}

impl wast::generated::imported_iface::HostCounter for HostState {
    fn new(&mut self, init: u32) -> Resource<HostCounter> {
        self.table.push(HostCounter { value: init }).unwrap()
    }

    fn get(&mut self, self_: Resource<HostCounter>) -> u32 {
        let c = self.table.get(&self_).unwrap();
        c.value
    }

    fn drop(&mut self, rep: Resource<HostCounter>) -> wasmtime::Result<()> {
        let c = self.table.delete(rep)?;
        self.drops.lock().unwrap().push(c.value);
        Ok(())
    }
}

fn build_db() -> WastDb {
    // world generated {
    //   import imported-iface;  // resource counter { ctor; get; }
    //   export roundtrip: func(n: u32) -> u32;
    // }
    //
    // roundtrip(n) body:
    //   let h = [constructor]counter(init: n)  // own<counter>
    //   let v = [method]counter.get(self_: h)  // u32 (via borrow)
    //   ResourceDrop(counter, h)
    //   v
    WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "ctr_ctor".into(),
                func: WastFunc {
                    source: FuncSource::Imported("[constructor]counter".into()),
                    params: vec![("init".into(), "u32".into())],
                    result: Some("own_counter".into()),
                    body: None,
                },
            },
            WastFuncRow {
                uid: "ctr_get".into(),
                func: WastFunc {
                    source: FuncSource::Imported("[method]counter.get".into()),
                    params: vec![("self_".into(), "borrow_counter".into())],
                    result: Some("u32".into()),
                    body: None,
                },
            },
            WastFuncRow {
                uid: "roundtrip".into(),
                func: WastFunc {
                    source: FuncSource::Exported("roundtrip".into()),
                    params: vec![("n".into(), "u32".into())],
                    result: Some("u32".into()),
                    body: Some(serialize_body(&[
                        Instruction::LocalSet {
                            uid: "h".into(),
                            value: Box::new(Instruction::Call {
                                func_uid: "[constructor]counter".into(),
                                args: vec![(
                                    "init".into(),
                                    Instruction::LocalGet { uid: "n".into() },
                                )],
                            }),
                        },
                        Instruction::LocalSet {
                            uid: "v".into(),
                            value: Box::new(Instruction::Call {
                                func_uid: "[method]counter.get".into(),
                                args: vec![(
                                    "self_".into(),
                                    Instruction::LocalGet { uid: "h".into() },
                                )],
                            }),
                        },
                        Instruction::ResourceDrop {
                            resource: "counter".into(),
                            handle: Box::new(Instruction::LocalGet { uid: "h".into() }),
                        },
                        Instruction::LocalGet { uid: "v".into() },
                    ])),
                },
            },
        ],
        types: vec![
            WastTypeRow {
                uid: "counter".into(),
                def: WastTypeDef {
                    source: TypeSource::Imported("counter".into()),
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

#[test]
fn imported_resource_roundtrip() {
    let db = build_db();
    let wasm = wast_compiler::compile(&db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");

    let mut linker: Linker<HostState> = Linker::new(&engine);
    Generated::add_to_linker(&mut linker, |s: &mut HostState| s).unwrap();
    let drops = Arc::new(Mutex::new(Vec::new()));
    let mut store = Store::new(
        &engine,
        HostState {
            table: ResourceTable::new(),
            drops: drops.clone(),
        },
    );
    let generated = Generated::instantiate(&mut store, &component, &linker).unwrap();

    let v = generated.call_roundtrip(&mut store, 99).unwrap();
    assert_eq!(v, 99);
    // The host-observed drop records the rep (the value).
    assert_eq!(drops.lock().unwrap().as_slice(), &[99]);

    let v = generated.call_roundtrip(&mut store, 7).unwrap();
    assert_eq!(v, 7);
    assert_eq!(drops.lock().unwrap().as_slice(), &[99, 7]);
}
