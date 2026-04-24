//! v0.19 char-primitive roundtrip. Canonical ABI encodes char as a 4-byte
//! Unicode scalar value (core i32). Already a primitive in wast-types so
//! this test mostly verifies nothing else in the pipeline short-circuits on
//! 'char'.

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wast_pattern_analyzer::{Instruction, serialize_body};
use wast_types::{FuncSource, WastDb, WastFunc, WastFuncRow};

fn load(db: &WastDb) -> (Engine, Component) {
    let wasm = wast_compiler::compile(db, "").expect("compile ok");
    let engine = Engine::new(&Config::new()).unwrap();
    let component = Component::from_binary(&engine, &wasm).expect("component load");
    (engine, component)
}

#[test]
fn echo_char() {
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "echo".into(),
            func: WastFunc {
                source: FuncSource::Exported("echo".into()),
                params: vec![("c".into(), "char".into())],
                result: Some("char".into()),
                body: Some(serialize_body(&[Instruction::LocalGet { uid: "c".into() }])),
            },
        }],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(char,), (char,)>(&mut store, "echo")
        .unwrap();

    for c in ['a', 'Z', '0', '\n', 'あ', '😀'] {
        assert_eq!(func.call(&mut store, (c,)).unwrap(), (c,));
        func.post_return(&mut store).unwrap();
    }
}
