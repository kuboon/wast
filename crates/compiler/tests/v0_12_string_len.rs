//! v0.12 test: `string` param + `StringLen` IR.
//! Canonical-ABI flat layout for string is `(ptr i32, len i32)` and the host
//! has already written utf-8 bytes into our memory before the body runs.
//! `StringLen` on a LocalGet of a string param reads just the `len` slot.

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
fn strlen_of_string_param() {
    // strlen(s: string) -> u32  { StringLen(s) }
    let db = WastDb {
        funcs: vec![WastFuncRow {
            uid: "strlen".into(),
            func: WastFunc {
                source: FuncSource::Exported("strlen".into()),
                params: vec![("s".into(), "string".into())],
                result: Some("u32".into()),
                body: Some(serialize_body(&[Instruction::StringLen {
                    value: Box::new(Instruction::LocalGet { uid: "s".into() }),
                }])),
            },
        }],
        types: vec![],
    };

    let (engine, component) = load(&db);
    let linker: Linker<()> = Linker::new(&engine);
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let func = instance
        .get_typed_func::<(&str,), (u32,)>(&mut store, "strlen")
        .unwrap();

    assert_eq!(func.call(&mut store, ("hello",)).unwrap(), (5,));
    func.post_return(&mut store).unwrap();
    assert_eq!(func.call(&mut store, ("",)).unwrap(), (0,));
    func.post_return(&mut store).unwrap();
    // utf-8 byte length (not code points): 'あ' is 3 bytes in UTF-8.
    assert_eq!(func.call(&mut store, ("あいう",)).unwrap(), (9,));
    func.post_return(&mut store).unwrap();
}
