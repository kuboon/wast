//! v0.5 test: `Call` to an imported component func. The component declares
//! `(import "bump" …)`, `canon lower`s it to a core func, and an exported
//! `forward(n)` invokes it via `call $bump`. The wasmtime linker supplies
//! a host closure that records the observed arg so the test can assert.

use std::sync::{Arc, Mutex};

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

struct Ctx {
    observed: Arc<Mutex<u32>>,
}

#[test]
fn exported_forwards_to_imported() {
    let db = WastDb {
        funcs: vec![
            WastFuncRow {
                uid: "bump".into(),
                func: WastFunc {
                    source: FuncSource::Imported("bump".into()),
                    params: vec![("delta".into(), "u32".into())],
                    result: None,
                    body: None,
                },
            },
            WastFuncRow {
                uid: "forward".into(),
                func: WastFunc {
                    source: FuncSource::Exported("forward".into()),
                    params: vec![("n".into(), "u32".into())],
                    result: None,
                    body: Some(serialize_body(&[Instruction::Call {
                        func_uid: "bump".into(),
                        args: vec![("delta".into(), Instruction::LocalGet { uid: "n".into() })],
                    }])),
                },
            },
        ],
        types: vec![],
    };

    let wasm = wast_compiler::compile(&db, "").expect("compile ok");

    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");

    let observed = Arc::new(Mutex::new(0u32));
    let ctx = Ctx {
        observed: observed.clone(),
    };

    let mut linker: Linker<Ctx> = Linker::new(&engine);
    linker
        .root()
        .func_wrap(
            "bump",
            |store: wasmtime::StoreContextMut<Ctx>, (delta,): (u32,)| {
                *store.data().observed.lock().unwrap() = delta;
                Ok(())
            },
        )
        .expect("bind bump");

    let mut store = Store::new(&engine, ctx);
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(u32,), ()>(&mut store, "forward")
        .unwrap();
    func.call(&mut store, (42,)).unwrap();
    func.post_return(&mut store).unwrap();

    assert_eq!(*observed.lock().unwrap(), 42);
}
