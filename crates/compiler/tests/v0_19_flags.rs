//! v0.19 flags: bitmask of declared flag names. Up to 32 flags fit in a
//! single i32; 33-64 in i64 (>64 deferred). FlagsCtor emits a compile-time
//! i32.const with the OR'd bit pattern.

use wasmtime::component::{Component, Linker, flags};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{
    FuncSource, TypeSource, WastDb, WastFunc, WastFuncRow, WastTypeDef, WastTypeRow, WitType,
};

flags! {
    Perms {
        #[component(name = "read")]
        const READ;
        #[component(name = "write")]
        const WRITE;
        #[component(name = "execute")]
        const EXECUTE;
    }
}

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

fn perms_type_row() -> WastTypeRow {
    WastTypeRow {
        uid: "perms".into(),
        def: WastTypeDef {
            source: TypeSource::Internal("perms".into()),
            definition: WitType::Flags(vec!["read".into(), "write".into(), "execute".into()]),
        },
    }
}

#[test]
fn flags_passthrough() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "echo".into(),
            func: WastFunc {
                source: FuncSource::Exported("echo".into()),
                params: vec![("p".into(), "perms".into())],
                result: Some("perms".into()),
                body: Some(serialize_body(&[Instruction::LocalGet { uid: "p".into() }])),
            },
        }],
        types: vec![perms_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(Perms,), (Perms,)>(&mut store, "echo")
        .unwrap();
    let p = Perms::READ | Perms::WRITE;
    assert_eq!(func.call(&mut store, (p,)).unwrap(), (p,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn flags_ctor_literal() {
    // read-write() -> perms  { { read, write } }
    // FlagsCtor folds at compile time to `i32.const 0b011` (bits 0|1).
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "read_write".into(),
            func: WastFunc {
                source: FuncSource::Exported("read-write".into()),
                params: vec![],
                result: Some("perms".into()),
                body: Some(serialize_body(&[Instruction::FlagsCtor {
                    flags: vec!["read".into(), "write".into()],
                }])),
            },
        }],
        types: vec![perms_type_row()],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(), (Perms,)>(&mut store, "read-write")
        .unwrap();
    let got = func.call(&mut store, ()).unwrap().0;
    assert_eq!(got, Perms::READ | Perms::WRITE);
    func.post_return(&mut store).unwrap();
}
