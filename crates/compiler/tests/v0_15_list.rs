//! v0.15 test: `list<T>` param + return + `ListLen` IR.
//! Same Canonical-ABI flat layout as string — `(ptr, len)` where `len` is
//! the element count (not byte count). We reuse the v0.14 ptr+len return
//! wrap for list return.

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

#[test]
fn list_u32_len() {
    // len-of(xs: list<u32>) -> u32  { ListLen(xs) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "len_of".into(),
            func: WastFunc {
                source: FuncSource::Exported("len-of".into()),
                params: vec![("xs".into(), "list_u32".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::ListLen {
                    value: Box::new(Instruction::LocalGet { uid: "xs".into() }),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "list_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("list_u32".into()),
                definition: WitType::List("u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&[u32],), (u32,)>(&mut store, "len-of")
        .unwrap();

    assert_eq!(func.call(&mut store, (&[][..],)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
    assert_eq!(
        func.call(&mut store, (&[1u32, 2, 3, 4, 5][..],)).unwrap(),
        (5,)
    );
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, (&[0u32; 100][..],)).unwrap(), (100,));
    func.post_return(&mut store).unwrap();
}

#[test]
fn echo_list_passthrough() {
    // echo-list(xs: list<u32>) -> list<u32>  { xs }
    //
    // Host writes element bytes into our memory via cabi_realloc before
    // the call; we just copy (ptr, len) to the return area.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "echo_list".into(),
            func: WastFunc {
                source: FuncSource::Exported("echo-list".into()),
                params: vec![("xs".into(), "list_u32".into())],
                result: Some("list_u32".into()),
                body: Some(serialize_body(&[Instruction::LocalGet {
                    uid: "xs".into(),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "list_u32".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("list_u32".into()),
                definition: WitType::List("u32".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&[u32],), (Vec<u32>,)>(&mut store, "echo-list")
        .unwrap();

    let input = vec![10u32, 20, 30, 42, 999];
    let (result,) = func.call(&mut store, (&input[..],)).unwrap();
    assert_eq!(result, input);
    func.post_return(&mut store).unwrap();

    let empty: Vec<u32> = vec![];
    let (result,) = func.call(&mut store, (&empty[..],)).unwrap();
    assert_eq!(result, empty);
    func.post_return(&mut store).unwrap();
}

#[test]
fn list_i64_roundtrip() {
    // Wider element type — verify 8-byte-aligned list<i64> round-trips.
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "echo_i64".into(),
            func: WastFunc {
                source: FuncSource::Exported("echo-i64".into()),
                params: vec![("xs".into(), "list_i64".into())],
                result: Some("list_i64".into()),
                body: Some(serialize_body(&[Instruction::LocalGet {
                    uid: "xs".into(),
                }])),
            },
        }],
        types: vec![WastTypeRow {
            uid: "list_i64".into(),
            def: WastTypeDef {
                source: TypeSource::Internal("list_i64".into()),
                definition: WitType::List("i64".into()),
            },
        }],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&[i64],), (Vec<i64>,)>(&mut store, "echo-i64")
        .unwrap();
    let input = vec![-1i64, 0, 1, i64::MAX, i64::MIN];
    let (result,) = func.call(&mut store, (&input[..],)).unwrap();
    assert_eq!(result, input);
    func.post_return(&mut store).unwrap();
}
